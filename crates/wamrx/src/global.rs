//! Exported Wasm globals and how to read/write them.

use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::value::{GlobalType, Mutability, Val, ValType};
use core::marker::PhantomData;
use std::ffi::c_void;

/// A handle to a global exported by an [`Instance`].
///
/// Borrows its [`Instance`]; obtain one via [`Instance::get_global`]. Reads and
/// writes go directly through the global's storage in the instance (WAMR's
/// `wasm_global_inst_t::global_data`).
pub struct Global<'a> {
    content: ValType,
    mutability: Mutability,
    /// Pointer to the global's storage inside the owning instance.
    data: *mut c_void,
    /// Ties this handle's lifetime to the borrowed [`Instance`] whose memory
    /// `data` points into.
    _marker: PhantomData<&'a Instance>,
}

impl<'a> Global<'a> {
    pub(crate) fn new(content: ValType, mutability: Mutability, data: *mut c_void) -> Global<'a> {
        Global {
            content,
            mutability,
            data,
            _marker: PhantomData,
        }
    }

    /// Returns the [`GlobalType`] (content type and mutability) of this global.
    pub fn ty(&self) -> GlobalType {
        GlobalType::new(self.content, self.mutability)
    }

    /// Reads the current value of the global.
    pub fn get(&self) -> Val {
        // SAFETY: `data` points to valid storage of type `content` inside the
        // instance, which outlives `self` (enforced by the `'a` borrow). WAMR
        // packs globals at 4-byte granularity, so the pointer may not meet the
        // natural alignment of an `i64`/`f64`; use unaligned reads.
        unsafe {
            match self.content {
                ValType::I32 => Val::I32((self.data as *const i32).read_unaligned()),
                ValType::I64 => Val::I64((self.data as *const i64).read_unaligned()),
                ValType::F32 => Val::F32((self.data as *const f32).read_unaligned()),
                ValType::F64 => Val::F64((self.data as *const f64).read_unaligned()),
            }
        }
    }

    /// Writes a new value to the global.
    ///
    /// Returns [`Error::GlobalImmutable`] if the global is `const`, or
    /// [`Error::TypeMismatch`] if `new_val`'s type differs from the global's
    /// content type.
    pub fn set(&mut self, new_val: Val) -> Result<()> {
        if self.mutability == Mutability::Const {
            return Err(Error::GlobalImmutable);
        }
        if new_val.ty() != self.content {
            return Err(Error::TypeMismatch {
                expected: self.content,
                provided: new_val.ty(),
            });
        }
        // SAFETY: as in `get`, plus the type check above guarantees we write the
        // correct width for `content`; unaligned writes for the same reason.
        unsafe {
            match new_val {
                Val::I32(v) => (self.data as *mut i32).write_unaligned(v),
                Val::I64(v) => (self.data as *mut i64).write_unaligned(v),
                Val::F32(v) => (self.data as *mut f32).write_unaligned(v),
                Val::F64(v) => (self.data as *mut f64).write_unaligned(v),
            }
        }
        Ok(())
    }
}

impl core::fmt::Debug for Global<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Global")
            .field("ty", &self.ty())
            .field("value", &self.get())
            .finish()
    }
}
