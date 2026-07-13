//! Configuration for instantiating a [`Module`](crate::Module).

/// Auxiliary (linear-memory) stack size carved out at instantiation, in bytes.
const DEFAULT_AUX_STACK_SIZE: u32 = 64 * 1024;
/// Host-managed app heap size, in bytes. `0` means "no extra app heap"; modules
/// that only use their own linear memory don't need one.
const DEFAULT_HEAP_SIZE: u32 = 0;
/// Interpreter execution-stack size per instance, in bytes. Generous by default
/// so deeply recursive workloads don't overflow it.
const DEFAULT_EXEC_STACK_SIZE: u32 = 8 * 1024 * 1024;

/// Per-instance resource sizes used when instantiating a module.
///
/// These map directly onto the sizes WAMR takes as arguments to
/// `wasm_runtime_instantiate` and `wasm_runtime_create_exec_env`. Unlike
/// `wasmtime`/`wasmi` — where such limits live on an engine-wide `Config` —
/// WAMR sets them per instantiation, so they live here rather than on
/// [`Engine`](crate::Engine).
///
/// Build one with the [`Default`] impl and the builder-style setters:
///
/// ```
/// use wamrx::InstanceConfig;
///
/// let mut config = InstanceConfig::new();
/// config.exec_stack_size(4 * 1024 * 1024).heap_size(64 * 1024);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceConfig {
    pub(crate) aux_stack_size: u32,
    pub(crate) heap_size: u32,
    pub(crate) exec_stack_size: u32,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            aux_stack_size: DEFAULT_AUX_STACK_SIZE,
            heap_size: DEFAULT_HEAP_SIZE,
            exec_stack_size: DEFAULT_EXEC_STACK_SIZE,
        }
    }
}

impl InstanceConfig {
    /// Creates a new configuration with default sizes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the auxiliary (linear-memory) stack size, in bytes.
    ///
    /// Passed as `wasm_runtime_instantiate`'s `default_stack_size`.
    pub fn aux_stack_size(&mut self, bytes: u32) -> &mut Self {
        self.aux_stack_size = bytes;
        self
    }

    /// Sets the host-managed app heap size, in bytes (`0` for none).
    ///
    /// Passed as `wasm_runtime_instantiate`'s `host_managed_heap_size`; backs
    /// `wasm_runtime_module_malloc`-style allocations.
    pub fn heap_size(&mut self, bytes: u32) -> &mut Self {
        self.heap_size = bytes;
        self
    }

    /// Sets the interpreter execution-stack size per instance, in bytes.
    ///
    /// Passed as `wasm_runtime_create_exec_env`'s `stack_size`; bounds Wasm
    /// call depth.
    pub fn exec_stack_size(&mut self, bytes: u32) -> &mut Self {
        self.exec_stack_size = bytes;
        self
    }
}
