//! The [`Instance`]: an instantiated Wasm module ready to be called.

use crate::config::InstanceConfig;
use crate::error::{Error, Result};
use crate::func::Func;
use crate::global::Global;
use crate::linker::LinkerState;
use crate::memory::Memory;
use crate::module::Module;
use crate::util::with_error_buf;
use crate::value::{Mutability, ValType};
use std::ffi::{CStr, CString};
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

    /// Looks up the exported linear memory named `name`.
    ///
    /// Returns [`Error::MemoryNotFound`] if no exported memory has that name.
    pub fn get_memory(&self, name: &str) -> Result<Memory<'_>> {
        // WAMR's `wasm_runtime_lookup_memory` ignores `name` with multi-memory
        // off (it returns the sole memory), so validate the export name here.
        if !self.has_exported_memory(name) {
            return Err(Error::MemoryNotFound(name.to_string()));
        }
        let cname = CString::new(name).map_err(|_| Error::MemoryNotFound(name.to_string()))?;
        // SAFETY: `module_inst` is live; `cname` is a valid C string.
        let inst = unsafe { sys::wasm_runtime_lookup_memory(self.module_inst, cname.as_ptr()) };
        if inst.is_null() {
            return Err(Error::MemoryNotFound(name.to_string()));
        }
        // The declared page limits come from the module's own bytes: WAMR
        // rewrites its in-memory type for non-growing modules (folding pages
        // into `num_bytes_per_page` and setting init = max = 1), so its runtime
        // structures no longer carry the declared min/max. At most one memory
        // exists (multi-memory is unsupported), so its parsed type applies.
        let ty = self
            ._module
            .memory_type()
            .ok_or_else(|| Error::MemoryNotFound(name.to_string()))?;
        Ok(Memory::new(inst, ty))
    }

    /// Returns whether the module exports a memory named `name`. Export names
    /// remain reliable even though WAMR rewrites the memory's page limits.
    fn has_exported_memory(&self, name: &str) -> bool {
        let module = self._module.raw();
        // SAFETY: `module` is a live module handle owned by this instance.
        let count = unsafe { sys::wasm_runtime_get_export_count(module) };
        (0..count).any(|i| {
            let mut export: sys::wasm_export_t = unsafe { core::mem::zeroed() };
            // SAFETY: `module` is live; `i` is in `0..count`; `export` is a
            // valid out-pointer.
            unsafe { sys::wasm_runtime_get_export_type(module, i, &mut export) };
            // SAFETY: `export.name` is a valid, NUL-terminated C string owned by
            // the module.
            export.kind == sys::WASM_IMPORT_EXPORT_KIND_MEMORY
                && unsafe { CStr::from_ptr(export.name) }.to_bytes() == name.as_bytes()
        })
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
