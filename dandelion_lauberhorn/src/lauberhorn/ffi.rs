use std::ffi::{c_void};
use log::{debug, error};
use crate::lauberhorn::{execution::execute_lauberhorn_function, types::DandelionRPCRequest};
use crate::lauberhorn::marshall;
use crate::lauberhorn::types::LauberhornServiceCtx;
use dandelion_server::DandelionBody;
use machine_interface::function_driver::thread_utils::Engine;
use crate::utils::xdr;
// ---------------------------------------------------------------------------
// Marshal / unmarshal callbacks (registered in LAUBERHORN_SCHEMA)
// ---------------------------------------------------------------------------

/// XDR stream struct matching glibc's `struct __rpc_xdr`.
/// Only used to extract the raw buffer pointer and remaining byte count
/// from an `xdrmem`-backed stream.
///
/// // temporary workaround until we remove C wrapper around XDR!
///


/// Unmarshal callback – matches `xdrproc_t` signature: `(XDR *, void *) -> bool_t`.
///
/// The C side calls this as `schema->call_func(&xdrs, out_msg)` where:
/// - `xdrs` is an XDR memory stream wrapping the raw payload bytes
/// - `out_msg` is the pre-allocated output buffer (Context)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unmarshal(
    xdrs: *mut xdr::XdrStream,
    out_msg: *mut c_void,
) -> i32 {
    if xdrs.is_null() {
        return 0;
    }
    let xdr_stream = unsafe { &mut *xdrs };
    // extract name

    let name = xdr_stream.get_string();
    let blob = xdr_stream.get_opaque();
    if name.is_none() || blob.is_none() {
        error!("Failed to unmarshal request: invalid XDR format");
        return 0;
    }
    let out_req = out_msg as *mut DandelionRPCRequest;
    marshall::dandelion_unmarshal(out_req, &name.unwrap(), &blob.unwrap()) as i32
}

// problem we have with just allocating 1 request on Lauberhorn side, is that
// we only have 1 buffer to work with, so size must fit for ALL possible requests!
// could alternatively have in buffer (name (max length) + ptr to blob) and then allocate the blob
// with dandelion managed memory ? 

#[unsafe(no_mangle)]
pub unsafe extern "C" fn marshal(
    xdrs: *mut xdr::XdrStream,   // *mut XdrStream
    in_msg: *mut c_void, // *mut DandelionBody
) -> i32 {

    // if in_msg is null, execution failed, return 0
    if in_msg.is_null() {
        return 0;
    }

    let xdr = unsafe { &mut *xdrs };


    let out_buf_ptr = xdr.get_current_position();
    let out_buf_size = xdr.get_remaining();
    let result = unsafe { &mut *(in_msg as *mut DandelionBody) };

    let serialized = marshall::dandelion_marshal(result, out_buf_ptr, out_buf_size);
    xdr.set_opaque(serialized.as_slice());
    
    1
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

pub type LauberhornUserCb = unsafe extern "C" fn(i32);

/// xdrproc_t: (XDR *, void *) -> bool_t
type XdrProc = unsafe extern "C" fn(*mut xdr::XdrStream, *mut c_void) -> i32;

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
    req: *mut c_void,  // *mut DandelionRPCRequest
    xid: i32,
) -> LauberhornMsg {
    let ctx = data as *mut LauberhornServiceCtx<E>;
    let rpc_req = req as *mut DandelionRPCRequest;

    // returns NULL on error, else pointer to result context
    execute_lauberhorn_function::<E>(ctx, rpc_req, xid) as LauberhornMsg
}
