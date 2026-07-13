use std::{ffi::c_void, sync::Arc};

use dandelion_commons::{DandelionError, DandelionResult};

use crate::lauberhorn::ffi::{
            LauberhornCtx, LauberhornHandler, LauberhornUserCb, LauberhornWorker, RpcCodec,
            lauberhorn_create_worker, lauberhorn_dereg_srv, lauberhorn_fini, lauberhorn_init,
            lauberhorn_join_worker, lauberhorn_reg_srv
};

/// Wrapper around the lauberhorn C library.
///
/// Manages the lifecycle of the lauberhorn context and its worker threads.
///
pub struct Lauberhorn {
    workers: Vec<*mut LauberhornWorker>,
    ctx: LauberhornCtx,

    // do we need codecs here ? 
}  

impl Lauberhorn {
    /// Initialize lauberhorn
    pub fn init() -> DandelionResult<Self> {

        let mut ctx = LauberhornCtx::new();

        match unsafe { lauberhorn_init(&mut ctx) } {
            0 => Ok(Lauberhorn { ctx, workers: Vec::new() }),
            // TODO(@Sven): create lauberhorn error separtely
            _ => Err(DandelionError::LauberhornError("lauberhorn_init".into())),
        }
    }

    /// Register a new RPC service 
    pub fn register_service(
        &self,
        handler: LauberhornHandler,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        rpc_codec: Arc<RpcCodec>,
    ) -> DandelionResult<i32> {

        let codec_ptr: *const RpcCodec = Arc::as_ptr(&rpc_codec);
        // self.codecs.push(rpc_codec); (why are we doing this ?) 

        let id = unsafe {
            lauberhorn_reg_srv(
                &self.ctx,
                handler,
                prog_num,
                prog_ver,
                proc_num,
                listen_port,
                codec_ptr, // this is now refcounted !
            )
        };

        if id < 0 {
            // self.codecs.pop()
            // TODO(@Sven): implement own error category
            Err(DandelionError::LauberhornError("lauberhorn_reg_srv".into()))
        } else {
            Ok(id)
        }
    }

    /// Deregister a previously registered RPC service.
    pub fn deregister_service(
        &mut self,
        srv_id: i32,
    ) -> DandelionResult<i32> {
        let id = unsafe { lauberhorn_dereg_srv(&mut self.ctx, srv_id) };
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
    ) -> DandelionResult<*mut LauberhornWorker> {
        let worker =
            unsafe { lauberhorn_create_worker(&mut self.ctx, init, fini) };

        if worker.is_null() {
            return Err(DandelionError::LauberhornError("lauberhorn_create_worker".into()));
        }
        self.workers.push(worker);
        Ok(worker)
    }

    /// Join all spawned worker threads, blocking until they finish.
    pub fn join_workers(&mut self) {
        for &worker in &self.workers {
            unsafe { lauberhorn_join_worker(&mut self.ctx, worker) };
        }
    }
}

impl Drop for Lauberhorn {
    fn drop(&mut self) {
        self.join_workers();
        unsafe { lauberhorn_fini(&mut self.ctx) };
    }
}