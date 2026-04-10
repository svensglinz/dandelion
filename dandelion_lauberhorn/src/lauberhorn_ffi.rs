use std::ffi::c_void;

use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::memory_domain::Context;

use crate::execution::execute_lauberhorn_function;
use crate::lauberhorn_types::LauberhornServiceCtx;
use crate::marshall;

// ---------------------------------------------------------------------------
// Marshal / unmarshal callbacks (registered in LAUBERHORN_SCHEMA)
// ---------------------------------------------------------------------------

/// Unmarshal an incoming RPC byte buffer into a dandelion `Context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unmarshal(
    out_ctx: *mut c_void,
    in_buf: *mut c_void,
    in_bytes: i32,
) -> i32 {
    let ctx = out_ctx as *mut Context;
    if marshall::dandelion_unmarshal(ctx, in_buf as *const u8, in_bytes) {
        1
    } else {
        0
    }
}

/// Marshal a dandelion `Context` into an outgoing RPC byte buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn marshal(
    out_buf: *mut c_void,
    in_ctx: *mut c_void,
    out_buf_size: i32,
) -> i32 {
    let ctx = in_ctx as *const Context;
    if marshall::dandelion_marshal(
        unsafe { &*ctx },
        out_buf as *mut u8,
        out_buf_size,
    ) {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// C FFI bindings to liblauberhorn
// ---------------------------------------------------------------------------

#[link(name = "lauberhorn")]
unsafe extern "C" {
    pub fn lauberhorn_reg_srv(
        ctx: *const LauberhornCtx,
        func: LauberhornHandlerFn,
        data: *mut c_void,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        schema: *const LauberhornSchema,
    ) -> i32;

    pub fn lauberhorn_init(ctx: *const LauberhornCtx) -> i32;

    pub fn lauberhorn_dereg_srv(
        ctx: *const LauberhornCtx,
        prog_num: u32,
    ) -> i32;

    pub fn lauberhorn_create_worker(
        ctx: *const LauberhornCtx,
        init: Option<LauberhornUserCb>,
        fini: Option<LauberhornUserCb>,
    ) -> *mut LauberhornWorker;

    pub fn lauberhorn_join_worker(
        ctx: *const LauberhornCtx,
        worker: *mut LauberhornWorker,
    );
}

// ---------------------------------------------------------------------------
// Type definitions matching the C header
// ---------------------------------------------------------------------------

pub type LauberhornUserCb = unsafe extern "C" fn(i32) -> c_void;
type LauberhornCallFunc =
    unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> i32;
type LauberhornRespFunc =
    unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> i32;

/// Schema telling lauberhorn how to serialize/deserialize requests.
#[repr(C)]
pub struct LauberhornSchema {
    pub call_size: usize,
    pub call_func: LauberhornCallFunc,
    pub resp_func: LauberhornRespFunc,
}

/// Opaque context handle returned by `lauberhorn_init`.
#[repr(C)]
pub struct LauberhornCtx {
    fd: i32,
}

impl LauberhornCtx {
    pub fn new() -> Self {
        LauberhornCtx { fd: 0 }
    }
}

/// Opaque handle to a lauberhorn worker thread.
pub type LauberhornWorker = *mut c_void;
/// Opaque handle to an RPC message.
pub type LauberhornMsg = *mut c_void;
/// Function pointer type for RPC service handlers.
pub type LauberhornHandlerFn = unsafe extern "C" fn(
    data: *mut c_void,
    req: LauberhornMsg,
    xid: i32,
) -> LauberhornMsg;

// ---------------------------------------------------------------------------
// Generic handler entry point (monomorphized per Engine type)
// ---------------------------------------------------------------------------

/// RPC handler callback — dispatches into the typed dandelion execution path.
pub unsafe extern "C" fn lauberhorn_function_handler<E: Engine>(
    data: *mut c_void,
    req: *mut c_void,
    xid: i32,
) -> LauberhornMsg {
    let ctx = data as *mut LauberhornServiceCtx<E>;
    let req_ctx = req as *mut Context;
    execute_lauberhorn_function::<E>(ctx, req_ctx, xid) as LauberhornMsg
}
