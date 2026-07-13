//! Error type for the `wamrx` high-level API.

use crate::value::ValType;

/// Errors returned by `wamrx` operations.
///
/// Implements [`std::error::Error`], so it converts cleanly into `anyhow::Error`
/// and other error-handling frameworks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Global WAMR runtime initialization failed.
    RuntimeInit,
    /// A module failed to load/validate. Carries WAMR's diagnostic message.
    ModuleLoad(String),
    /// A module failed to instantiate. Carries WAMR's diagnostic message.
    Instantiate(String),
    /// No exported function with the requested name was found.
    FuncNotFound(String),
    /// A call trapped or otherwise failed. Carries WAMR's exception string.
    Trap(String),
    /// The number of provided arguments/results did not match the callee.
    SignatureMismatch {
        /// What was expected (e.g. parameter count).
        expected: usize,
        /// What was actually provided.
        provided: usize,
    },
    /// No exported global with the requested name was found.
    GlobalNotFound(String),
    /// Attempted to set an immutable (`const`) global.
    GlobalImmutable,
    /// A value's type did not match the expected type.
    TypeMismatch {
        /// The type that was required.
        expected: ValType,
        /// The type that was provided.
        provided: ValType,
    },
    /// Encountered a Wasm value type these bindings do not model (only the four
    /// numeric types `i32`/`i64`/`f32`/`f64` are supported).
    UnsupportedType,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::RuntimeInit => write!(f, "failed to initialize the WAMR runtime"),
            Error::ModuleLoad(msg) => write!(f, "failed to load Wasm module: {msg}"),
            Error::Instantiate(msg) => write!(f, "failed to instantiate Wasm module: {msg}"),
            Error::FuncNotFound(name) => write!(f, "exported function not found: {name}"),
            Error::Trap(msg) => write!(f, "Wasm trap: {msg}"),
            Error::SignatureMismatch { expected, provided } => {
                write!(
                    f,
                    "signature mismatch: expected {expected}, provided {provided}"
                )
            }
            Error::GlobalNotFound(name) => write!(f, "exported global not found: {name}"),
            Error::GlobalImmutable => write!(f, "cannot set an immutable global"),
            Error::TypeMismatch { expected, provided } => {
                write!(f, "type mismatch: expected {expected}, provided {provided}")
            }
            Error::UnsupportedType => {
                write!(f, "unsupported Wasm value type (only i32/i64/f32/f64)")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Convenience result alias for `wamrx`.
pub type Result<T> = core::result::Result<T, Error>;
