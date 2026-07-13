//! Safe, Wasmi/Wasmtime-inspired Rust bindings for the [WebAssembly Micro
//! Runtime (WAMR)] **fast interpreter**.
//!
//! These bindings deliberately target only WAMR's fast interpreter — no classic
//! interpreter, no AOT, and no JIT tiers — so no LLVM toolchain is required to
//! build them. Compile-time WAMR proposals map to cargo features (see the
//! crate's `Cargo.toml`).
//!
//! # Example
//!
//! ```no_run
//! use wamrx::{Engine, Linker, Module, Val};
//!
//! let engine = Engine::new()?;
//! let wasm = wat::parse_str(r#"(module (func (export "add") (param i32 i32) (result i32)
//!     local.get 0 local.get 1 i32.add))"#)?;
//! let module = Module::new(&engine, &wasm)?;
//! let linker = Linker::new(&engine);
//! let instance = linker.instantiate(module)?;
//!
//! let mut results = [Val::I32(0)];
//! instance.get_func("add")?.call(&[Val::I32(2), Val::I32(3)], &mut results)?;
//! assert_eq!(results[0], Val::I32(5));
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! # Relationship to WAMR's model
//!
//! Where WAMR's invariants differ from Wasmtime, `wamrx` surfaces them honestly
//! rather than faking them:
//!
//! - Host functions are registered in a **process-global** registry keyed by
//!   module name; the [`Linker`] owns that registration.
//! - Host functions support the four numeric types and **at most one result**
//!   (a limitation of WAMR's raw calling convention).
//! - A [`Module`] owns its Wasm bytes because WAMR mutates and retains the input
//!   buffer.
//! - The runtime is single-threaded; [`Engine`] is neither `Send` nor `Sync`.
//!
//! [WebAssembly Micro Runtime (WAMR)]: https://github.com/bytecodealliance/wasm-micro-runtime

mod config;
mod engine;
mod error;
mod func;
mod global;
mod instance;
mod linker;
mod module;
mod thread;
mod util;
mod value;

pub use self::config::InstanceConfig;
pub use self::engine::Engine;
pub use self::error::{Error, Result};
pub use self::func::Func;
pub use self::global::Global;
pub use self::instance::Instance;
pub use self::linker::{HostFunc, Linker};
pub use self::module::Module;
pub use self::value::{FuncType, GlobalType, Mutability, Val, ValType};
