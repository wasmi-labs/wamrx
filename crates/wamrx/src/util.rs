//! Small internal FFI helpers shared across modules.

use std::ffi::c_char;

/// Size of the stack buffer WAMR writes diagnostic messages into.
pub(crate) const ERROR_BUF_SIZE: usize = 256;

/// Runs `f` with a fresh zeroed error buffer and returns the message WAMR wrote
/// into it (empty if none).
pub(crate) fn with_error_buf<T>(f: impl FnOnce(*mut c_char, u32) -> T) -> (T, String) {
    let mut buf = [0 as c_char; ERROR_BUF_SIZE];
    let ret = f(buf.as_mut_ptr(), buf.len() as u32);
    (ret, cstr_buf_to_string(&buf))
}

/// Converts a NUL-terminated C buffer into an owned `String` (lossily).
pub(crate) fn cstr_buf_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Converts a borrowed C string pointer into an owned `String` (lossily).
///
/// # Safety
///
/// `ptr` must be null or point to a valid NUL-terminated C string.
pub(crate) unsafe fn cstr_ptr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
}
