use crate::config::LauberhornConfig;
use crate::lauberhorn::codec::LauberhornRpcEndpoint;
use crate::lauberhorn::ffi::LauberhornExecCtx;
use crate::lauberhorn::ffi::LauberhornHandler;
use crate::lauberhorn::ffi::LauberhornHandlerFunc;
use crate::lauberhorn::ffi::RpcCodec;
use crate::lauberhorn::ffi::RpcOps;
use crate::lauberhorn::ffi::dandelion_function_free;
use crate::lauberhorn::ffi::dandelion_function_handler;
use crate::lauberhorn::lauberhorn::Lauberhorn;
use crate::lauberhorn::marshal::DandelionRPCRequest;
use crate::lauberhorn::marshal::DandelionRPCResponse;
use crossbeam::queue::ArrayQueue;
use dandelion_commons::DandelionError;
use dandelion_commons::DandelionResult;
use dandelion_commons::FunctionId;
use dispatcher::function_registry::{FunctionRegistry, FunctionType};
use log::debug;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::{
    get_available_domains, DomainType, EngineType,
};
use machine_interface::memory_domain::{MemoryDomain, MemoryResource};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::LazyLock;

static DANDELION_RPC_OPS_SERVER: RpcOps = RpcOps::for_server::<DandelionRPCRequest, DandelionRPCResponse>();
static DANDELION_RPC_OPS_CLIENT: RpcOps = RpcOps::for_client::<DandelionRPCRequest, DandelionRPCResponse>();

static DANDELION_RPC_SERVER_CODEC: LazyLock<Arc<RpcCodec>> = LazyLock::new(|| {
    Arc::new(RpcCodec {
        // for_server, for_client ? 
        ops: &DANDELION_RPC_OPS_SERVER,
        private: std::ptr::null(),
    })
});

/// Context that the runtime exposes to functions
pub struct RuntimeContext<E: Engine> {
    pub engines: ArrayQueue<E>,
    // pub nested_results: ObjectPool<64, Vec<Option<CompositionSet>>>,
    pub registry: Arc<FunctionRegistry>,
    // TODO not necessary anymor e? 
    pub nested_ep: Arc<LauberhornRpcEndpoint<DandelionRPCRequest, DandelionRPCResponse>>
}

/// Runtime
pub struct Runtime<E: Engine> {
    ctx: Arc<RuntimeContext<E>>,
    lauberhorn: Lauberhorn,
    domains: Vec<Arc<Box<dyn MemoryDomain>>>,
}

unsafe impl<E: Engine> Send for Runtime<E> {}
unsafe impl<E: Engine> Sync for Runtime<E> {}

// DOnt we already have this implemented in drop for lauebrhorn ? 
// probably not neede d here anymore ? 
impl<E: Engine> Drop for Runtime<E> {
    fn drop(&mut self) {
        debug!("shutting down lauberhorn workers");
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
        self.ctx.registry.get_function(function_id).ok()
    }

    /// Initialize the runtime.
    ///
    /// Sets up engines, memory domains, function registry
    pub fn init(
        config: LauberhornConfig,
        memory_pool: BTreeMap<DomainType, MemoryResource>,
    ) -> DandelionResult<Self> {

        let engine_queue = ArrayQueue::new(config.num_cores);
        for i in 0..config.num_cores {
            engine_queue.push(*E::init(i as u8).unwrap());
        }

        let domains = get_available_domains(memory_pool);
        let registry = Arc::new(FunctionRegistry::new(&domains));
        let lauberhorn = Lauberhorn::init()?;

        // register unique handler invocation RPC
        // TODO(@sven): use user configured values via config.rs in server crate
        debug!("registering lauberhorn function handler under prog_num={}, prog_ver={}, proc_num={}, port={}", 1, 1, 1, 11111);

        let nested_ep: Arc<LauberhornRpcEndpoint<DandelionRPCRequest, DandelionRPCResponse>> = Arc::new(LauberhornRpcEndpoint::new(
            &config.daddr,
            config.port,
            config.prog_num, 
            config.prog_ver,
            config.proc_num,
            &DANDELION_RPC_OPS_CLIENT as *const RpcOps as *mut RpcOps
        ));

        let rt_ctx = Arc::new(RuntimeContext{
            engines: engine_queue,
            // nested_results: ObjectPool::new(Vec::with_capacity(64)),
            registry: registry.clone(),
            nested_ep: nested_ep.clone()
        });

        // handler must not outlive runtime
        let private = Arc::as_ptr(&rt_ctx) as *mut c_void; 
        // TODO(@sven): use user configured values via config.rs in server crate
        
        lauberhorn.register_service( 
            // INFO: as we only have one entry point, the top level function must be executed in a fiber as
            // we don't know if the user invokes a composition or a simple function
            // TODO: create separate endpoints / services for compositions / individual functions
            LauberhornHandler {
             data: private, 
             func: dandelion_function_handler::<E> as LauberhornHandlerFunc,
             free: dandelion_function_free,   
             mode: LauberhornExecCtx::ExecFiber
            },
            config.prog_num,
            config.prog_ver,
            config.proc_num,
            config.port,
            (*&DANDELION_RPC_SERVER_CODEC).clone()
        )?;

        Ok(Runtime { 
            ctx: rt_ctx, 
            lauberhorn: lauberhorn, 
            domains: domains 
        })
        
    }

    pub fn register_composition(
        &self,
        composition_desc: &str,
    ) -> DandelionResult<()> {
        debug!("Registering composition {}", composition_desc);
        self.ctx.registry.insert_compositions(composition_desc)
    }

    pub fn deregister_function(
        &self,
        function_name: String,
    ) -> DandelionResult<()> {
        debug!("Deregistering function {}", function_name);
        self.ctx.registry.remove(&function_name)
    }

    /// Register a function with the runtime's function registry.
    pub fn register_function(
        &self,
        function_name: String,
        engine_type: EngineType,
        ctx_size: usize,
        path: String,
        metadata: Metadata,
    ) -> DandelionResult<()> {
        let domain_type = engine_type.get_domain_type();

        debug!(
            "Registering function '{}' with engine {:?} and domain {:?}",
            function_name, engine_type, domain_type
        );

        let memory_domain: &Arc<Box<dyn MemoryDomain>> = self
            .domains
            .get(domain_type as usize)
            .ok_or(DandelionError::FunctionRegistry(
                dandelion_commons::FunctionRegistryError::DuplicateInsert(
                    "error".into(),
                ),
            ))?;

        // insert function into registry
        self.ctx.registry.insert_function(
            Arc::new(function_name),
            engine_type,
            memory_domain.clone(),
            ctx_size,
            path,
            metadata,
        )
    }

    /// Start lauberhorn workers (one per engine) and block until they finish.
    pub fn run(&mut self) -> Result<(), ()> {
        debug!(
            "Running runtime with {} engines and {} memory domains",
            self.ctx.engines.len(),
            self.domains.len()
        );

        for _ in 0..self.ctx.engines.len() {
            self.lauberhorn.create_worker(None, None);
        }
        Ok(())
    }
}

/// Create a runtime with the engine type selected by the active feature flag.
///
/// The concrete engine type is hidden behind `impl Engine` so callers
/// do not need to name `MmuLoop` / `KvmLoop` / `CheriLoop`.
pub fn create_runtime(
    config: LauberhornConfig,
    memory_pool: BTreeMap<DomainType, MemoryResource>,
) -> DandelionResult<Runtime<impl Engine>> {
    #[cfg(feature = "mmu")]
    {
        use machine_interface::function_driver::compute_driver::mmu::MmuLoop;
        Runtime::<MmuLoop>::init(config, memory_pool)
    }
    #[cfg(feature = "kvm")]
    {
        use machine_interface::function_driver::compute_driver::kvm::KvmLoop;
        Runtime::<KvmLoop>::init(config, memory_pool)
    }
    #[cfg(feature = "cheri")]
    {
        use machine_interface::function_driver::compute_driver::cheri::CheriLoop;
        Runtime::<CheriLoop>::init(config, memory_pool)
    }
}
