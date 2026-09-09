use std::time::Instant;

use wasmi::{Config, Engine, Linker, Module, Store, TrapCode};

const FUEL_BUDGET: u64 = 100_000;
const MAX_LINEAR_MEMORY_PAGES: u32 = 4;

fn engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

fn compile(engine: &Engine, source: &str) -> Module {
    let bytes = wat::parse_str(source).expect("comparison WAT must parse");
    Module::new(engine, &bytes).expect("comparison module must validate")
}

fn main() {
    let engine = engine();
    let source = r#"
        (module
          (import "ui" "create_panel" (func $create_panel (param i32)))
          (func (export "run")
            i32.const 7
            call $create_panel))
    "#;
    let module = compile(&engine, source);
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap(
            "ui",
            "create_panel",
            |_caller: wasmi::Caller<'_, ()>, _text: i32| {},
        )
        .expect("allowlisted UI import must link");
    let mut store = Store::new(&engine, ());
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

    let loop_module = compile(&engine, r#"(module (func (export "loop") (loop br 0)))"#);
    let mut loop_store = Store::new(&engine, ());
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
    let mut memory_store = Store::new(&engine, ());
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

    println!(
        "wasm comparison: allowlisted_import=pass fuel=out_of_fuel fuel_trap_us={elapsed_us} max_memory_pages={MAX_LINEAR_MEMORY_PAGES} wasi_imports=none"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_proof_runs() {
        main();
    }
}
