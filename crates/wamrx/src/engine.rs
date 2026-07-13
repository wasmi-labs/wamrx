//! The [`Engine`]: a handle to the process-global WAMR runtime.

use crate::error::{Error, Result};
use std::rc::Rc;
use std::sync::Mutex;
use wamrx_sys as sys;

/// Number of live [`EngineInner`] instances. The WAMR runtime is a process-wide
/// singleton (`wasm_runtime_full_init` / `wasm_runtime_destroy`), so we
/// reference-count initialization and tear it down when the last engine drops.
static RUNTIME_REFCOUNT: Mutex<usize> = Mutex::new(0);

struct EngineInner;

impl Drop for EngineInner {
    fn drop(&mut self) {
        let mut count = RUNTIME_REFCOUNT.lock().unwrap();
        *count -= 1;
        if *count == 0 {
            // SAFETY: no modules/instances can outlive their owning `Engine`
            // clone (each holds one), so the runtime is idle here.
            unsafe { sys::wasm_runtime_destroy() };
        }
    }
}

/// The engine owns global WAMR runtime state.
///
/// Cloning an `Engine` is cheap and shares the same underlying runtime. WAMR is
/// initialized on the first `Engine` and destroyed when the last one is dropped.
///
/// # Note
///
/// WAMR is single-threaded per runtime; `Engine` is therefore neither `Send`
/// nor `Sync` (enforced by the internal [`Rc`]).
#[derive(Clone)]
pub struct Engine {
    // Held for its ref-counting `Drop` (runtime teardown) and to make `Engine`
    // neither `Send` nor `Sync`; not read directly.
    #[allow(dead_code)]
    inner: Rc<EngineInner>,
}

impl Engine {
    /// Creates a new engine, initializing the global WAMR runtime if needed.
    ///
    /// Uses the system allocator (`Alloc_With_System_Allocator`) so instances
    /// are not constrained by a fixed global heap pool.
    pub fn new() -> Result<Engine> {
        let mut count = RUNTIME_REFCOUNT.lock().unwrap();
        if *count == 0 {
            let mut args = sys::RuntimeInitArgs {
                mem_alloc_type: sys::Alloc_With_System_Allocator,
                ..Default::default()
            };
            // SAFETY: `args` is a valid, fully-initialized init-args struct.
            let ok = unsafe { sys::wasm_runtime_full_init(&mut args) };
            if !ok {
                return Err(Error::RuntimeInit);
            }
        }
        *count += 1;
        Ok(Engine {
            inner: Rc::new(EngineInner),
        })
    }
}
