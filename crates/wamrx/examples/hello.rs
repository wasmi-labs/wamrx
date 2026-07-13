//! Minimal `wamrx` example: link a host function, instantiate a module, and
//! call an exported function.
//!
//! Run with: `cargo run -p wamrx --example hello`

use wamrx::{Engine, FuncType, Linker, Module, Val, ValType};

/// Host function imported by the module as `env::add_one`.
fn add_one(params: &[Val], results: &mut [Val]) {
    let x = match params[0] {
        Val::I32(v) => v,
        other => panic!("unexpected argument: {other}"),
    };
    results[0] = Val::I32(x + 1);
}

fn main() -> anyhow::Result<()> {
    let engine = Engine::new()?;

    // Define host functions before loading the module that imports them:
    // WAMR resolves imports at load time.
    let mut linker = Linker::new(&engine);
    linker.define_func(
        "env",
        "add_one",
        FuncType::new([ValType::I32], [ValType::I32]),
        add_one,
    )?;

    let wasm = wat::parse_str(
        r#"(module
            (import "env" "add_one" (func $add_one (param i32) (result i32)))
            ;; square(x) = (x * x) + 1, using the host `add_one`.
            (func (export "square_plus_one") (param i32) (result i32)
                local.get 0
                local.get 0
                i32.mul
                call $add_one))"#,
    )?;
    let module = Module::new(&engine, &wasm)?;
    let instance = linker.instantiate(module)?;

    let mut results = [Val::I32(0)];
    instance
        .get_func("square_plus_one")?
        .call(&[Val::I32(7)], &mut results)?;

    println!("square_plus_one(7) = {}", results[0]); // 7*7 + 1 = 50
    assert_eq!(results[0], Val::I32(50));
    Ok(())
}
