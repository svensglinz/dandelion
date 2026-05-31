use crate::lauberhorn::codec::LauberhornRpcEndpoint;
use crate::lauberhorn::ffi::LauberhornHandler;
use crate::lauberhorn::ffi::LauberhornHandlerFunc;
use crate::lauberhorn::ffi::RpcCodec;
use crate::lauberhorn::ffi::RpcOps;
use crate::lauberhorn::ffi::dandelion_function_free;
use crate::lauberhorn::ffi::dandelion_function_handler;
use crate::lauberhorn::lauberhorn::Lauberhorn;
use crate::lauberhorn::marshal::DandelionRPCRequest;
use crate::lauberhorn::marshal::DandelionRPCResponse;
use crate::utils::objectpool::ObjectPool;
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

const NUM_CORES: usize = 2; 

static DANDELION_RPC_OPS_SERVER: RpcOps = RpcOps::for_server::<DandelionRPCRequest, DandelionRPCResponse>();
static DANDELION_RPC_OPS_CLIENT: RpcOps = RpcOps::for_client::<DandelionRPCRequest, DandelionRPCResponse>();

static DANDELION_RPC_SERVER_CODEC: LazyLock<Arc<RpcCodec>> = LazyLock::new(|| {
    Arc::new(RpcCodec {
        // for_server, for_client ? 
        ops: &DANDELION_RPC_OPS_SERVER,
        private: std::ptr::null(),
    })
});

static DANDELION_RPC_CLIENT_CODEC: LazyLock<Arc<RpcCodec>> = LazyLock::new(|| {
    Arc::new(RpcCodec {
        // for_server, for_client ? 
        ops: &DANDELION_RPC_OPS_CLIENT,
        private: std::ptr::null(),
    })
});


static DANDELION_NESTED_EP: LazyLock<Arc<LauberhornRpcEndpoint<DandelionRPCRequest, DandelionRPCResponse>>> = 
    LazyLock::new(|| {
        Arc::new(LauberhornRpcEndpoint::new(
            "10.0.0.5", 
            1, 
            1, 
            1, 
            1,
            &DANDELION_RPC_OPS_CLIENT as *const RpcOps as *mut RpcOps
        ))
});

/// Context that the runtime exposes to functions
pub struct RuntimeContext<E: Engine> {
    pub engines: ObjectPool<NUM_CORES, E>,
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

// SAFETY: Runtime is shared via Arc and accessed through &self methods only.
// The raw pointers inside Lauberhorn are C FFI handles with a managed lifecycle
// (init on creation, join_workers on drop) and are not mutated concurrently.
unsafe impl<E: Engine> Send for Runtime<E> {}
unsafe impl<E: Engine> Sync for Runtime<E> {}

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
    /// Sets up engines, memory domains, function registry, and the lauberhorn
    /// RPC subsystem.
    /// TODO: which kind of errror to return here ? DandelionError ?
    pub fn init(
        memory_pool: BTreeMap<DomainType, MemoryResource>,
    ) -> DandelionResult<Self> {

        // TODO(@sven): implement properly based on #cores we want. Currently static allication of 2 engines
        // for testing
        let engines: Vec<E> = vec![*E::init(0).unwrap(), *E::init(1).unwrap()];

        let domains = get_available_domains(memory_pool);
        let registry = Arc::new(FunctionRegistry::new(&domains));
        let lauberhorn = Lauberhorn::init()?;

        // currently maximum number of pending nested calls

        // register unique handler invocation RPC
        // TODO(@sven): use user configured values via config.rs in server crate
        debug!("registering lauberhorn function handler under prog_num={}, prog_ver={}, proc_num={}, port={}", 1, 1, 1, 11111);

        let rt_ctx = Arc::new(RuntimeContext{
            engines: ObjectPool::new(engines),
            // nested_results: ObjectPool::new(Vec::with_capacity(64)),
            registry: registry.clone(),
            nested_ep: (*DANDELION_NESTED_EP).clone()
        });

        // TODO(@sven): use user configured values via config.rs in server crate
        lauberhorn.register_service( 
            Arc::into_raw(rt_ctx.clone()) as *mut c_void, // TODO(@Sven): prevent memory leaks ? 
            LauberhornHandler {
             func: dandelion_function_handler::<E> as LauberhornHandlerFunc,
             free: dandelion_function_free,   
            },
            1, 1, 1,
            11111, false,
            (*&DANDELION_RPC_SERVER_CODEC).clone()
        )?;
        Ok(Runtime { 
            ctx: rt_ctx, 
            lauberhorn: lauberhorn, 
            domains: domains 
        })
        
    }

    /// Register a service with lauberhorn.
    ///
    /// The function identified by `function_id` must already be registered
    /// via [`register_function`](Self::register_function).
    //pub fn register_service(
    //    &self,
    //    function_id: FunctionId,
    //    prog_num: u32,
    //    prog_ver: u32,
    //    proc_num: u32,
    //    listen_port: u16,
    //) -> Result<(), ()> {
//
    //    // get prog_num, prog_ver, proc_num, listen_port from config
    //    // this is the 4 tuple under which we register ALL services
    //    // and all it does is invoke the dispatch_function shim
//
    //    debug!("Registering service for function '{}' with prog_num {}, prog_ver {}, proc_num {}, listen_port {}",
    //        function_id, prog_num, prog_ver, proc_num, listen_port);
//
    //    // INFO AFTER REFACTOR: just store registration in a map to verify on calls if this funciton is actually registered
    //    // create context for this service
    //    // lauberhorn needs this to access runtime data structures when executing requests for this service
    //    let srv_ctx = Box::new(LauberhornServiceCtx {
    //        function_registry: self.registry.clone(),
    //        // currently pass all engines as vector -> index into this by core_id this thing runs on
    //        // probably need more reliable mapping core -> vector slot
    //        engines: self
    //            .engines
    //            .iter()
    //            .map(|e| &**e as *const E as *mut E)
    //            .collect(),
    //        id: 0,
    //    });
//
    //    self.lauberhorn
    //        .register_service(
    //            srv_ctx,
    //            LauberhornHandler {
    //                func: 
    //            },
    //            prog_num,
    //            prog_ver,
    //            proc_num,
    //            listen_port,
    //            false
    //        )
    //        .map(|_| ())
    //        .map_err(|_| ())
    //    // TODO: return DandelionResult
    //}

    /// Register a composition with the runtime's function registry.
    pub fn register_composition(
        &self,
        composition_desc: &str,
    ) -> DandelionResult<()> {
        self.ctx.registry.insert_compositions(composition_desc)
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

        // clean this up!
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
            self.ctx.engines.size(),
            self.domains.len()
        );

        for _ in 0..self.ctx.engines.size() {
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
    memory_pool: BTreeMap<DomainType, MemoryResource>,
) -> DandelionResult<Runtime<impl Engine>> {
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
