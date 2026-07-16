//! End-to-end tests for the `wamrx` high-level API against the WAMR fast
//! interpreter: module loading, exported calls across all numeric types, host
//! functions dispatched through the raw trampoline, and trap handling.

use wamrx::{
    Engine, Error, FuncType, GlobalType, InstanceConfig, Linker, MemoryType, Module, Mutability,
    Val, ValType,
};

/// Loads `wat`, instantiates with an empty linker, and returns the instance.
fn instantiate(wat: &str) -> wamrx::Instance {
    let engine = Engine::new().expect("engine init");
    let wasm = wat::parse_str(wat).expect("wat parse");
    let module = Module::new(&engine, &wasm).expect("module load");
    let linker = Linker::new(&engine);
    linker.instantiate(module).expect("instantiate")
}

#[test]
fn call_exported_add_i32() {
    let instance = instantiate(
        r#"(module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0 local.get 1 i32.add))"#,
    );
    let mut results = [Val::I32(0)];
    instance
        .get_func("add")
        .unwrap()
        .call(&[Val::I32(2), Val::I32(40)], &mut results)
        .unwrap();
    assert_eq!(results[0], Val::I32(42));
}

#[test]
fn call_roundtrips_all_numeric_types() {
    // Identity functions confirm i32/i64/f32/f64 encode/decode correctly.
    let instance = instantiate(
        r#"(module
            (func (export "id_i32") (param i32) (result i32) local.get 0)
            (func (export "id_i64") (param i64) (result i64) local.get 0)
            (func (export "id_f32") (param f32) (result f32) local.get 0)
            (func (export "id_f64") (param f64) (result f64) local.get 0))"#,
    );

    let mut r = [Val::I32(0)];
    instance
        .get_func("id_i32")
        .unwrap()
        .call(&[Val::I32(-123456)], &mut r)
        .unwrap();
    assert_eq!(r, [Val::I32(-123456)]);

    let mut r = [Val::I64(0)];
    instance
        .get_func("id_i64")
        .unwrap()
        .call(&[Val::I64(i64::MIN + 7)], &mut r)
        .unwrap();
    assert_eq!(r, [Val::I64(i64::MIN + 7)]);

    let mut r = [Val::F32(0.0)];
    instance
        .get_func("id_f32")
        .unwrap()
        .call(&[Val::F32(-3.5)], &mut r)
        .unwrap();
    assert_eq!(r, [Val::F32(-3.5)]);

    let mut r = [Val::F64(0.0)];
    instance
        .get_func("id_f64")
        .unwrap()
        .call(&[Val::F64(-123456.5)], &mut r)
        .unwrap();
    assert_eq!(r, [Val::F64(-123456.5)]);
}

#[test]
fn host_function_via_raw_trampoline() {
    // A host `mul` imported from "env", called back by Wasm. Exercises the raw
    // trampoline: argument decode + single-result write-back.
    fn mul(params: &[Val], results: &mut [Val]) {
        let a = match params[0] {
            Val::I32(v) => v,
            _ => unreachable!(),
        };
        let b = match params[1] {
            Val::I32(v) => v,
            _ => unreachable!(),
        };
        results[0] = Val::I32(a * b);
    }

    let engine = Engine::new().unwrap();

    // Host functions must be defined before the importing module is loaded.
    let mut linker = Linker::new(&engine);
    linker
        .define_func(
            "env",
            "mul",
            FuncType::new([ValType::I32, ValType::I32], [ValType::I32]),
            mul,
        )
        .unwrap();

    let wasm = wat::parse_str(
        r#"(module
            (import "env" "mul" (func $mul (param i32 i32) (result i32)))
            (func (export "run") (param i32 i32) (result i32)
                local.get 0 local.get 1 call $mul))"#,
    )
    .unwrap();
    let module = Module::new(&engine, &wasm).unwrap();
    let instance = linker.instantiate(module).unwrap();

    let mut results = [Val::I32(0)];
    instance
        .get_func("run")
        .unwrap()
        .call(&[Val::I32(6), Val::I32(7)], &mut results)
        .unwrap();
    assert_eq!(results[0], Val::I32(42));
}

#[test]
fn host_function_mixed_types_and_no_result() {
    use std::cell::Cell;
    std::thread_local! {
        static SEEN: Cell<(i64, f64)> = const { Cell::new((0, 0.0)) };
    }

    // (i64, f64) -> () host function, verifies wide-slot decode and 0 results.
    fn observe(params: &[Val], _results: &mut [Val]) {
        let i = match params[0] {
            Val::I64(v) => v,
            _ => unreachable!(),
        };
        let f = match params[1] {
            Val::F64(v) => v,
            _ => unreachable!(),
        };
        SEEN.with(|s| s.set((i, f)));
    }

    let engine = Engine::new().unwrap();

    let mut linker = Linker::new(&engine);
    linker
        .define_func(
            "env",
            "observe",
            FuncType::new([ValType::I64, ValType::F64], []),
            observe,
        )
        .unwrap();

    let wasm = wat::parse_str(
        r#"(module
            (import "env" "observe" (func $observe (param i64 f64)))
            (func (export "run") (param i64 f64)
                local.get 0 local.get 1 call $observe))"#,
    )
    .unwrap();
    let module = Module::new(&engine, &wasm).unwrap();
    let instance = linker.instantiate(module).unwrap();

    instance
        .get_func("run")
        .unwrap()
        .call(&[Val::I64(9_000_000_000), Val::F64(1.25)], &mut [])
        .unwrap();

    assert_eq!(SEEN.with(|s| s.get()), (9_000_000_000, 1.25));
}

#[test]
fn trap_surfaces_as_error() {
    let instance = instantiate(r#"(module (func (export "boom") (result i32) unreachable))"#);
    let err = instance
        .get_func("boom")
        .unwrap()
        .call(&[], &mut [Val::I32(0)])
        .unwrap_err();
    assert!(matches!(err, Error::Trap(_)), "expected trap, got {err:?}");
}

#[test]
fn missing_export_is_error() {
    let instance = instantiate(r#"(module (func (export "present")))"#);
    let err = instance.get_func("absent").unwrap_err();
    assert!(matches!(err, Error::FuncNotFound(_)));
}

#[test]
fn invalid_module_is_error() {
    let engine = Engine::new().unwrap();
    let err = Module::new(&engine, &[0, 1, 2, 3]).unwrap_err();
    assert!(matches!(err, Error::ModuleLoad(_)));
}

#[test]
fn host_function_capturing_state() {
    use std::cell::Cell;
    use std::rc::Rc;

    // A closure capturing shared, mutable host state (the substitute for
    // `Store<T>` data). Accumulates every argument it is called with.
    let sum = Rc::new(Cell::new(0i32));
    let sum_in_host = Rc::clone(&sum);

    let engine = Engine::new().unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .define_func(
            "env",
            "record",
            FuncType::new([ValType::I32], []),
            move |params, _results| {
                let x = match params[0] {
                    Val::I32(v) => v,
                    _ => unreachable!(),
                };
                sum_in_host.set(sum_in_host.get() + x);
            },
        )
        .unwrap();

    let wasm = wat::parse_str(
        r#"(module
            (import "env" "record" (func $record (param i32)))
            (func (export "run")
                i32.const 10 call $record
                i32.const 32 call $record))"#,
    )
    .unwrap();
    let module = Module::new(&engine, &wasm).unwrap();
    let instance = linker.instantiate(module).unwrap();

    instance
        .get_func("run")
        .unwrap()
        .call(&[], &mut [])
        .unwrap();
    assert_eq!(sum.get(), 42);
}

#[test]
fn func_ty_reports_signature() {
    let instance = instantiate(
        r#"(module
            (func (export "f") (param i32 f64) (result i64)
                local.get 0 i64.extend_i32_s))"#,
    );
    let func = instance.get_func("f").unwrap();

    let ty = func.ty();
    assert_eq!(ty.params(), &[ValType::I32, ValType::F64]);
    assert_eq!(ty.results(), &[ValType::I64]);
    assert_eq!(
        ty,
        FuncType::new([ValType::I32, ValType::F64], [ValType::I64])
    );
}

#[test]
fn exported_globals_get_set_and_type() {
    let instance = instantiate(
        r#"(module
            (global $g (export "g") (mut i32) (i32.const 7))
            (global $c (export "c") i64 (i64.const 100))
            (func (export "read_g") (result i32) global.get $g))"#,
    );

    // Mutable i32 global: type, read, write (and confirm Wasm sees the write).
    let mut g = instance.get_global("g").unwrap();
    assert_eq!(g.ty(), GlobalType::new(ValType::I32, Mutability::Mutable));
    assert_eq!(g.get(), Val::I32(7));

    g.set(Val::I32(42)).unwrap();
    assert_eq!(g.get(), Val::I32(42));

    let mut r = [Val::I32(0)];
    instance
        .get_func("read_g")
        .unwrap()
        .call(&[], &mut r)
        .unwrap();
    assert_eq!(r[0], Val::I32(42));

    // Setting a wrong-typed value is a type mismatch.
    assert!(matches!(
        g.set(Val::I64(1)),
        Err(Error::TypeMismatch { .. })
    ));

    // Immutable i64 global: type, read, and rejected write.
    let mut c = instance.get_global("c").unwrap();
    assert_eq!(c.ty(), GlobalType::new(ValType::I64, Mutability::Const));
    assert_eq!(c.get(), Val::I64(100));
    assert_eq!(c.set(Val::I64(1)), Err(Error::GlobalImmutable));

    // Missing global.
    assert!(matches!(
        instance.get_global("nope"),
        Err(Error::GlobalNotFound(_))
    ));
}

#[test]
fn exported_memory_data_size_and_type() {
    let instance = instantiate(
        r#"(module
            (memory (export "mem") 1 2)
            (func (export "load") (param i32) (result i32) local.get 0 i32.load)
            (func (export "store") (param i32 i32) local.get 0 local.get 1 i32.store))"#,
    );

    // Declared type: minimum 1 page, explicit maximum 2 pages.
    let mut mem = instance.get_memory("mem").unwrap();
    assert_eq!(mem.ty(), MemoryType::new(1, Some(2)));
    assert_eq!(mem.size(), 1);
    assert_eq!(mem.data().len(), 64 * 1024);

    // Host write via `data_mut` is visible to Wasm: store 42 (little-endian) at
    // offset 0, then read it back through the exported `load`.
    mem.data_mut()[0..4].copy_from_slice(&42i32.to_le_bytes());
    let mut r = [Val::I32(0)];
    instance
        .get_func("load")
        .unwrap()
        .call(&[Val::I32(0)], &mut r)
        .unwrap();
    assert_eq!(r[0], Val::I32(42));

    // Wasm write via `store` is visible to the host through `data`.
    instance
        .get_func("store")
        .unwrap()
        .call(&[Val::I32(8), Val::I32(99)], &mut [])
        .unwrap();
    let mem = instance.get_memory("mem").unwrap();
    assert_eq!(
        i32::from_le_bytes(mem.data()[8..12].try_into().unwrap()),
        99
    );
}

#[test]
fn multi_page_memory_reports_true_page_count() {
    // A 2-page memory: WAMR folds a non-growing memory into one oversized page
    // (num_bytes_per_page = 131072), so `size` must be recovered from bytes.
    let instance = instantiate(r#"(module (memory (export "mem") 2 4))"#);
    let mem = instance.get_memory("mem").unwrap();
    assert_eq!(mem.ty(), MemoryType::new(2, Some(4)));
    assert_eq!(mem.size(), 2);
    assert_eq!(mem.data().len(), 2 * 64 * 1024);
}

#[test]
fn growable_memory_size_tracks_growth() {
    // Containing `memory.grow` keeps WAMR from folding the memory, exercising
    // the non-collapsed path; `size` must reflect the post-grow page count.
    let instance = instantiate(
        r#"(module
            (memory (export "mem") 1 3)
            (func (export "grow") (param i32) (result i32) local.get 0 memory.grow))"#,
    );
    let mem = instance.get_memory("mem").unwrap();
    assert_eq!(mem.ty(), MemoryType::new(1, Some(3)));
    assert_eq!(mem.size(), 1);

    let mut r = [Val::I32(0)];
    instance
        .get_func("grow")
        .unwrap()
        .call(&[Val::I32(1)], &mut r)
        .unwrap();
    assert_eq!(r[0], Val::I32(1)); // previous page count

    let mem = instance.get_memory("mem").unwrap();
    assert_eq!(mem.size(), 2);
    assert_eq!(mem.data().len(), 2 * 64 * 1024);
}

#[test]
fn memory_without_declared_maximum_has_none() {
    let instance = instantiate(r#"(module (memory (export "m") 1))"#);
    let mem = instance.get_memory("m").unwrap();
    assert_eq!(mem.ty(), MemoryType::new(1, None));
    assert_eq!(mem.ty().maximum(), None);
}

#[test]
fn missing_memory_is_error() {
    let instance = instantiate(r#"(module (memory (export "m") 1))"#);
    assert!(matches!(
        instance.get_memory("absent"),
        Err(Error::MemoryNotFound(_))
    ));
}

#[test]
fn instantiate_with_custom_config() {
    // Exercises `InstanceConfig` + `Linker::instantiate_with`.
    let engine = Engine::new().unwrap();
    let wasm =
        wat::parse_str(r#"(module (func (export "answer") (result i32) i32.const 42))"#).unwrap();
    let module = Module::new(&engine, &wasm).unwrap();

    let mut config = InstanceConfig::new();
    config
        .exec_stack_size(1024 * 1024)
        .aux_stack_size(32 * 1024)
        .heap_size(0);

    let linker = Linker::new(&engine);
    let instance = linker.instantiate_with(module, &config).unwrap();

    let mut results = [Val::I32(0)];
    instance
        .get_func("answer")
        .unwrap()
        .call(&[], &mut results)
        .unwrap();
    assert_eq!(results[0], Val::I32(42));
}
