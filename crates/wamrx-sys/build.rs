//! Build script for `wamrx-sys`.
//!
//! Compiles the vendored WAMR (WebAssembly Micro Runtime) sources into a static
//! `libiwasm.a` via WAMR's own CMake build (the `vmlib` target), then generates
//! Rust FFI bindings for the embedding C API (`wasm_export.h`) with `bindgen`.
//!
//! The build is pinned to the **fast interpreter** running mode: AOT and every
//! JIT tier (LLVM-JIT, Fast-JIT) are hard-disabled so the heavy LLVM toolchain
//! is never required. Each optional WAMR compile-time toggle is surfaced as a
//! cargo feature (see `Cargo.toml`) and forwarded to CMake here.

use std::{
    env,
    path::{Path, PathBuf},
};

/// Returns `"1"` if the given cargo feature is enabled, otherwise `"0"`.
fn flag(feature: &str) -> &'static str {
    // Cargo exposes enabled features as `CARGO_FEATURE_<NAME>` with `-` -> `_`.
    let var = format!("CARGO_FEATURE_{}", feature.to_uppercase().replace('-', "_"));
    if env::var_os(var).is_some() { "1" } else { "0" }
}

/// Maps the Rust target arch (`CARGO_CFG_TARGET_ARCH`) to a `WAMR_BUILD_TARGET`.
fn wamr_target() -> Option<&'static str> {
    match env::var("CARGO_CFG_TARGET_ARCH").ok()?.as_str() {
        "x86_64" => Some("X86_64"),
        "x86" => Some("X86_32"),
        "aarch64" => Some("AARCH64"),
        "arm" => Some("ARM"),
        "riscv64" => Some("RISCV64"),
        "riscv32" => Some("RISCV32"),
        _ => None,
    }
}

/// Maps the Rust target OS (`CARGO_CFG_TARGET_OS`) to a `WAMR_BUILD_PLATFORM`.
fn wamr_platform() -> Option<&'static str> {
    match env::var("CARGO_CFG_TARGET_OS").ok()?.as_str() {
        "macos" => Some("darwin"),
        "linux" => Some("linux"),
        "windows" => Some("windows"),
        "android" => Some("android"),
        "freebsd" => Some("freebsd"),
        _ => None,
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let wamr_root = manifest_dir.join("wasm-micro-runtime");

    assert!(
        wamr_root.join("CMakeLists.txt").exists(),
        "WAMR sources not found at {}.\n\
         Initialize the git submodule first:\n    \
         git submodule update --init --recursive",
        wamr_root.display(),
    );

    build_wamr(&wamr_root);
    generate_bindings(&wamr_root);

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");
    // Rebuild if the pinned submodule commit changes.
    println!("cargo:rerun-if-changed=wasm-micro-runtime/CMakeLists.txt");
    for var in [
        "WAMR_BUILD_PLATFORM",
        "WAMR_BUILD_TARGET",
        "WAMR_SHARED_PLATFORM_CONFIG",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
}

/// Configures and runs WAMR's CMake build for the static `vmlib` (`libiwasm.a`).
fn build_wamr(wamr_root: &Path) {
    let mut cfg = cmake::Config::new(wamr_root);

    // --- Running mode: fast interpreter only, no AOT / no JIT tiers. ---
    cfg.define("WAMR_BUILD_INTERP", "1")
        .define("WAMR_BUILD_FAST_INTERP", "1")
        .define("WAMR_BUILD_AOT", "0")
        .define("WAMR_BUILD_JIT", "0")
        .define("WAMR_BUILD_FAST_JIT", "0");

    // --- Wasm proposals (each a cargo feature). ---
    // WAMR enables several of these by default in its root CMakeLists, so we
    // always define them explicitly to honor our own feature defaults.
    // Note: exception-handling, memory64 and multi-memory are intentionally not
    // exposed — WAMR forbids each in combination with the fast interpreter
    // (see build-scripts/unsupported_combination.cmake).
    cfg.define("WAMR_BUILD_BULK_MEMORY", flag("bulk-memory"))
        .define("WAMR_BUILD_REF_TYPES", flag("reference-types"))
        .define("WAMR_BUILD_TAIL_CALL", flag("tail-call"))
        .define("WAMR_BUILD_GC", flag("gc"))
        .define("WAMR_BUILD_SHARED_MEMORY", flag("shared-memory"))
        .define("WAMR_BUILD_EXTENDED_CONST_EXPR", flag("extended-const"));

    // SIMD in the pure interpreter relies on the portable SIMDe backend; enable
    // it alongside WAMR_BUILD_SIMD so the feature works on every target.
    let simd = flag("simd");
    cfg.define("WAMR_BUILD_SIMD", simd)
        .define("WAMR_BUILD_LIB_SIMDE", simd);

    // --- Runtime / embedding toggles. ---
    cfg.define(
        "WAMR_BUILD_CUSTOM_NAME_SECTION",
        flag("custom-name-section"),
    )
    .define(
        "WAMR_BUILD_LOAD_CUSTOM_SECTION",
        flag("load-custom-section"),
    )
    .define("WAMR_BUILD_DUMP_CALL_STACK", flag("dump-call-stack"))
    .define("WAMR_BUILD_MULTI_MODULE", flag("multi-module"))
    .define("WAMR_BUILD_THREAD_MGR", flag("thread-mgr"))
    .define("WAMR_BUILD_PERF_PROFILING", flag("perf-profiling"));

    // --- libc flavors (host imports); off by default. ---
    cfg.define("WAMR_BUILD_LIBC_BUILTIN", flag("libc-builtin"))
        .define("WAMR_BUILD_LIBC_WASI", flag("libc-wasi"));

    // Hardware bound checking uses per-thread SIGSEGV handlers and native-stack
    // boundary tricks. That is fragile across arbitrary host threads (and can
    // abort on the main thread), so we disable it by default and use software
    // bounds checks. Opt in with the `hw-bound-check` feature for the faster,
    // signal-based path. Note the CMake flag is inverted (`DISABLE`).
    let disable_hw_bound_check = if flag("hw-bound-check") == "1" {
        "0"
    } else {
        "1"
    };
    cfg.define("WAMR_DISABLE_HW_BOUND_CHECK", disable_hw_bound_check);

    // --- Platform / target: explicit when we can map them, else let WAMR's
    // CMake auto-detect from the host. Env vars always win. ---
    if let Ok(platform) = env::var("WAMR_BUILD_PLATFORM") {
        cfg.define("WAMR_BUILD_PLATFORM", platform);
    } else if let Some(platform) = wamr_platform() {
        cfg.define("WAMR_BUILD_PLATFORM", platform);
    }
    if let Ok(target) = env::var("WAMR_BUILD_TARGET") {
        cfg.define("WAMR_BUILD_TARGET", target);
    } else if let Some(target) = wamr_target() {
        cfg.define("WAMR_BUILD_TARGET", target);
    }

    // Build only the static runtime library, not the samples/tools.
    let dst = cfg.build_target("vmlib").build();

    // With `build_target`, the artifact lands under `<out>/build/`.
    println!("cargo:rustc-link-search=native={}/build", dst.display());
    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-lib=static=iwasm");

    link_system_libs();
}

/// Emits the transitive system libraries WAMR's static lib depends on.
fn link_system_libs() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match os.as_str() {
        "linux" | "android" | "freebsd" => {
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
        }
        "windows" => {
            println!("cargo:rustc-link-lib=dylib=ntdll");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
        }
        // macOS/iOS: libm, libpthread and libdl live in libSystem; nothing extra.
        _ => {}
    }
}

/// Generates Rust bindings for the WAMR embedding C API via `bindgen`.
fn generate_bindings(wamr_root: &Path) {
    let include_dir = wamr_root.join("core/iwasm/include");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        .use_core()
        .ctypes_prefix("::core::ffi")
        // WAMR's C doc comments aren't valid Rustdoc (Doxygen `@param`, bare
        // `<...>` HTML, `args[0]` link-like text); dropping them keeps
        // `cargo doc` clean. Consult the C headers for API docs.
        .generate_comments(false)
        .derive_default(true)
        .derive_debug(true)
        // Only surface the embedding API surface, not libc/system decls.
        .allowlist_function("wasm_.*")
        .allowlist_type("wasm_.*")
        .allowlist_type("NativeSymbol")
        .allowlist_type("RuntimeInitArgs")
        .allowlist_type("RunningMode")
        .allowlist_type("mem_alloc_type_t")
        .allowlist_type("package_type_t")
        .allowlist_var("WASM_.*")
        .prepend_enum_name(false)
        .generate()
        .expect("failed to generate WAMR bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}
