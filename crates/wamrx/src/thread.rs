//! Per-thread WAMR execution environment setup.
//!
//! WAMR's hardware-bound-checking (enabled by default) installs per-thread
//! signal handlers, so every thread that instantiates or calls Wasm must first
//! call `wasm_runtime_init_thread_env`. The main thread is initialized by
//! `wasm_runtime_full_init`; any other thread (e.g. a `cargo test` worker) needs
//! this. We do it lazily via a thread-local guard that also tears the env down
//! when the thread exits.

use wamrx_sys as sys;

struct ThreadEnvGuard;

impl ThreadEnvGuard {
    fn new() -> Self {
        // Call unconditionally: `wasm_runtime_thread_env_inited()` is unreliable
        // in AOT-disabled builds (it only checks the signal env when AOT is
        // compiled in, so it falsely reports "inited" here). The underlying
        // `os_thread_signal_init` is idempotent per thread, and this guard runs
        // only once per thread anyway.
        //
        // SAFETY: the runtime is initialized before any `Engine`-derived object
        // (and hence any call to this) exists.
        unsafe { sys::wasm_runtime_init_thread_env() };
        ThreadEnvGuard
    }
}

impl Drop for ThreadEnvGuard {
    fn drop(&mut self) {
        // SAFETY: matches the init above; safe to call at thread exit.
        unsafe { sys::wasm_runtime_destroy_thread_env() };
    }
}

thread_local! {
    static THREAD_ENV: ThreadEnvGuard = ThreadEnvGuard::new();
}

/// Ensures the current thread's WAMR execution environment is initialized.
///
/// Idempotent and cheap after the first call on a thread.
pub(crate) fn ensure_thread_env() {
    THREAD_ENV.with(|_| {});
}
