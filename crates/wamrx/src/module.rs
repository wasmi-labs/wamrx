//! The [`Module`]: a loaded (but not yet instantiated) Wasm module.

use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::util::with_error_buf;
use crate::value::MemoryType;
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
    /// Declared type of the module's linear memory, if it defines one. Parsed
    /// from the original bytes because WAMR discards the declared page limits
    /// for non-growing modules (see [`crate::Instance::get_memory`]).
    memory_type: Option<MemoryType>,
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

        // Parse from `bytes` (the caller's untouched input); WAMR rewrites its
        // own `owned` copy during load.
        let memory_type = parse_memory_type(bytes);

        Ok(Module {
            raw,
            _bytes: owned,
            memory_type,
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

    /// The declared [`MemoryType`] of this module's linear memory, if any.
    pub(crate) fn memory_type(&self) -> Option<MemoryType> {
        self.memory_type
    }
}

/// Parses the declared type of the module's (single) linear memory from raw
/// Wasm bytes, returning `None` if it defines no memory.
///
/// WAMR forbids multi-memory with the fast interpreter, so a module defines at
/// most one memory; this reads the first entry of the memory section (section
/// id 5). Returns `None` on any malformed input — the bytes have already been
/// validated by `wasm_runtime_load` before this runs.
fn parse_memory_type(bytes: &[u8]) -> Option<MemoryType> {
    // Preamble: 4-byte magic `\0asm` + 4-byte version.
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        return None;
    }
    let mut pos = 8;
    while pos < bytes.len() {
        let id = bytes[pos];
        pos += 1;
        let size = read_leb_u32(bytes, &mut pos)? as usize;
        let payload_end = pos.checked_add(size)?;
        if payload_end > bytes.len() {
            return None;
        }
        // Memory section: a vector of memory types (`limits`).
        if id == 5 {
            let mut mp = pos;
            let count = read_leb_u32(bytes, &mut mp)?;
            if count == 0 {
                return None;
            }
            // `limits`: a flags byte then the minimum, and the maximum iff the
            // low flag bit is set (bit 1 = shared, bit 2 = 64-bit; unused here).
            let flags = read_leb_u32(bytes, &mut mp)?;
            let minimum = read_leb_u32(bytes, &mut mp)? as u64;
            let maximum = if flags & 0x01 != 0 {
                Some(read_leb_u32(bytes, &mut mp)? as u64)
            } else {
                None
            };
            return Some(MemoryType::new(minimum, maximum));
        }
        pos = payload_end;
    }
    None
}

/// Reads an unsigned LEB128 `u32` from `buf` at `*pos`, advancing `*pos`.
/// Returns `None` if the input is truncated or the value overflows 32 bits.
fn read_leb_u32(buf: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result: u32 = 0;
    let mut shift = 0;
    loop {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        result |= ((byte & 0x7f) as u32).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
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
