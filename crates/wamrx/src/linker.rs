//! The [`Linker`]: defines host functions and instantiates [`Module`]s.
//!
//! # WAMR host-function model (honest divergence from Wasmtime)
//!
//! Two WAMR invariants shape this API:
//!
//! 1. **Global registry.** WAMR resolves imported host functions from a
//!    *process-global* registry keyed by module name
//!    (`wasm_runtime_register_natives*`), not from per-store state. The
//!    [`Linker`] owns that registration; it lives as long as the `Linker` (or
//!    any [`Instance`] it produced) and is torn down on drop.
//!
//! 2. **Imports are resolved at module *load* time**, not at instantiation.
//!    Therefore every host function must be defined via [`Linker::define_func`]
//!    **before** the [`Module`] that imports it is created with
//!    [`Module::new`](crate::Module::new). `define_func` registers with WAMR
//!    immediately for this reason.
//!
//! Host functions are dispatched through a single generic C trampoline using
//! WAMR's "raw" calling convention (`wasm_runtime_register_natives_raw`): each
//! argument occupies one 64-bit slot and a single result is written back to
//! slot 0. This is what lets one trampoline serve arbitrary Rust `fn`s. As a
//! consequence, host functions support the four numeric types and at most one
//! result.

use crate::config::InstanceConfig;
use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::module::Module;
use crate::value::{FuncType, Val};
use std::cell::RefCell;
use std::ffi::{CString, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use wamrx_sys as sys;

/// The boxed Rust closure backing a `wamrx` host function.
///
/// Accepting any `Fn(&[Val], &mut [Val])` (not just a bare `fn`) lets
/// callers capture host state and adapt foreign host-function signatures by
/// wrapping them in a converting closure.
pub type HostFunc = Box<dyn Fn(&[Val], &mut [Val]) + 'static>;

/// Per-host-function context carried through WAMR's `attachment` pointer and
/// read back by [`raw_trampoline`].
struct HostFuncCtx {
    ty: FuncType,
    func: HostFunc,
}

/// One registered host function plus the keep-alive storage for every pointer
/// embedded in `symbol`, which WAMR retains by reference after registration.
/// Nothing here may be moved out or reallocated while registered.
struct NativeEntry {
    module_name: CString,
    _field: CString,
    _signature: CString,
    _context: Box<HostFuncCtx>,
    /// Single-element array registered with WAMR (kept at a stable address).
    symbol: Box<[sys::NativeSymbol; 1]>,
}

impl NativeEntry {
    /// Builds and eagerly registers a host function with WAMR's global registry.
    fn register(module: &str, name: &str, ty: FuncType, func: HostFunc) -> Result<NativeEntry> {
        let module_name = CString::new(module).map_err(|_| interior_nul("module name"))?;
        let field = CString::new(name).map_err(|_| interior_nul("function name"))?;
        let signature = ty.signature_cstring();
        let context = Box::new(HostFuncCtx { ty, func });

        // Pointers reference heap buffers owned by the fields above; those
        // owners are never moved out of `NativeEntry`, so the pointers remain
        // valid for as long as the entry lives.
        let mut symbol = Box::new([sys::NativeSymbol {
            symbol: field.as_ptr(),
            func_ptr: raw_trampoline as *mut c_void,
            signature: signature.as_ptr(),
            attachment: (&*context as *const HostFuncCtx) as *mut c_void,
        }]);

        // SAFETY: the array is a valid single `NativeSymbol`; WAMR stores the
        // pointer, which stays valid because `symbol` is boxed and owned here.
        let ok = unsafe {
            sys::wasm_runtime_register_natives_raw(module_name.as_ptr(), symbol.as_mut_ptr(), 1)
        };
        if !ok {
            return Err(Error::Instantiate(
                "failed to register host function with WAMR".to_string(),
            ));
        }

        Ok(NativeEntry {
            module_name,
            _field: field,
            _signature: signature,
            _context: context,
            symbol,
        })
    }
}

fn interior_nul(what: &str) -> Error {
    Error::Instantiate(format!("{what} contains an interior NUL byte"))
}

/// Registration storage shared between a [`Linker`] and the [`Instance`]s it
/// produces. Kept behind an [`Rc`] so instances keep the natives alive.
pub struct LinkerState {
    // Held so the runtime stays alive through registration and teardown.
    _engine: Engine,
    entries: RefCell<Vec<NativeEntry>>,
}

impl Drop for LinkerState {
    fn drop(&mut self) {
        for entry in self.entries.borrow_mut().iter_mut() {
            // SAFETY: exactly the arrays we registered and have not yet freed;
            // unregistering drops WAMR's reference before the storage is freed.
            unsafe {
                sys::wasm_runtime_unregister_natives(
                    entry.module_name.as_ptr(),
                    entry.symbol.as_mut_ptr(),
                );
            }
        }
    }
}

/// Defines host functions and instantiates [`Module`]s against them.
///
/// Wasmtime-inspired, but the module-level documentation explains how WAMR's
/// global native registry and load-time import resolution shape the semantics.
pub struct Linker {
    state: Rc<LinkerState>,
}

impl Linker {
    /// Creates a new, empty linker for `engine`.
    pub fn new(engine: &Engine) -> Linker {
        Linker {
            state: Rc::new(LinkerState {
                _engine: engine.clone(),
                entries: RefCell::new(Vec::new()),
            }),
        }
    }

    /// Defines and registers host function `module::name` with signature `ty`.
    ///
    /// Registers with WAMR immediately, so this **must** be called before the
    /// [`Module`] importing it is loaded (see the module-level documentation).
    /// `func` may take the four numeric value types and return at most one
    /// result, and may be any `Fn` (bare function or closure).
    pub fn define_func(
        &mut self,
        module: &str,
        name: &str,
        ty: FuncType,
        func: impl Fn(&[Val], &mut [Val]) + 'static,
    ) -> Result<&mut Self> {
        let entry = NativeEntry::register(module, name, ty, Box::new(func))?;
        self.state.entries.borrow_mut().push(entry);
        Ok(self)
    }

    /// Instantiates `module` using this linker's host functions and the default
    /// [`InstanceConfig`].
    pub fn instantiate(&self, module: Module) -> Result<Instance> {
        self.instantiate_with(module, &InstanceConfig::default())
    }

    /// Instantiates `module` with an explicit [`InstanceConfig`] controlling the
    /// auxiliary-stack, app-heap, and interpreter execution-stack sizes.
    pub fn instantiate_with(&self, module: Module, config: &InstanceConfig) -> Result<Instance> {
        Instance::new(module, Rc::clone(&self.state), config)
    }
}

/// The single generic C trampoline used for every registered host function.
///
/// WAMR's raw convention hands us `argv`, an array of 64-bit slots — one per
/// parameter — and expects a single result written back to `argv[0]`. The
/// per-function [`HostFuncCtx`] (types + Rust `fn`) travels via the
/// `attachment` pointer.
unsafe extern "C" fn raw_trampoline(exec_env: sys::wasm_exec_env_t, argv: *mut u64) {
    let attachment = sys::wasm_runtime_get_function_attachment(exec_env);
    if attachment.is_null() {
        return;
    }
    // SAFETY: `attachment` is the `HostFuncCtx` we registered; it outlives every
    // call because the owning `LinkerState` is kept alive by live instances.
    let ctx = &*(attachment as *const HostFuncCtx);

    // Decode one 64-bit slot per parameter.
    let params: Vec<Val> = ctx
        .ty
        .params()
        .iter()
        .enumerate()
        .map(|(i, &ty)| Val::from_raw_slot(ty, *argv.add(i)))
        .collect();
    let mut results: Vec<Val> = ctx.ty.results().iter().map(|t| t.default_value()).collect();

    // Guard the FFI boundary: unwinding a panic across `extern "C"` is UB, so
    // convert it into a Wasm trap instead.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        (ctx.func)(&params, &mut results);
    }));

    match outcome {
        Ok(()) => {
            if let Some(result) = results.first() {
                *argv = result.to_raw_slot();
            }
        }
        Err(_) => {
            let module_inst = sys::wasm_runtime_get_module_inst(exec_env);
            let msg = CString::new("host function panicked").unwrap();
            sys::wasm_runtime_set_exception(module_inst, msg.as_ptr());
        }
    }
}
