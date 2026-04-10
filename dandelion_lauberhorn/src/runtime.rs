use std::collections::BTreeMap;
use std::sync::Arc;

use dandelion_commons::FunctionId;
use dispatcher::function_registry::{FunctionRegistry, FunctionType};
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::{get_available_domains, DomainType, EngineType};
use machine_interface::memory_domain::{MemoryDomain, MemoryResource};

use crate::lauberhorn::Lauberhorn;
use crate::lauberhorn_types::LauberhornServiceCtx;

pub struct Runtime<E: Engine> {
    registry: Arc<FunctionRegistry>,
    lauberhorn: Lauberhorn,
    engines: Vec<Box<E>>,
    domains: Vec<Arc<Box<dyn MemoryDomain>>>,
}

// SAFETY: Runtime is shared via Arc and accessed through &self methods only.
// The raw pointers inside Lauberhorn are C FFI handles with a managed lifecycle
// (init on creation, join_workers on drop) and are not mutated concurrently.
unsafe impl<E: Engine> Send for Runtime<E> {}
unsafe impl<E: Engine> Sync for Runtime<E> {}

/// guarantee that runtime unwinds lauberhorn workers on drop,
/// to ensure clean shutdown of lauberhorn and avoid dangling workers
impl<E: Engine> Drop for Runtime<E> {
    fn drop(&mut self) {
        // ensure lauberhorn workers are stopped when runtime is dropped
        self.lauberhorn.join_workers();
    }
}

impl<E: Engine> Runtime<E> {
    /// Get function from registry
    pub fn get_registered_function(
        &self,
        function_id: &FunctionId,
    ) -> Option<FunctionType> {
        self.registry.get_function(function_id).ok()
    }

    /// Initialize the runtime.
    ///
    /// Sets up engines, memory domains, function registry, and the lauberhorn
    /// RPC subsystem.
    pub fn init(
        memory_pool: BTreeMap<DomainType, MemoryResource>,
    ) -> Result<Self, ()> {
        let engines: Vec<Box<E>> = vec![E::init(0).unwrap()];
        let domains = get_available_domains(memory_pool);
        let registry = Arc::new(FunctionRegistry::new(&domains));
        let lauberhorn = Lauberhorn::init().map_err(|_| ())?;

        Ok(Runtime {
            registry,
            lauberhorn,
            engines,
            domains,
        })
    }

    /// Register a service with lauberhorn.
    ///
    /// The function identified by `function_id` must already be registered
    /// via [`register_function`](Self::register_function).
    pub fn register_service(
        &self,
        function_id: FunctionId,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
    ) -> Result<(), ()> {
        let srv_ctx = Box::new(LauberhornServiceCtx {
            function_registry: self.registry.clone(),
            engines: self
                .engines
                .iter()
                .map(|e| &**e as *const E as *mut E)
                .collect(),
            function_id: function_id.clone(),
            id: 0,
        });

        self.lauberhorn
            .register_service(srv_ctx, prog_num, prog_ver, proc_num, listen_port)
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Register a function with the runtime's function registry.
    pub fn register_function(
        &self,
        function_name: String,
        engine_type: EngineType,
        ctx_size: usize,
        path: String,
        metadata: Metadata,
    ) -> Result<(), ()> {
        let domain_type = engine_type.get_domain_type();
        let memory_domain = self.domains.get(domain_type as usize).ok_or(())?;

        /// insert function into registry
        self.registry.insert_function(
            Arc::new(function_name),
            engine_type,
            memory_domain.clone(),
            ctx_size,
            path,
            metadata,
        );
        Ok(())
    }

    /// Start lauberhorn workers (one per engine) and block until they finish.
    pub fn run(&mut self) -> Result<(), ()> {
        for _ in &self.engines {
            self.lauberhorn.create_worker(None, None);
        }
        self.lauberhorn.join_workers();
        Ok(())
    }
}

/// Create a runtime with the engine type selected by the active feature flag.
///
/// The concrete engine type is hidden behind `impl Engine` so callers
/// do not need to name `MmuLoop` / `KvmLoop` / `CheriLoop`.
pub fn create_runtime(
    memory_pool: BTreeMap<DomainType, MemoryResource>,
) -> Result<Runtime<impl Engine>, ()> {
    #[cfg(feature = "mmu")]
    {
        use machine_interface::function_driver::compute_driver::mmu::MmuLoop;
        Runtime::<MmuLoop>::init(memory_pool)
    }
    #[cfg(feature = "kvm")]
    {
        use machine_interface::function_driver::compute_driver::kvm::KvmLoop;
        Runtime::<KvmLoop>::init(memory_pool)
    }
    #[cfg(feature = "cheri")]
    {
        use machine_interface::function_driver::compute_driver::cheri::CheriLoop;
        Runtime::<CheriLoop>::init(memory_pool)
    }
}
