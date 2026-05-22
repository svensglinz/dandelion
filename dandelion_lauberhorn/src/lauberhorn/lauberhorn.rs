use std::{ffi::c_void, sync::Arc};

use dandelion_commons::{DandelionError, DandelionResult};
use machine_interface::function_driver::thread_utils::Engine;

use crate::{
    lauberhorn::ffi::{
            LauberhornCtx, LauberhornHandler, LauberhornUserCb, LauberhornWorker, RpcCodec, lauberhorn_create_worker, lauberhorn_dereg_srv, lauberhorn_init, lauberhorn_join_worker, lauberhorn_reg_srv
        },
    runtime::RuntimeContext,
};

/// Wrapper around the lauberhorn C library.
///
/// Manages the lifecycle of the lauberhorn RPC context and its worker threads.
///
pub struct Lauberhorn {
    workers: Vec<*mut LauberhornWorker>,
    ctx: LauberhornCtx,
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
    pub fn register_service(
        &self,
        private: *mut c_void,
        handler: LauberhornHandler,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        is_nested: bool,
        rpc_codec: Arc<RpcCodec>,
    ) -> DandelionResult<i32> {
        let id = unsafe {
            lauberhorn_reg_srv(
                &self.ctx,
                handler,
                private,  // TODO(@Sven): prevent memory leak somehow
                prog_num,
                prog_ver,
                proc_num,
                listen_port,
                is_nested,
                Arc::into_raw(rpc_codec),
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
        let worker =
            unsafe { lauberhorn_create_worker(&mut self.ctx, init, fini) };
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
