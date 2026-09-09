use std::time::Instant;

use mmorpg_ui_contract::{
    Generation, Handle, MAX_TEXT_BYTES, NodeId, PackageId, UiLimits, UiOperation,
    validate_operations,
};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode};

const FUEL_BUDGET: u64 = 100_000;
const MAX_LINEAR_MEMORY_PAGES: u32 = 4;
const WASM_PAGE_BYTES: usize = 65_536;
const ABI_OK: i32 = 0;
const ABI_INVALID_MEMORY: i32 = -1;
const ABI_INVALID_UTF8: i32 = -2;
const ABI_TEXT_TOO_LARGE: i32 = -3;

struct HostState {
    limits: StoreLimits,
    operations: Vec<UiOperation>,
    next_node: u64,
}

impl HostState {
    fn new() -> Self {
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
    let mut store = Store::new(engine, HostState::new());
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
                    owner: PackageId::new(7).expect("fixed package ID is non-zero"),
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

fn main() {
    let engine = engine();
    let source = r#"
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
    let module = compile(&engine, source);
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
