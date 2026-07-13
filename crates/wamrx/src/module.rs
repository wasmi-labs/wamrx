//! The [`Module`]: a loaded (but not yet instantiated) Wasm module.

use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::util::with_error_buf;
use wamrx_sys as sys;

/// A loaded Wasm module, ready to be instantiated via a
/// [`Linker`](crate::Linker).
///
/// # Note on ownership
///
/// `wasm_runtime_load` both **mutates** the input byte buffer in place and
/// retains pointers into it for the lifetime of the loaded module. `Module`
/// therefore owns a private, heap-stable copy of the Wasm bytes and keeps it
/// alive (and un-moved) until the module is unloaded on drop.
pub struct Module {
    // Field order matters: `raw` must be unloaded (in `Drop`) before `bytes`
    // (which backs it) and `engine` (which owns the runtime) are released.
    raw: sys::wasm_module_t,
    /// Backing store for the loaded module; WAMR holds pointers into this.
    _bytes: Box<[u8]>,
    /// Keeps the global runtime alive for at least as long as this module.
    engine: Engine,
}

impl Module {
    /// Loads and validates a Wasm module from `bytes` (binary `.wasm`).
    pub fn new(engine: &Engine, bytes: &[u8]) -> Result<Module> {
        // Own a mutable, address-stable copy; WAMR may rewrite/reference it.
        let mut owned: Box<[u8]> = bytes.to_vec().into_boxed_slice();

        let (raw, err) = with_error_buf(|err_buf, err_size| unsafe {
            sys::wasm_runtime_load(owned.as_mut_ptr(), owned.len() as u32, err_buf, err_size)
        });

        if raw.is_null() {
            return Err(Error::ModuleLoad(err));
        }

        Ok(Module {
            raw,
            _bytes: owned,
            engine: engine.clone(),
        })
    }

    /// The [`Engine`] this module belongs to.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub(crate) fn raw(&self) -> sys::wasm_module_t {
        self.raw
    }
}

impl core::fmt::Debug for Module {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Module").finish_non_exhaustive()
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // SAFETY: `raw` is a live module handle produced by `wasm_runtime_load`
        // and not yet unloaded; no instances reference it (they own their
        // `Module`).
        unsafe { sys::wasm_runtime_unload(self.raw) };
    }
}
