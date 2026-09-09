use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use mmorpg_ui_contract::{
    Generation, Handle, MAX_TEXT_BYTES, NodeId, PackageId, UiLimits, UiOperation,
    validate_operations,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode};

const FUEL_BUDGET: u64 = 100_000;
const MAX_LINEAR_MEMORY_PAGES: u32 = 4;
const WASM_PAGE_BYTES: usize = 65_536;
const ABI_OK: i32 = 0;
const ABI_INVALID_MEMORY: i32 = -1;
const ABI_INVALID_UTF8: i32 = -2;
const ABI_TEXT_TOO_LARGE: i32 = -3;
const MAX_PACKAGE_SOURCE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
struct GuestPackage {
    package_id: PackageId,
    source: String,
}

#[derive(Deserialize)]
struct PackageManifest {
    package_id: u64,
    entry: String,
    integrity_sha256: String,
}

struct HostState {
    limits: StoreLimits,
    operations: Vec<UiOperation>,
    next_node: u64,
    package_id: PackageId,
}

impl HostState {
    fn new(package_id: PackageId) -> Self {
        Self {
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_LINEAR_MEMORY_PAGES as usize * WASM_PAGE_BYTES)
                .memories(1)
                .instances(1)
                .tables(1)
                .trap_on_grow_failure(true)
                .build(),
            operations: Vec::new(),
            next_node: 1,
            package_id,
        }
    }
}

fn engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

fn compile(engine: &Engine, source: &str) -> Module {
    let bytes = wat::parse_str(source).expect("comparison WAT must parse");
    let module = Module::new(engine, &bytes).expect("comparison module must validate");
    for import in module.imports() {
        assert_eq!(
            (import.module(), import.name()),
            ("ui", "create_panel"),
            "comparison module imported a non-allowlisted host symbol"
        );
    }
    module
}

fn bounded_store(engine: &Engine) -> Store<HostState> {
    bounded_store_for_package(
        engine,
        PackageId::new(7).expect("fixed package ID is non-zero"),
    )
}

fn bounded_store_for_package(engine: &Engine, package_id: PackageId) -> Store<HostState> {
    let mut store = Store::new(engine, HostState::new(package_id));
    store.limiter(|state| &mut state.limits);
    store
}

fn define_ui_imports(linker: &mut Linker<HostState>) {
    linker
        .func_wrap(
            "ui",
            "create_panel",
            |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 || len as usize > MAX_TEXT_BYTES {
                    return ABI_TEXT_TOO_LARGE;
                }
                let Some(wasmi::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return ABI_INVALID_MEMORY;
                };
                let mut bytes = vec![0; len as usize];
                if memory.read(&caller, ptr as usize, &mut bytes).is_err() {
                    return ABI_INVALID_MEMORY;
                }
                let Ok(text) = String::from_utf8(bytes) else {
                    return ABI_INVALID_UTF8;
                };
                let Some(node) = NodeId::new(caller.data().next_node) else {
                    return ABI_INVALID_MEMORY;
                };
                caller.data_mut().next_node += 1;
                let handle = Handle {
                    node,
                    owner: caller.data().package_id,
                    generation: Generation::new(1).expect("fixed generation is non-zero"),
                };
                caller
                    .data_mut()
                    .operations
                    .push(UiOperation::CreatePanel { handle, text });
                ABI_OK
            },
        )
        .expect("allowlisted UI import must link");
}

const PANEL_MODULE: &str = r#"
    (module
      (import "ui" "create_panel" (func $create_panel (param i32 i32) (result i32)))
      (memory (export "memory") 1 4)
      (data (i32.const 16) "Town")
      (func (export "run")
        i32.const 16
        i32.const 4
        call $create_panel
        drop))
"#;

fn render_panel(source: &str, package_id: PackageId) -> Result<String, String> {
    let engine = engine();
    let module = compile(&engine, source);
    let mut linker = Linker::new(&engine);
    define_ui_imports(&mut linker);
    let mut store = bounded_store_for_package(&engine, package_id);
    store
        .set_fuel(FUEL_BUDGET)
        .map_err(|error| format!("fuel setup failed: {error}"))?;
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|error| format!("guest startup failed: {error}"))?;
    let run = instance
        .get_typed_func::<(), ()>(&store, "run")
        .map_err(|error| format!("guest ABI is missing run: {error}"))?;
    run.call(&mut store, ())
        .map_err(|error| format!("guest execution failed: {error}"))?;
    validate_operations(
        &store.data().operations,
        package_id,
        Generation::new(1).expect("fixed generation is non-zero"),
        &UiLimits::default(),
    )
    .map_err(|error| format!("contract validation failed: {error}"))?;
    match store.data().operations.as_slice() {
        [UiOperation::CreatePanel { handle, text }] => {
            Ok(format!("PANEL\t{}\t{}", handle.node.get(), text))
        }
        _ => Err("guest produced an unexpected operation batch".to_owned()),
    }
}

fn load_package(root: &Path) -> Result<GuestPackage, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize package root: {error}"))?;
    let manifest_path = root.join("manifest.toml");
    let manifest_source = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read package manifest: {error}"))?;
    let manifest: PackageManifest = toml::from_str(&manifest_source)
        .map_err(|error| format!("cannot parse package manifest: {error}"))?;
    let package_id = PackageId::new(manifest.package_id)
        .ok_or_else(|| "package manifest ID must be non-zero".to_owned())?;
    let entry = PathBuf::from(&manifest.entry);
    if entry.is_absolute()
        || entry
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("package entry must remain inside the package root".to_owned());
    }
    let entry_path = root.join(entry);
    let entry_path = entry_path
        .canonicalize()
        .map_err(|error| format!("cannot canonicalize package entry: {error}"))?;
    if !entry_path.starts_with(&root) {
        return Err("package entry escaped the package root".to_owned());
    }
    let source_bytes =
        fs::read(&entry_path).map_err(|error| format!("cannot read package entry: {error}"))?;
    if source_bytes.len() > MAX_PACKAGE_SOURCE_BYTES {
        return Err("package entry exceeds the source limit".to_owned());
    }
    let actual_hash = format!("{:x}", Sha256::digest(&source_bytes));
    if manifest.integrity_sha256 != actual_hash {
        return Err(format!(
            "package integrity mismatch: expected {}, got {actual_hash}",
            manifest.integrity_sha256
        ));
    }
    let source = String::from_utf8(source_bytes)
        .map_err(|_| "package entry must be valid UTF-8 WAT".to_owned())?;
    Ok(GuestPackage { package_id, source })
}

fn process_host(package: GuestPackage) -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(stdout, "READY").map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())?;
    for line in stdin.lock().lines() {
        match line.map_err(|error| error.to_string())?.trim() {
            "render" => {
                let result = render_panel(&package.source, package.package_id)?;
                writeln!(stdout, "{result}").map_err(|error| error.to_string())?;
                stdout.flush().map_err(|error| error.to_string())?;
            }
            "shutdown" => {
                writeln!(stdout, "BYE").map_err(|error| error.to_string())?;
                stdout.flush().map_err(|error| error.to_string())?;
                return Ok(());
            }
            "" => {}
            command => return Err(format!("unknown process-host command '{command}'")),
        }
    }
    Ok(())
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--process-host") {
        let arguments: Vec<String> = std::env::args().skip(2).collect();
        let mut package_root = None;
        let mut index = 0;
        while index < arguments.len() {
            if arguments[index] == "--package-root" {
                if package_root.is_some() || index + 1 >= arguments.len() {
                    eprintln!("--package-root requires exactly one directory");
                    std::process::exit(2);
                }
                package_root = Some(PathBuf::from(&arguments[index + 1]));
                index += 2;
            } else {
                eprintln!("unknown process-host argument '{}'", arguments[index]);
                std::process::exit(2);
            }
        }
        let package = match package_root {
            Some(root) => load_package(&root),
            None => Ok(GuestPackage {
                package_id: PackageId::new(7).expect("fixed package ID is non-zero"),
                source: PANEL_MODULE.to_owned(),
            }),
        };
        if let Err(error) = package.and_then(process_host) {
            eprintln!("process-host error: {error}");
            std::process::exit(1);
        }
        return;
    }
    let engine = engine();
    let module = compile(&engine, PANEL_MODULE);
    let mut linker = Linker::new(&engine);
    define_ui_imports(&mut linker);
    let mut store = bounded_store(&engine);
    store
        .set_fuel(FUEL_BUDGET)
        .expect("fuel must be configured");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("module must instantiate and start");
    let run = instance
        .get_typed_func::<(), ()>(&store, "run")
        .expect("run export must have the fixed ABI");
    run.call(&mut store, ())
        .expect("bounded UI call must succeed");
    validate_operations(
        &store.data().operations,
        PackageId::new(7).expect("fixed package ID is non-zero"),
        Generation::new(1).expect("fixed generation is non-zero"),
        &UiLimits::default(),
    )
    .expect("guest operation must satisfy the language-neutral contract");
    assert!(matches!(
        &store.data().operations[..],
        [UiOperation::CreatePanel { text, .. }] if text == "Town"
    ));

    let loop_module = compile(&engine, r#"(module (func (export "loop") (loop br 0)))"#);
    let mut loop_store = bounded_store(&engine);
    loop_store
        .set_fuel(FUEL_BUDGET)
        .expect("fuel must be configured");
    let loop_instance = linker
        .instantiate_and_start(&mut loop_store, &loop_module)
        .expect("loop module must instantiate and start");
    let loop_run = loop_instance
        .get_typed_func::<(), ()>(&loop_store, "loop")
        .expect("loop export must have the fixed ABI");
    let start = Instant::now();
    let trap = loop_run
        .call(&mut loop_store, ())
        .expect_err("fuel must interrupt an infinite module");
    let elapsed_us = start.elapsed().as_micros();
    assert_eq!(trap.as_trap_code(), Some(TrapCode::OutOfFuel));

    let bounded_memory = compile(
        &engine,
        &format!("(module (memory (export \"memory\") 1 {MAX_LINEAR_MEMORY_PAGES}))"),
    );
    let mut memory_store = bounded_store(&engine);
    let memory_instance = linker
        .instantiate_and_start(&mut memory_store, &bounded_memory)
        .expect("bounded memory module must instantiate and start");
    let memory = memory_instance
        .get_memory(&memory_store, "memory")
        .expect("memory export must exist");
    assert_eq!(
        memory.ty(&memory_store).maximum(),
        Some(u64::from(MAX_LINEAR_MEMORY_PAGES))
    );
    let grow_module = compile(
        &engine,
        &format!(
            "(module (memory (export \"memory\") 1 {MAX_LINEAR_MEMORY_PAGES}) (func (export \"grow\") i32.const 4 memory.grow drop))"
        ),
    );
    let mut grow_store = bounded_store(&engine);
    let grow_instance = linker
        .instantiate_and_start(&mut grow_store, &grow_module)
        .expect("bounded memory module must instantiate");
    let grow = grow_instance
        .get_typed_func::<(), ()>(&grow_store, "grow")
        .expect("grow export must have the fixed ABI");
    assert!(
        grow.call(&mut grow_store, ()).is_err(),
        "host memory limiter must reject growth beyond the configured bytes"
    );

    println!(
        "wasm comparison: allowlisted_import=pass host_memory_limit=pass fuel=out_of_fuel fuel_trap_us={elapsed_us} max_memory_pages={MAX_LINEAR_MEMORY_PAGES} wasi_imports=none"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_proof_runs() {
        main();
    }

    #[test]
    #[should_panic(expected = "comparison module imported a non-allowlisted host symbol")]
    fn rejects_non_allowlisted_imports_before_instantiation() {
        let engine = engine();
        compile(
            &engine,
            r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)))"#,
        );
    }

    #[test]
    fn guest_memory_pointer_errors_are_reported_without_host_panic() {
        let engine = engine();
        let module = compile(
            &engine,
            r#"(module
                (import "ui" "create_panel" (func $create_panel (param i32 i32) (result i32)))
                (memory (export "memory") 1 1)
                (func (export "run") (result i32)
                  i32.const 65535 i32.const 4 call $create_panel))"#,
        );
        let mut linker = Linker::new(&engine);
        define_ui_imports(&mut linker);
        let mut store = bounded_store(&engine);
        store
            .set_fuel(FUEL_BUDGET)
            .expect("fuel must be configured");
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .expect("malformed-pointer module must instantiate");
        let run = instance
            .get_typed_func::<(), i32>(&store, "run")
            .expect("run export must exist");
        assert_eq!(run.call(&mut store, ()).unwrap(), ABI_INVALID_MEMORY);
        assert!(store.data().operations.is_empty());
    }
}
