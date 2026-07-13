//! The [`Instance`]: an instantiated Wasm module ready to be called.

use crate::config::InstanceConfig;
use crate::error::{Error, Result};
use crate::func::Func;
use crate::global::Global;
use crate::linker::LinkerState;
use crate::module::Module;
use crate::util::with_error_buf;
use crate::value::{Mutability, ValType};
use std::ffi::CString;
use std::rc::Rc;
use wamrx_sys as sys;

/// An instantiated Wasm module.
///
/// Owns its [`Module`] and a WAMR execution environment. Look up exported
/// functions with [`Instance::get_func`].
pub struct Instance {
    module_inst: sys::wasm_module_inst_t,
    exec_env: sys::wasm_exec_env_t,
    /// Owns the module (keeps its handle + backing bytes + engine alive).
    _module: Module,
    /// Keeps the linker's globally-registered host functions alive for as long
    /// as this instance can call them.
    _linker_state: Rc<LinkerState>,
}

impl Instance {
    /// Instantiates `module`, keeping `linker_state`'s host functions alive.
    ///
    /// Host functions must already be registered (the linker does this before
    /// calling here).
    pub(crate) fn new(
        module: Module,
        linker_state: Rc<LinkerState>,
        config: &InstanceConfig,
    ) -> Result<Instance> {
        // This thread must have a WAMR execution environment before we
        // instantiate or create an exec env on it.
        crate::thread::ensure_thread_env();

        let (module_inst, err) = with_error_buf(|err_buf, err_size| unsafe {
            sys::wasm_runtime_instantiate(
                module.raw(),
                config.aux_stack_size,
                config.heap_size,
                err_buf,
                err_size,
            )
        });
        if module_inst.is_null() {
            return Err(Error::Instantiate(err));
        }

        // SAFETY: `module_inst` is a live instance handle.
        let exec_env =
            unsafe { sys::wasm_runtime_create_exec_env(module_inst, config.exec_stack_size) };
        if exec_env.is_null() {
            unsafe { sys::wasm_runtime_deinstantiate(module_inst) };
            return Err(Error::Instantiate(
                "failed to create execution environment".to_string(),
            ));
        }

        Ok(Instance {
            module_inst,
            exec_env,
            _module: module,
            _linker_state: linker_state,
        })
    }

    /// Looks up the exported function named `name`.
    pub fn get_func(&self, name: &str) -> Result<Func<'_>> {
        let cname = CString::new(name).map_err(|_| Error::FuncNotFound(name.to_string()))?;
        // SAFETY: `module_inst` is live; `cname` is a valid C string.
        let raw = unsafe { sys::wasm_runtime_lookup_function(self.module_inst, cname.as_ptr()) };
        if raw.is_null() {
            return Err(Error::FuncNotFound(name.to_string()));
        }
        Ok(Func::new(self, raw))
    }

    /// Looks up the exported global named `name`.
    ///
    /// Returns [`Error::UnsupportedType`] if the global's content type is not
    /// one of the four numeric types these bindings model.
    pub fn get_global(&self, name: &str) -> Result<Global<'_>> {
        let cname = CString::new(name).map_err(|_| Error::GlobalNotFound(name.to_string()))?;
        let mut raw: sys::wasm_global_inst_t = unsafe { core::mem::zeroed() };
        // SAFETY: `module_inst` is live; `cname` is a valid C string; `raw` is a
        // valid out-pointer.
        let ok = unsafe {
            sys::wasm_runtime_get_export_global_inst(self.module_inst, cname.as_ptr(), &mut raw)
        };
        if !ok {
            return Err(Error::GlobalNotFound(name.to_string()));
        }
        let content = ValType::from_valkind(raw.kind).ok_or(Error::UnsupportedType)?;
        let mutability = if raw.is_mutable {
            Mutability::Mutable
        } else {
            Mutability::Const
        };
        Ok(Global::new(content, mutability, raw.global_data))
    }

    pub(crate) fn module_inst(&self) -> sys::wasm_module_inst_t {
        self.module_inst
    }

    pub(crate) fn exec_env(&self) -> sys::wasm_exec_env_t {
        self.exec_env
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: both handles are live and owned exclusively by this instance.
        // Tear down in reverse order of creation: exec env, then instance. The
        // owned `Module` unloads afterwards as its field is dropped.
        unsafe {
            sys::wasm_runtime_destroy_exec_env(self.exec_env);
            sys::wasm_runtime_deinstantiate(self.module_inst);
        }
    }
}
