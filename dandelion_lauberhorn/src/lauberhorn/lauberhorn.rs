use std::ffi::c_void;

use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::memory_domain::Context;

use crate::lauberhorn::ffi::*;
use crate::lauberhorn::types::LauberhornServiceCtx;
use dandelion_commons::{DandelionResult, DandelionError};

/// Wrapper around the lauberhorn C library.
///
/// Manages the lifecycle of the lauberhorn RPC context and its worker threads.
pub struct Lauberhorn {
    workers: Vec<*mut LauberhornWorker>,
    ctx: LauberhornCtx,
}

/// Static schema that tells lauberhorn how to marshal/unmarshal requests.
pub const LAUBERHORN_SCHEMA: LauberhornSchema = LauberhornSchema {
    call_func: marshal,
    resp_func: unmarshal,
    call_size: std::mem::size_of::<Context>(),
};

impl Lauberhorn {
    /// Initialize the lauberhorn RPC subsystem.
    pub fn init() -> DandelionResult<Self> {
        let ctx = LauberhornCtx::new();
        match unsafe { lauberhorn_init(&ctx) } {
            0 => Ok(Lauberhorn {
                ctx, 
                workers: Vec::new()
            }),
            // just some generic error code for now --> specialize and refine
            _ => Err(DandelionError::LauberhornError("lauberhorn_init".into())),
        }
    }

    /// Register a new RPC service backed by a dandelion function.
    pub fn register_service<E: Engine>(
        &self,
        service_ctx: Box<LauberhornServiceCtx<E>>,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
    ) -> Result<i32, i32> {
        let id = unsafe {
            lauberhorn_reg_srv(
                &self.ctx,
                lauberhorn_function_handler::<E>,
                Box::into_raw(service_ctx) as *mut c_void,
                prog_num,
                prog_ver,
                proc_num,
                listen_port,
                &LAUBERHORN_SCHEMA,
            )
        };
        if id < 0 { Err(id) } else { Ok(id) }
    }

    /// Deregister a previously registered RPC service.
    pub fn deregister_service(&mut self, prog_num: u32) -> Result<i32, i32> {
        let id = unsafe { lauberhorn_dereg_srv(&mut self.ctx, prog_num) };
        if id < 0 { Err(id) } else { Ok(id) }
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
