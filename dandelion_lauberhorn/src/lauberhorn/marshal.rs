use std::ffi::c_void;
use bytes::{Buf};
use dandelion_server::{DandelionBody};
use crate::lauberhorn::ffi::*;
use crate::{lauberhorn::types::{DandelionArgs, DandelionRPCRequest}, webserver::schemas::DandelionDeserializeResponse};
use log::{debug};
use crate::utils::xdr::{xdr_get_opaque, xdr_write_opaque};

// TODO(@Sven): move this somewhere else
pub fn linearize_dandelion_body(body: &mut DandelionBody) -> Vec<u8> {
    if let Some(mut buf) = body.buffer.take() {
        let total_size = buf.remaining();
        let mut linear_vec = Vec::with_capacity(total_size);

        while buf.has_remaining() {
            let chunk = buf.chunk();
            linear_vec.extend_from_slice(chunk);
            buf.advance(chunk.len());
        }
        linear_vec
    } else {
        Vec::new()
    }
}

/// [ be32 (len_str) -- char[len_str] (function_name) -- be32 (len_blob) -- char[len_blob] (data) --- ]
/// 
/// function_name: name of the function to be called
/// data: bson encoded DandelionInput
/// 
/// returns -1 on failure, 0 on success 
pub extern "C" fn dandelion_unmarshal_call(
     in_buf: *const c_void,  out_buf: *mut *mut c_void,
     in_buf_len: usize, private: *const c_void
) -> i32 {    

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null());

    let data = unsafe { std::slice::from_raw_parts(in_buf as *const u8, in_buf_len) };

    // extract function name
    let (string, rest) = match xdr_get_opaque(data) {
        Some(v) => v,
        None => {
            log::warn!("dandelion_unmarshal_call: unmarshalling failed");
            return -1;
        }
    };

    // extract args BSON
    let (args, _) = match xdr_get_opaque(rest) {
        Some(v) => v,
        None => {
            log::warn!("dandelion_unmarshal_call: unmarshalling failed");
            return -1;
        }
    };

    let payload: DandelionArgs = match bson::from_slice(args) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("dandelion_unmarshal_call: unmarshalling failed");
            return -1;
        }
    };

    let function_name = match String::from_utf8(string.to_vec()) {
        Ok(name) => name,
        Err(e) => {
            log::warn!("dandelion_unmarshal_call: unmarshalling failed");
            return -1;
        }
    };

    let unmarshaled = Box::new(
        DandelionRPCRequest {
            function_name,
            payload
    });
    
    // memory must be freed by call to dandelion_free() 
    // which will be called by the runtime after the call returns
    // a reply
    unsafe { *out_buf = Box::into_raw(unmarshaled) as *mut c_void; };
    return 0;
}

// marshal DandelionRPCRequest into wire format
// Returns the number of bytes written to out_buf, or -1 on error
pub extern "C" fn dandelion_marshal_call(
    in_buf: *const c_void, out_buf: *mut c_void, 
    in_buf_len: usize, private: *const c_void
) -> i32 {

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null());

    let data = unsafe {&*(in_buf as *const DandelionRPCRequest) };
    let args_bson = match bson::to_vec(&data.payload) {
        Ok(bson) => bson,
        Err(e) => {
            log::warn!("dandelion_marshal_call: marshalling failed");
            return -1;
        }
    };

    let out_bytes = out_buf as *mut u8;

    unsafe {
        // write function name 
        let len_func = match xdr_write_opaque(data.function_name.as_bytes(), out_bytes, 1500) {
            Ok(n) => n,
            Err(()) => {
            log::warn!("dandelion_marshal_call: marshalling failed");
                return -1;
            }
        };
        
        // write args
        let remaining = 1500 - len_func;
        let len_args = match xdr_write_opaque(args_bson.as_slice(), out_bytes.add(len_func), remaining) {
            Ok(n) => n,
            Err(()) => {
                log::warn!("dandelion_marshal_call: marshalling failed");
                return -1;
            }
        };
        
        let total = len_func + len_args;
        total as i32
    }
}

/// TODO(@Sven): we dont actually serialize to bson here ? what does dandelion do here in linearize_body ???
/// In: DandelionBody, Out: [ be32 (len) -- char[len]  (serialized data) ]
/// Incoming: Vec<Option<CompositionSet>>
/// Returns: number of bytes written, or -1 on error
pub extern "C" fn dandelion_marshal_resp(
    in_buf: *const c_void,  out_buf: *mut c_void,
    _in_buf_len: usize, _private: *const c_void
)  -> i32 {

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null());

    let result = unsafe { &mut *(in_buf as *mut DandelionBody) };
    let bytes = linearize_dandelion_body(result);

    
    // TODO(@Sven): fix 1500
    match xdr_write_opaque(bytes.as_slice(),  out_buf as *mut u8, 1500) {
        Ok(len) => {
            return len as i32;
        },
        Err(()) => {
            log::warn!("dandelion_marshal_call: marshalling failed");
            return -1;
        }
    }
}

/// 
pub extern "C" fn dandelion_unmarshal_resp(
    in_buf: *const c_void, out_buf: *mut *mut c_void,
    in_buf_len: usize, private: *const c_void
) -> i32 {

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null());     

    // this should be the same form as DandelionArgs ... ? (then we can jsut turn it back like this 
    // IDEA(@Sven): could just store the result in a slot and somehow return an index ? 
    let data = unsafe { std::slice::from_raw_parts(in_buf as *const u8, in_buf_len) };
    let (resp, _) = xdr_get_opaque(data).unwrap(); 
// 
    let resp_deser: DandelionDeserializeResponse = bson::from_slice(&resp).unwrap();
    let unmarshaled = Box::new(resp_deser);
    unsafe {*out_buf = Box::into_raw(unmarshaled) as *mut c_void; };
    return 0; 
}


/// frees the released pointer from dandelion_unmarshal_resp or dandelion_unmarshal_call
pub extern "C" fn dandelion_free(ptr: *mut c_void, kind: RpcFreeKind, private: *mut c_void) {
    match kind {
        _ => {
            unsafe { let _ = Box::from_raw(ptr); }
        },
    }
}