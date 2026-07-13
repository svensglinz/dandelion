use std::ffi::{c_char, c_void};
use std::sync::Arc;
use crate::dispatcher::execution::dandelion_handler;
use crate::lauberhorn::marshal::{DandelionRPCRequest, DandelionRPCResponse};
use crate::runtime::RuntimeContext;
use machine_interface::function_driver::thread_utils::Engine;

// C FFI bindings to liblauberhorn

#[link(name = "lauberhorn")]
unsafe extern "C" {

    pub fn lauberhorn_init(ctx: *const LauberhornCtx) -> i32;
    pub fn lauberhorn_fini(ctx: *const LauberhornCtx);

    pub fn lauberhorn_reg_srv(
        ctx: *const LauberhornCtx,
        handler: LauberhornHandler,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        rpc_codec: *const RpcCodec,
    ) -> i32;

    pub fn lauberhorn_dereg_srv(
        ctx: *const LauberhornCtx,
        srv_id: i32,
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

    pub fn lauberhorn_call_async(
        ep: *const LauberhornRpcEndpointRaw,
        payload: *const c_void,
        payload_len: usize,
    ) -> i32;

    pub fn lauberhorn_call_cb(
        ep: *const LauberhornRpcEndpointRaw,
        payload: *const c_void,
        payload_len: usize,
        cb: LauberhornCbHandler,
    ) -> i32;

    pub fn lauberhorn_await_any(
        set: *mut AwaitSetRaw,
        result: *mut RpcResult,
    ) -> bool;

    pub fn lauberhorn_free_result(result: *mut RpcResult);
}

// ---------------------------------------------------------------------------
// Type definitions 
// ---------------------------------------------------------------------------

pub type LauberhornUserCb = unsafe extern "C" fn(i32);

// lauberhorn_handler_func_t, lauberhorn_handler_free_t
pub type LauberhornHandlerFunc = extern "C" fn(private: *mut c_void, msg: *mut c_void, xid: u32) -> *mut c_void;
pub type LauberhornHandlerFree = extern "C" fn(msg: *mut c_void);

pub type LauberhornCb = extern "C" fn(reply: *mut RpcResult, data: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy)]
pub enum LauberhornExecCtx {
    ExecFiber = 0,
    ExecInline = 1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LauberhornHandler {
    pub data: *mut c_void,
    pub func: LauberhornHandlerFunc,
    pub free: LauberhornHandlerFree,
    pub mode: LauberhornExecCtx,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RpcStatus {
    RpcOk = 0,
    RpcTimeout = 1,
    RpcError = 2,
}

#[repr(C)]
pub struct RpcResult {
    pub data: *mut c_void,
    pub status: RpcStatus,
    pub id: i32,
    pub codec: *mut RpcCodec,
}

impl RpcResult {
    pub fn empty() -> Self {
        RpcResult { data: std::ptr::null_mut(), status: RpcStatus::RpcOk, id: -1, codec: std::ptr::null_mut() }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LauberhornCbHandler {
    pub cb: LauberhornCb,
    pub data: *mut c_void,
    pub mode: LauberhornExecCtx,
}

pub type LauberhornWorker = *mut c_void;
pub type LauberhornMsg = *mut c_void;


// Codec (rpc.h / xdr_rpc.h)

pub(crate) type MarshalProc =  extern "C" fn (in_buf: *const c_void, out_buf: *mut c_void, in_buf_len: usize, private: *const c_void) -> i32;
pub(crate) type UnmarshalProc = extern "C" fn(in_buf: *const c_void, out_buf: *mut c_void, in_buf_len: usize, private: *const c_void) -> i32;
pub(crate) type FreeProc = extern "C" fn(*mut c_void, kind: RpcFreeKind, private: *mut c_void);
pub(crate) type AllocProc = extern "C" fn(kind: RpcAllocKind, private: *mut c_void) -> *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub enum RpcFreeKind {
    RpcFreeCall = 0,
    RpcFreeResp = 1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum RpcAllocKind {
    RpcAllocCall = 0, 
    RpcAllocResp = 1
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RpcOps {
    pub(crate) marshal_call: MarshalProc,
    pub(crate) marshal_resp: MarshalProc,
    pub(crate) unmarshal_call: UnmarshalProc,
    pub(crate) unmarshal_resp: UnmarshalProc,
    pub(crate) free: FreeProc,
    pub(crate) alloc: AllocProc,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct RpcCodec {
    pub(crate) ops: *const RpcOps,
    pub(crate) private: *const c_void,
}

// move somewhere else and explain safety
unsafe impl Sync for RpcCodec {}
unsafe impl Sync for RpcOps {}
unsafe impl Send for RpcCodec {}
unsafe impl Send for RpcOps {}


// await_set_t
#[repr(C)]
pub struct AwaitSetRaw {
    pub(crate) rpc_mask: usize,
}


// lauberhorn_rpc_endpoint_t
#[repr(C)]
pub struct LauberhornRpcEndpointRaw {
    pub(crate) daddr: *const c_char,
    pub(crate) dport: u16,
    pub(crate) prog_num: u32,
    pub(crate) prog_ver: u32,
    pub(crate) proc_num: u32,
    pub(crate) codec: *mut RpcCodec,
}


/// Opaque context handle returned by `lauberhorn_init`.
#[repr(C)]
pub struct LauberhornCtx {
    fd: i32,
    parity_page: *mut u8,
}

impl LauberhornCtx {
    pub fn new() -> Self {
        LauberhornCtx { fd: 0, parity_page: std::ptr::null_mut() }
    }
}


/// RPC handler callback — dispatches into the typed dandelion execution path.
/// 
pub extern "C" fn dandelion_function_handler<E: Engine>(
    data: *mut c_void, // *mut LauberhornServiceCtx<E>
    req: *mut c_void,  // *mut DandelionRPCRequest
    xid: u32,
) -> *mut c_void {
    
    // get reference to the runtiem context
    let ctx = unsafe {
        Arc::increment_strong_count(data as *const RuntimeContext<E>);
        Arc::from_raw(data as *const RuntimeContext<E>)    
    };

    let rpc_req = unsafe { &mut *(req as *mut DandelionRPCRequest) };

    // execute request
    let sets = dandelion_handler::<E>(ctx, rpc_req).unwrap();

    // create response
    let response = DandelionRPCResponse {
        sets: sets
    };  

    // return raw pointer to (unserialized response)
    Box::into_raw(Box::new(response)) as *mut c_void 
}

/// called to free result
pub extern "C" fn dandelion_function_free(ptr: *mut c_void) {
    unsafe {
        let _ = Box::from_raw(ptr);
    }
}