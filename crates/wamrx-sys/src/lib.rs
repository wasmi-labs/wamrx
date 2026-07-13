//! Raw FFI bindings to the [WebAssembly Micro Runtime (WAMR)] fast interpreter.
//!
//! This crate is the low-level `-sys` layer: it builds the vendored WAMR C
//! sources (fast-interpreter configuration only) and exposes the auto-generated
//! `bindgen` bindings for the embedding API declared in `wasm_export.h`.
//!
//! Prefer the safe [`wamrx`] crate for application code; reach for these raw
//! bindings only when you need an API that the high-level wrapper does not yet
//! cover.
//!
//! [WebAssembly Micro Runtime (WAMR)]: https://github.com/bytecodealliance/wasm-micro-runtime
//! [`wamrx`]: https://docs.rs/wamrx
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
