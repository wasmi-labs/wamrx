//! Exported Wasm linear memories and how to read/write their bytes.

use crate::instance::Instance;
use crate::value::MemoryType;
use core::marker::PhantomData;
use core::slice;
use wamrx_sys as sys;

/// The size of a Wasm linear-memory page, in bytes (64 KiB).
const WASM_PAGE_SIZE: u64 = 65536;

/// A handle to a linear memory exported by an [`Instance`].
///
/// Borrows its [`Instance`]; obtain one via [`Instance::get_memory`]. The
/// declared [`MemoryType`] is captured up front, while [`Memory::size`],
/// [`Memory::data`], and [`Memory::data_mut`] query WAMR live so they reflect
/// any `memory.grow` the Wasm side has performed.
pub struct Memory<'a> {
    /// The WAMR memory instance backing this handle.
    inst: sys::wasm_memory_inst_t,
    /// The declared type (page limits), stable across grows.
    ty: MemoryType,
    /// Ties this handle's lifetime to the borrowed [`Instance`] whose linear
    /// memory `inst` refers to.
    _marker: PhantomData<&'a Instance>,
}

impl<'a> Memory<'a> {
    pub(crate) fn new(inst: sys::wasm_memory_inst_t, ty: MemoryType) -> Memory<'a> {
        Memory {
            inst,
            ty,
            _marker: PhantomData,
        }
    }

    /// Returns the declared [`MemoryType`] (page limits) of this memory.
    pub fn ty(&self) -> MemoryType {
        self.ty
    }

    /// Returns the current size of the memory in Wasm pages (64 KiB each), not
    /// bytes.
    ///
    /// Derived from the live byte length rather than WAMR's page count, which is
    /// unreliable: WAMR folds a non-growing memory into a single oversized page.
    pub fn size(&self) -> u64 {
        self.byte_len() as u64 / WASM_PAGE_SIZE
    }

    /// Returns the number of bytes currently backing the memory,
    /// `cur_page_count * bytes_per_page`. This product is invariant under WAMR's
    /// single-page folding, so it is the true linear-memory size.
    fn byte_len(&self) -> usize {
        // SAFETY: `inst` is a live memory instance owned by the borrowed
        // `Instance` (enforced by the `'a` borrow).
        let pages = unsafe { sys::wasm_memory_get_cur_page_count(self.inst) };
        let bytes_per_page = unsafe { sys::wasm_memory_get_bytes_per_page(self.inst) };
        pages.saturating_mul(bytes_per_page) as usize
    }

    /// Returns the memory's bytes as a read-only slice.
    ///
    /// The slice is valid only until the memory is grown; do not hold it across
    /// a Wasm call that may execute `memory.grow` (which can reallocate and move
    /// the backing storage).
    pub fn data(&self) -> &[u8] {
        // SAFETY: `base` points to `byte_len` valid bytes inside the borrowed
        // `Instance`. WAMR may hand back a null base for a zero-length memory.
        let base = unsafe { sys::wasm_memory_get_base_address(self.inst) };
        if base.is_null() {
            return &[];
        }
        unsafe { slice::from_raw_parts(base as *const u8, self.byte_len()) }
    }

    /// Returns the memory's bytes as a mutable slice.
    ///
    /// The same borrow caveat as [`Memory::data`] applies.
    pub fn data_mut(&mut self) -> &mut [u8] {
        // SAFETY: as in `data`; the `&mut self` borrow rules out aliasing views
        // through this handle.
        let base = unsafe { sys::wasm_memory_get_base_address(self.inst) };
        if base.is_null() {
            return &mut [];
        }
        unsafe { slice::from_raw_parts_mut(base as *mut u8, self.byte_len()) }
    }
}

impl core::fmt::Debug for Memory<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Memory")
            .field("ty", &self.ty)
            .field("size", &self.size())
            .finish()
    }
}
