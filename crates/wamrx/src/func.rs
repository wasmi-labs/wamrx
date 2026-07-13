//! Exported Wasm functions and how to call them.

use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::util::cstr_ptr_to_string;
use crate::value::{FuncType, Val, ValType};
use wamrx_sys as sys;

/// A handle to a function exported by an [`Instance`].
///
/// Borrows its [`Instance`]; obtain one via [`Instance::get_func`].
pub struct Func<'a> {
    instance: &'a Instance,
    raw: sys::wasm_function_inst_t,
}

impl core::fmt::Debug for Func<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Func").field("ty", &self.ty()).finish()
    }
}

impl<'a> Func<'a> {
    pub(crate) fn new(instance: &'a Instance, raw: sys::wasm_function_inst_t) -> Self {
        Func { instance, raw }
    }

    /// The number of parameters this function expects.
    ///
    /// Private: the public accessor is [`Func::ty`]; this cheap count is kept
    /// for internal use (e.g. sizing buffers) without allocating a `FuncType`.
    fn len_params(&self) -> usize {
        // SAFETY: `raw` and the instance handle are both live.
        unsafe { sys::wasm_func_get_param_count(self.raw, self.instance.module_inst()) as usize }
    }

    /// The number of results this function returns (private; see [`Func::ty`]).
    fn len_results(&self) -> usize {
        // SAFETY: `raw` and the instance handle are both live.
        unsafe { sys::wasm_func_get_result_count(self.raw, self.instance.module_inst()) as usize }
    }

    /// Returns the [`FuncType`] (parameter and result types) of this function.
    ///
    /// Non-numeric parameter/result kinds (which the four-type [`ValType`] does
    /// not model) are reported as [`ValType::I32`]; in practice these bindings
    /// only deal with numeric signatures.
    pub fn ty(&self) -> FuncType {
        let module_inst = self.instance.module_inst();
        let n_params = self.len_params();
        let n_results = self.len_results();

        let mut param_kinds = vec![0 as sys::wasm_valkind_t; n_params];
        let mut result_kinds = vec![0 as sys::wasm_valkind_t; n_results];
        // SAFETY: `raw`/`module_inst` are live and each buffer is sized to the
        // matching count queried above.
        unsafe {
            sys::wasm_func_get_param_types(self.raw, module_inst, param_kinds.as_mut_ptr());
            sys::wasm_func_get_result_types(self.raw, module_inst, result_kinds.as_mut_ptr());
        }

        let to_ty = |k: &sys::wasm_valkind_t| ValType::from_valkind(*k).unwrap_or(ValType::I32);
        FuncType::new(
            param_kinds.iter().map(to_ty),
            result_kinds.iter().map(to_ty),
        )
    }

    /// Calls the function with `params`, writing outputs into `results`.
    ///
    /// `results` must be large enough to hold the function's result values; any
    /// extra slots are left untouched.
    pub fn call(&self, params: &[Val], results: &mut [Val]) -> Result<()> {
        // Calls execute on the current thread; ensure its WAMR env is ready.
        crate::thread::ensure_thread_env();

        let module_inst = self.instance.module_inst();
        let exec_env = self.instance.exec_env();

        let n_results = self.len_results();
        if results.len() < n_results {
            return Err(Error::SignatureMismatch {
                expected: n_results,
                provided: results.len(),
            });
        }

        let mut args: Vec<sys::wasm_val_t> = params.iter().map(|v| v.to_wasm_val()).collect();
        let mut raw_results: Vec<sys::wasm_val_t> = vec![unsafe { core::mem::zeroed() }; n_results];

        // SAFETY: `exec_env`/`raw` are live; buffers are correctly sized and the
        // pointers are valid for the given counts (empty `Vec`s yield a
        // dangling-but-non-dereferenced pointer, which WAMR won't read).
        let ok = unsafe {
            sys::wasm_runtime_call_wasm_a(
                exec_env,
                self.raw,
                n_results as u32,
                raw_results.as_mut_ptr(),
                args.len() as u32,
                args.as_mut_ptr(),
            )
        };

        if !ok {
            // SAFETY: on failure WAMR records an exception on the instance.
            let exc = unsafe { cstr_ptr_to_string(sys::wasm_runtime_get_exception(module_inst)) };
            return Err(Error::Trap(exc));
        }

        for (dst, raw) in results.iter_mut().zip(raw_results.iter()) {
            *dst = Val::from_wasm_val(raw);
        }
        Ok(())
    }
}
