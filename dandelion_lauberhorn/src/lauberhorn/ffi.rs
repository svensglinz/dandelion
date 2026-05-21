use std::ffi::{CString, c_char, c_int, c_void};
use std::str::FromStr;
use crate::lauberhorn::execution::dandelion_handler;
use crate::lauberhorn::{types::DandelionRPCRequest};
use crate::lauberhorn::types::LauberhornServiceCtx;
use machine_interface::function_driver::thread_utils::Engine;

// ---------------------------------------------------------------------------
// C FFI bindings to liblauberhorn
// ---------------------------------------------------------------------------

#[link(name = "lauberhorn")]
unsafe extern "C" {
    pub fn lauberhorn_reg_srv(
        ctx: *const LauberhornCtx,
        handler: LauberhornHandler,
        data: *mut c_void,
        prog_num: u32,
        prog_ver: u32,
        proc_num: u32,
        listen_port: u16,
        is_nested: bool,
        rpc_codec: *const RpcCodec,
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

    pub fn lauberhorn_await_any(
        set:  *const AwaitSet, result: *mut LauberhornCompletion
    ) -> bool;

    pub fn lauberhorn_await_all(set: *const AwaitSet, results: *mut AwaitResult, len: usize) -> bool;

    pub fn lauberhorn_call_async(
        ep: *const LauberhornRpcEndpoint, payload: *const c_void,
        payload_len: usize
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Type definitions 
// ---------------------------------------------------------------------------

pub type LauberhornUserCb = unsafe extern "C" fn(i32);

// lauberhorn_handler_func_t, lauberhorn_handler_free_t
type LauberhornHandlerFunc = extern "C" fn(private: *mut c_void, msg: *mut c_void, xid: u32) -> *mut c_void;
type LauberhornHandlerFree = extern "C" fn(msg: *mut c_void);

// lauberhorn_handler_t
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LauberhornHandler {
    pub func: LauberhornHandlerFunc,
    pub free: LauberhornHandlerFree
}

/// Opaque handle to a lauberhorn worker thread.
pub type LauberhornWorker = *mut c_void;
/// Opaque handle to an RPC message.
pub type LauberhornMsg = *mut c_void;

type MarshalProc =  extern "C" fn (in_buf: *const c_void, out_buf: *mut c_void, in_buf_len: usize, private: *const c_void) -> i32;
type UnmarshalProc = extern "C" fn(in_buf: *const c_void, out_buf: *mut *mut c_void, in_buf_len: usize, private: *const c_void) -> i32;
type FreeProc = extern "C" fn(*mut c_void, kind: RpcFreeKind, private: *mut c_void);

// rpc_free_kind_t
#[repr(C)]
pub enum RpcFreeKind {
    RpcFreeCall = 0,
    RpcFreeResp = 1
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RpcOps {
    pub marshal_call: MarshalProc,
    pub marshal_resp: MarshalProc,
    pub unmarshal_call: UnmarshalProc,
    pub unmarshal_resp: UnmarshalProc,
    pub free: FreeProc,
}

// rpc_codec_t
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RpcCodec {
    pub ops: *const RpcOps, 
    pub private: *const c_void
}

// await_set_t
#[repr(C)]
pub struct AwaitSet {
    pending: usize
}

// lauberhorn_rpc_endpoint_t
#[repr(C)]
pub struct LauberhornRpcEndpoint {
    daddr: *const c_char,
    dport: u16,
    prog_num: u32,
    prog_ver: u32,
    proc_num: u32,
    codec: RpcCodec
}

use crate::lauberhorn::lauberhorn::RPC_CODEC;

impl LauberhornRpcEndpoint {
    pub fn new(
        daddr: &str, dport: u16,  prog_num: u32,
        prog_ver: u32, proc_num: u32
    ) -> Self {

        let c_str = CString::from_str(daddr).unwrap();
        let daddr_ptr = c_str.into_raw();

        LauberhornRpcEndpoint {
            daddr: daddr_ptr,
            dport: dport, 
            prog_num: prog_num, 
            prog_ver: prog_ver,
            proc_num: proc_num, 
            codec: RPC_CODEC
        }
    }
}

// to free the Cstring
impl Drop for LauberhornRpcEndpoint {
    fn drop(&mut self) {
        if !self.daddr.is_null() {
            unsafe {
                let _ = CString::from_raw(self.daddr as *mut c_char);
            }
        }
    }
}

impl AwaitSet {

    pub fn new() -> Self {
        AwaitSet { pending: 0 }
    }

    pub fn  add(&mut self, indices: &[i64]) {
        for &idx in indices {
            self.pending |= 1 << idx;
        }
    }

    pub fn del(&mut self, indices: &[i64]) {
        for &idx in indices {
            self.pending &= !(1 << idx);
        }
    }

    pub fn size(&self) -> u32 {
        self.pending.count_ones()
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
    }
}

#[repr(C)]
pub struct AwaitResult {
    idx: c_int,
    data: *mut c_void
}

impl AwaitResult {
    pub fn new() -> Self {
        AwaitResult {
            idx: 0,
            data: std::ptr::null_mut::<c_void>()
        }
    }
}

/// Opaque context handle returned by `lauberhorn_init`.
#[repr(C)]
pub struct LauberhornCtx {
    fd: i32,
    parity_page: *mut u8, // will probably go at some point...
}

impl LauberhornCtx {
    pub fn new() -> Self {
        LauberhornCtx { fd: 0, parity_page: std::ptr::null_mut() }
    }
}

#[repr(C)]
pub struct LauberhornCompletion {
    pub idx: i32,
    pub data: *mut c_void
}

impl LauberhornCompletion {
    pub fn new() -> Self {
        LauberhornCompletion {
            idx: 0,
            data: 0 as *mut c_void
        }
    }
}

/// RPC handler invoked for nested function calls
/// does the same as the regular handler, 
/// but stores the result in an internal table, and returns
/// the index to the caller where the result is stored
pub extern "C" fn dandelion_nested_function_handler<E: Engine>(
    data: *mut c_void,
    req: *mut c_void, 
    xid: u32
) -> *mut c_void {

    return 0 as *mut c_void; 
}


/// RPC handler callback — dispatches into the typed dandelion execution path.
pub extern "C" fn dandelion_function_handler<E: Engine>(
    data: *mut c_void, // *mut LauberhornServiceCtx<E>
    req: *mut c_void,  // *mut DandelionRPCRequest
    xid: u32,
) -> *mut c_void {
    let ctx = unsafe {&mut  *(data as *mut LauberhornServiceCtx<E>) };
    let rpc_req = unsafe { &mut *(req as *mut DandelionRPCRequest) };

    // returns NULL on error, else pointer to result context
    dandelion_handler::<E>(ctx, rpc_req) as LauberhornMsg
}

// called to free result
pub extern "C" fn dandelion_function_free(ptr: *mut c_void) {
    unsafe {
        let _ = Box::from_raw(ptr);
    }
}