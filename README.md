# wamrx

Safe, [Wasmi]/[Wasmtime]-inspired Rust bindings for the [WebAssembly Micro
Runtime (WAMR)] **fast interpreter**.

These bindings deliberately target *only* WAMR's fast interpreter — no classic
interpreter, no AOT, and no JIT tiers. As a result, **no LLVM toolchain is ever
required** to build them; just `cmake` and a C compiler.

[Wasmi]: https://github.com/wasmi-labs/wasmi
[Wasmtime]: https://github.com/bytecodealliance/wasmtime
[WebAssembly Micro Runtime (WAMR)]: https://github.com/bytecodealliance/wasm-micro-runtime

## What this is

`wamrx` is a standalone Cargo workspace with two crates:

- **`crates/wamrx-sys`** — raw [`bindgen`] FFI. Its `build.rs` compiles the
  vendored WAMR git submodule into a static `libiwasm.a` (via the `cmake`
  crate's `vmlib` target) and generates bindings from `wasm_export.h`.
- **`crates/wamrx`** — a safe, Wasmi/Wasmtime-inspired API built on top:
  `Engine` / `Module` / `Linker` / `Instance` / `Func` / `Global`, plus the
  value types `Val` / `ValType` / `FuncType` / `GlobalType` / `Mutability`.

[`bindgen`]: https://github.com/rust-lang/rust-bindgen

## Requirements & first-time setup

The WAMR sources are a git submodule; **the build fails without it**:

```sh
git submodule update --init --recursive
```

Building also requires **`cmake`** and a **C compiler** on `PATH` (WAMR is built
from C). The first build compiles WAMR from source and is therefore slow;
subsequent builds are cached.

## Quick start

Instantiate a module and call an exported function:

```rust
use wamrx::{Engine, Linker, Module, Val};

let engine = Engine::new()?;
let wasm = wat::parse_str(r#"(module (func (export "add") (param i32 i32) (result i32)
    local.get 0 local.get 1 i32.add))"#)?;
let module = Module::new(&engine, &wasm)?;
let linker = Linker::new(&engine);
let instance = linker.instantiate(module)?;

let mut results = [Val::I32(0)];
instance.get_func("add")?.call(&[Val::I32(2), Val::I32(3)], &mut results)?;
assert_eq!(results[0], Val::I32(5));
# Ok::<(), anyhow::Error>(())
```

For a version that also links a **host function**, see
[`crates/wamrx/examples/hello.rs`](crates/wamrx/examples/hello.rs):

```sh
cargo run -p wamrx --example hello
```

## API overview

| Type | Role |
| --- | --- |
| `Engine` | Owns a reference to the process-global WAMR runtime. |
| `Module` | A loaded, validated Wasm module (owns its bytes). |
| `Linker` | Defines host functions and instantiates modules. |
| `Instance` | An instantiated module; look up its exports. |
| `Func` | A callable exported function. |
| `Global` | An exported global; read/write its value. |
| `Val` / `ValType` | Runtime values / value types (i32, i64, f32, f64). |
| `FuncType` / `GlobalType` / `Mutability` | Type descriptions. |
| `InstanceConfig` | Per-instance stack/heap configuration. |
| `Error` / `Result` | Crate error type and result alias. |

## How it differs from Wasmtime / Wasmi

`wamrx` surfaces WAMR's actual model honestly rather than faking Wasmtime
semantics. Keep these in mind:

- **Imports resolve at module *load* time, not at instantiate time.** Host
  functions must be defined via `Linker::define_func` **before** the importing
  module is created with `Module::new`.
- **Process-global runtime and native registry.** The runtime is a singleton
  ref-counted by `Engine`; host functions live in a process-global registry
  keyed by module name, owned by the `Linker`.
- **Host functions are limited by WAMR's raw calling convention.** They accept
  only the four numeric types (i32/i64/f32/f64) and return **at most one
  result**.
- **Single-threaded.** `Engine` is intentionally neither `Send` nor `Sync`.
- **Values are the four numeric MVP types only.** The type is `Val` (not
  `Value`); `v128`, `funcref`, and `externref` are not modeled as values.

## Cargo features

Default: `bulk-memory`, `reference-types`. Each `wamrx` feature is a
pass-through to the identically named `wamrx-sys` feature, which `build.rs` maps
1:1 to a WAMR CMake toggle. Only fast-interpreter-compatible options are
exposed.

| Feature | Notes |
| --- | --- |
| `bulk-memory`, `reference-types` | Enabled by default. |
| `simd`, `gc`, `tail-call`, `shared-memory`, `extended-const` | Additional Wasm proposals. |
| `custom-name-section`, `load-custom-section`, `dump-call-stack`, `multi-module`, `thread-mgr`, `perf-profiling` | Runtime / embedding toggles. |
| `libc-builtin`, `libc-wasi` | libc flavors for host imports (off by default). |
| `hw-bound-check` | WAMR's signal-based bounds checking. **Off by default**: it is fragile across host threads and can abort on the main thread (e.g. under criterion). The default software bounds-check path is portable. |

**Deliberately absent** (WAMR forbids them together with the fast interpreter):
exception-handling, memory64, multi-memory, stringref.

## Common commands

```sh
cargo build --workspace                 # builds WAMR via cmake on first run (slow)
cargo test --workspace                  # run all tests
cargo run -p wamrx --example hello      # run the example
cargo fmt --all --check                 # format check (CI gate)
cargo clippy --workspace --all-targets -- -D warnings   # lint (CI gate)
cargo doc --workspace --no-deps --document-private-items # docs (CI gate)
```

Integration tests live in
[`crates/wamrx/tests/integration.rs`](crates/wamrx/tests/integration.rs).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your
option. (`wamrx-sys` and WAMR itself are Apache-2.0-licensed.)
