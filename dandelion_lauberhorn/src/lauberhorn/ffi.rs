use std::ffi::c_void;

use crate::lauberhorn::execution::execute_lauberhorn_function;
use crate::lauberhorn::marshall;
use crate::lauberhorn::types::LauberhornServiceCtx;
use dandelion_server::DandelionBody;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::memory_domain::Context;

// ---------------------------------------------------------------------------
// Marshal / unmarshal callbacks (registered in LAUBERHORN_SCHEMA)
// ---------------------------------------------------------------------------

/// XDR stream struct matching glibc's `struct __rpc_xdr`.
/// Only used to extract the raw buffer pointer and remaining byte count
/// from an `xdrmem`-backed stream.
///
/// // temporary workaround until we remove C wrapper around XDR!
///
#[repr(C)]
struct XdrStream {
    x_op: i32,            // enum xdr_op
    x_ops: *const c_void, // xdr_ops vtable pointer
    x_public: *mut u8,    // users' data
    x_private: *mut u8,   // current position in buffer
    x_base: *mut u8,      // start of buffer
    x_handy: u32,         // remaining bytes
}

/// Unmarshal callback – matches `xdrproc_t` signature: `(XDR *, void *) -> bool_t`.
///
/// The C side calls this as `schema->call_func(&xdrs, out_msg)` where:
/// - `xdrs` is an XDR memory stream wrapping the raw payload bytes
/// - `out_msg` is the pre-allocated output buffer (Context)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unmarshal(xdrs: *mut c_void, out_msg: *mut c_void) -> i32 {
    let xdr = xdrs as *const XdrStream;
    let in_buf = unsafe { (*xdr).x_private };
    let in_bytes = unsafe { (*xdr).x_handy } as i32;

    let ctx = out_msg as *mut Context;
    marshall::dandelion_unmarshal(ctx, in_buf as *const u8, in_bytes) as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn marshal(
    xdrs: *mut c_void,   // *mut XdrStream
    in_msg: *mut c_void, // *mut DandelionBody
) -> i32 {
    let xdr = xdrs as *mut XdrStream;
    let out_buf_ptr = unsafe { (*xdr).x_private };
    let out_buf_size = unsafe { (*xdr).x_handy } as i32;
    let result = in_msg as *mut DandelionBody;

    marshall::dandelion_marshal(result, out_buf_ptr, out_buf_size) as i32
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

    pub fn lauberhorn_dereg_srv(ctx: *const LauberhornCtx, prog_num: u32) -> i32;

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

pub type LauberhornUserCb = unsafe extern "C" fn(i32);

/// xdrproc_t: (XDR *, void *) -> bool_t
type XdrProc = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;

/// Schema telling lauberhorn how to serialize/deserialize requests.
#[repr(C)]
pub struct LauberhornSchema {
    pub call_func: XdrProc,
    pub resp_func: XdrProc,
    pub call_size: usize,
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

/// RPC handler callback — dispatches into the typed dandelion execution path.
pub unsafe extern "C" fn lauberhorn_function_handler<E: Engine>(
    data: *mut c_void, // *mut LauberhornServiceCtx<E>
    req: *mut c_void,  // *mut Context
    xid: i32,
) -> LauberhornMsg {
    let ctx = data as *mut LauberhornServiceCtx<E>;
    let req_ctx = req as *mut Context;

    // returns NULL on error, else pointer to result context
    execute_lauberhorn_function::<E>(ctx, req_ctx, xid) as LauberhornMsg
}
