//! Wasm value and type representations, plus conversions to/from the raw
//! `wasm_val_t` used by the WAMR C API.

use wamrx_sys as sys;

/// A Wasm value type.
///
/// The WAMR fast interpreter bindings expose the four numeric MVP types. Any
/// reference-typed proposal values are out of scope for these bindings.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ValType {
    /// The Wasm `i32` type.
    I32,
    /// The Wasm `i64` type.
    I64,
    /// The Wasm `f32` type.
    F32,
    /// The Wasm `f64` type.
    F64,
}

impl core::fmt::Display for ValType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F32 => "f32",
            ValType::F64 => "f64",
        };
        f.write_str(s)
    }
}

impl ValType {
    /// The single `char` used for this type in a WAMR native signature string.
    pub(crate) fn signature_char(self) -> u8 {
        match self {
            ValType::I32 => b'i',
            ValType::I64 => b'I',
            ValType::F32 => b'f',
            ValType::F64 => b'F',
        }
    }

    /// The default (zero) [`Val`] for this type.
    pub fn default_value(self) -> Val {
        match self {
            ValType::I32 => Val::I32(0),
            ValType::I64 => Val::I64(0),
            ValType::F32 => Val::F32(0.0),
            ValType::F64 => Val::F64(0.0),
        }
    }

    /// Converts a raw WAMR value kind into a [`ValType`].
    ///
    /// Returns `None` for non-numeric kinds (e.g. `v128`, `funcref`,
    /// `externref`), which these bindings do not model.
    pub(crate) fn from_valkind(kind: sys::wasm_valkind_t) -> Option<ValType> {
        match kind as u32 {
            k if k == sys::WASM_I32 => Some(ValType::I32),
            k if k == sys::WASM_I64 => Some(ValType::I64),
            k if k == sys::WASM_F32 => Some(ValType::F32),
            k if k == sys::WASM_F64 => Some(ValType::F64),
            _ => None,
        }
    }
}

/// A typed Wasm value.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Val {
    /// A Wasm `i32` value.
    I32(i32),
    /// A Wasm `i64` value.
    I64(i64),
    /// A Wasm `f32` value.
    F32(f32),
    /// A Wasm `f64` value.
    F64(f64),
}

impl Val {
    /// Returns the [`ValType`] of this value.
    pub fn ty(self) -> ValType {
        match self {
            Val::I32(_) => ValType::I32,
            Val::I64(_) => ValType::I64,
            Val::F32(_) => ValType::F32,
            Val::F64(_) => ValType::F64,
        }
    }

    /// Encodes this value into a single 64-bit WAMR "raw" argument slot.
    ///
    /// WAMR's raw native calling convention stores every parameter in one
    /// `uint64` slot, with `i32`/`f32` occupying the low 32 bits.
    pub(crate) fn to_raw_slot(self) -> u64 {
        match self {
            Val::I32(v) => v as u32 as u64,
            Val::I64(v) => v as u64,
            Val::F32(v) => v.to_bits() as u64,
            Val::F64(v) => v.to_bits(),
        }
    }

    /// Decodes a value of type `ty` from a single 64-bit WAMR raw argument slot.
    pub(crate) fn from_raw_slot(ty: ValType, slot: u64) -> Val {
        match ty {
            ValType::I32 => Val::I32(slot as u32 as i32),
            ValType::I64 => Val::I64(slot as i64),
            ValType::F32 => Val::F32(f32::from_bits(slot as u32)),
            ValType::F64 => Val::F64(f64::from_bits(slot)),
        }
    }

    /// Converts this value into a `wasm_val_t` for `wasm_runtime_call_wasm_a`.
    pub(crate) fn to_wasm_val(self) -> sys::wasm_val_t {
        let mut raw: sys::wasm_val_t = unsafe { core::mem::zeroed() };
        match self {
            Val::I32(v) => {
                raw.kind = sys::WASM_I32 as sys::wasm_valkind_t;
                raw.of.i32_ = v;
            }
            Val::I64(v) => {
                raw.kind = sys::WASM_I64 as sys::wasm_valkind_t;
                raw.of.i64_ = v;
            }
            Val::F32(v) => {
                raw.kind = sys::WASM_F32 as sys::wasm_valkind_t;
                raw.of.f32_ = v;
            }
            Val::F64(v) => {
                raw.kind = sys::WASM_F64 as sys::wasm_valkind_t;
                raw.of.f64_ = v;
            }
        }
        raw
    }

    /// Converts a `wasm_val_t` returned by WAMR back into a [`Val`].
    pub(crate) fn from_wasm_val(raw: &sys::wasm_val_t) -> Val {
        let kind = raw.kind as u32;
        // SAFETY: we read the union member matching the reported `kind`.
        unsafe {
            if kind == sys::WASM_I32 {
                Val::I32(raw.of.i32_)
            } else if kind == sys::WASM_I64 {
                Val::I64(raw.of.i64_)
            } else if kind == sys::WASM_F32 {
                Val::F32(raw.of.f32_)
            } else {
                Val::F64(raw.of.f64_)
            }
        }
    }
}

impl From<i32> for Val {
    fn from(v: i32) -> Self {
        Val::I32(v)
    }
}
impl From<i64> for Val {
    fn from(v: i64) -> Self {
        Val::I64(v)
    }
}
impl From<f32> for Val {
    fn from(v: f32) -> Self {
        Val::F32(v)
    }
}
impl From<f64> for Val {
    fn from(v: f64) -> Self {
        Val::F64(v)
    }
}

impl core::fmt::Display for Val {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Val::I32(v) => write!(f, "{v}"),
            Val::I64(v) => write!(f, "{v}"),
            Val::F32(v) => write!(f, "{v}"),
            Val::F64(v) => write!(f, "{v}"),
        }
    }
}

/// Whether a Wasm global may be mutated after instantiation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Mutability {
    /// An immutable (`const`) global.
    Const,
    /// A mutable global.
    Mutable,
}

/// The type of a Wasm global: its content type and mutability.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GlobalType {
    content: ValType,
    mutability: Mutability,
}

impl GlobalType {
    /// Creates a new [`GlobalType`] from its content type and mutability.
    pub fn new(content: ValType, mutability: Mutability) -> Self {
        Self {
            content,
            mutability,
        }
    }

    /// Returns the value type stored in the global.
    pub fn content(&self) -> ValType {
        self.content
    }

    /// Returns the mutability of the global.
    pub fn mutable(&self) -> Mutability {
        self.mutability
    }
}

/// The type of a Wasm linear memory: its page limits.
///
/// Page counts are in units of Wasm pages (64 KiB each), matching the module's
/// declared limits rather than the byte length of the live data.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MemoryType {
    minimum: u64,
    maximum: Option<u64>,
}

impl MemoryType {
    /// Creates a new [`MemoryType`] from its minimum and optional maximum page
    /// counts.
    pub fn new(minimum: u64, maximum: Option<u64>) -> Self {
        Self { minimum, maximum }
    }

    /// Returns the minimum number of Wasm pages.
    pub fn minimum(&self) -> u64 {
        self.minimum
    }

    /// Returns the optional maximum number of Wasm pages.
    pub fn maximum(&self) -> Option<u64> {
        self.maximum
    }
}

/// The signature of a Wasm function: its parameter and result types.
///
/// Mirrors the shape used by `wasmi`/`wasmtime` for easy interoperation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    params: Box<[ValType]>,
    results: Box<[ValType]>,
}

impl FuncType {
    /// Creates a new [`FuncType`] from its parameter and result types.
    pub fn new(
        params: impl IntoIterator<Item = ValType>,
        results: impl IntoIterator<Item = ValType>,
    ) -> Self {
        Self {
            params: params.into_iter().collect(),
            results: results.into_iter().collect(),
        }
    }

    /// Returns the parameter types.
    pub fn params(&self) -> &[ValType] {
        &self.params
    }

    /// Returns the result types.
    pub fn results(&self) -> &[ValType] {
        &self.results
    }

    /// Builds the WAMR native signature string for this type, e.g. `"(ii)i"`.
    ///
    /// Used purely to satisfy WAMR's symbol registration; the raw calling
    /// convention derives the actual argument layout from the module's own
    /// import type, so only the arity/shape needs to be well-formed here.
    pub(crate) fn signature_cstring(&self) -> std::ffi::CString {
        let mut s = Vec::with_capacity(self.params.len() + self.results.len() + 2);
        s.push(b'(');
        s.extend(self.params.iter().map(|t| t.signature_char()));
        s.push(b')');
        s.extend(self.results.iter().map(|t| t.signature_char()));
        // No interior NUL bytes are possible from signature chars.
        std::ffi::CString::new(s).expect("signature contains no NUL bytes")
    }
}
