use std::ffi::c_void;

use crate::lauberhorn::marshal::{dandelion_free, dandelion_marshal_call, dandelion_marshal_resp,
    dandelion_unmarshal_call, dandelion_unmarshal_resp};
use crate::lauberhorn::types::LauberhornServiceCtx;
use dandelion_commons::{DandelionError, DandelionResult};
use machine_interface::function_driver::thread_utils::Engine;

use crate::lauberhorn::ffi::{AwaitResult, LauberhornCtx, LauberhornRpcEndpoint, LauberhornUserCb, LauberhornWorker,
    RpcCodec, RpcOps, lauberhorn_await_all, lauberhorn_await_any, lauberhorn_call_async, lauberhorn_create_worker,
    lauberhorn_dereg_srv, lauberhorn_init, lauberhorn_join_worker, lauberhorn_reg_srv, LauberhornCompletion, AwaitSet, }; 

// TODO(@Sven): integrate these into the normal handler (execution) (1 function less)
// as rn, dandelion_fucntion_handler just nicely prepares the arguments
pub use crate::lauberhorn::ffi::{dandelion_function_handler, dandelion_function_free, LauberhornHandler};

/// Wrapper around the lauberhorn C library.
///
/// Manages the lifecycle of the lauberhorn RPC context and its worker threads.
/// 
pub struct Lauberhorn {
    workers: Vec<*mut LauberhornWorker>,
    ctx: LauberhornCtx,
}

unsafe impl Sync for RpcCodec {}
pub static RPC_CODEC: RpcCodec = RpcCodec {
    ops: &RpcOps {
    marshal_call: dandelion_marshal_call,
    marshal_resp: dandelion_marshal_resp, 
    unmarshal_call: dandelion_unmarshal_call, 
    unmarshal_resp: dandelion_unmarshal_resp,
    free: dandelion_free,
    },
    private: 0 as *const c_void
};


pub fn call_async(ep: &LauberhornRpcEndpoint, payload: &[u8]) -> i32 {
    unsafe {
        // from ffi
        lauberhorn_call_async(
            ep as *const LauberhornRpcEndpoint,
            payload.as_ptr() as *const c_void,
            payload.len()
        )
    }
}

pub fn await_all(set: &AwaitSet) -> Option<Vec<AwaitResult>> {
    
    let mut await_results = Vec::with_capacity(set.size() as usize);
    let result = unsafe { 
        lauberhorn_await_all(set, await_results.as_mut_ptr(), set.size() as usize) };

    if result {
        unsafe { await_results.set_len(set.size() as usize); }
        Some(await_results)
    } else {
        None 
    }
}

pub fn await_any(set: &mut AwaitSet) -> Option<LauberhornCompletion> {

    let mut await_result = LauberhornCompletion::new();

    let res = unsafe { lauberhorn_await_any(set, &mut await_result as *mut LauberhornCompletion) };

    if res {
        Some(await_result)
    } else {
        None
    }
}

impl Lauberhorn {

    /// Initialize the lauberhorn RPC subsystem.
    pub fn init() -> DandelionResult<Self> {
        let mut ctx = LauberhornCtx::new();
        match unsafe { lauberhorn_init(&mut ctx) } {
            0 => Ok(Lauberhorn { ctx, workers: Vec::new() }),
            // TODO(@Sven): create lauberhorn error separtely
            _ => Err(DandelionError::LauberhornError("lauberhorn_init".into())),
        }
    }

    /// Register a new RPC service backed by a dandelion function.
    /// 
    pub fn register_service<E: Engine>(
        &self,
        service_ctx: Box<LauberhornServiceCtx<E>>,
        handler: LauberhornHandler,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        is_nested: bool,
        rpc_codec: &RpcCodec
    ) -> DandelionResult<i32> {
        
        let id = unsafe {
            lauberhorn_reg_srv(
                &self.ctx,
                handler,
                Box::into_raw(service_ctx) as *mut c_void,
                prog_num,
                prog_ver,
                proc_num,
                listen_port,
                is_nested, 
                rpc_codec
            )
        };
        if id < 0 {
            // TODO(@Sven): implement own error category
            Err(DandelionError::LauberhornError("lauberhorn_reg_srv".into()))
        } else {
            Ok(id)
        }
    }

    /// Deregister a previously registered RPC service.
    pub fn deregister_service(
        &mut self,
        prog_num: u32,
    ) -> DandelionResult<i32> {
        let id = unsafe { lauberhorn_dereg_srv(&mut self.ctx, prog_num) };
        if id < 0 {
            Err(DandelionError::LauberhornError("lauberhorn_dereg_srv".into()))
        } else {
            Ok(id)
        }
    }

    /// Spawn a lauberhorn worker thread.
    pub fn create_worker(
        &mut self,
        init: Option<LauberhornUserCb>,
        fini: Option<LauberhornUserCb>,
    ) -> *mut LauberhornWorker {
        let worker = unsafe { lauberhorn_create_worker(&mut self.ctx, init, fini) };
        self.workers.push(worker);
        worker
    }

    /// Join all spawned worker threads, blocking until they finish.
    pub fn join_workers(&mut self) {
        for &worker in &self.workers {
            unsafe { lauberhorn_join_worker(&mut self.ctx, worker) };
        }
    }
}
