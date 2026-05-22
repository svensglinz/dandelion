use std::ffi::c_void;
use bytes::{Buf};
use dandelion_server::{DandelionBody};
use crate::{lauberhorn::ffi::*, utils::xdr::{xdr_get_opaque, xdr_write_opaque}, webserver::schemas::{DandelionDeserializeResponse, InputSet}};

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

pub trait RpcEncode {
    fn rpc_encode(&self, out_buf: &mut [u8]) -> Result<usize, ()>;
}

pub trait RpcDecode: Sized {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()>; 
}

/// generic decoder that calls T::rpc_decode and writes 
/// the pointer to the heap allocated result into *out_buf
pub extern "C" fn dandelion_unmarshal<T: RpcDecode> (
     in_buf: *const c_void,  out_buf: *mut *mut c_void,
     in_buf_len: usize, private: *const c_void
) -> i32 {    

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null()); 

    let data = unsafe { std::slice::from_raw_parts(in_buf as *const u8, in_buf_len) };

    let value = match T::rpc_decode(data) {
        Ok(v) => v,
        Err(_) => return -1
    }; 
    unsafe { *out_buf = Box::into_raw(Box::new(value)) as *mut c_void; }
    return 0;
}

/// generic encoder that calls T::rpc_encode and write the result 
/// into out_buf
pub extern "C" fn dandelion_marshal<T: RpcEncode> (
    in_buf: *const c_void, out_buf: *mut c_void, 
    in_buf_len: usize, private: *const c_void
) -> i32 {

    // sanity check
    //debug_assert!(in_buf.is_null());
    //debug_assert!(out_buf.is_null());

    let data = unsafe {&*(in_buf as *const T) };
    let out_buf = unsafe  { std::slice::from_raw_parts_mut(out_buf as *mut u8, 1500) };

    match data.rpc_encode(out_buf) {
        Ok(len) => len as i32,
        Err(()) => -1
    }
}

/// frees the released pointer from dandelion_unmarshal
pub extern "C" fn dandelion_free(ptr: *mut c_void, kind: RpcFreeKind, private: *mut c_void) {
    match kind {
        _ => {
            unsafe { let _ = Box::from_raw(ptr); }
        },
    }
}

/// ---- Implementation of concrete Serialization / Deserialzation Types ----

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DandelionArgs {
    pub sets: Vec<InputSet>
}

// User sends XDR encoded request with {function_name, blob}
// where blob is the bjson-encoded dandelion request context
pub struct DandelionRPCRequest {
    pub function_name: String,
    pub payload: DandelionArgs
}

pub struct DandelionRPCResponse {
    // bson encoded output thingy ? 
    pub data: Vec<u8>,
}

impl From<DandelionRPCResponse> for DandelionDeserializeResponse {
    fn from(value: DandelionRPCResponse) -> Self {
        let resp: DandelionDeserializeResponse = bson::from_slice(value.data.as_slice()).unwrap();
        resp
    }
}

impl RpcEncode for DandelionRPCResponse {
    fn rpc_encode(&mut self, out_buf: &mut [u8]) -> Result<usize, ()> {
        // encoding destroys the buffer.... okay for the time being
        xdr_write_opaque(self.data.as_slice(), out_buf.as_ptr() as *mut u8, 1500)
    }
}

impl RpcDecode for DandelionRPCResponse {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {
        todo!()
    }
}

impl RpcDecode for DandelionRPCRequest {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {

        let (string, rest) = xdr_get_opaque(in_buf).ok_or(())?;
        let (args, _) = xdr_get_opaque(rest).ok_or(())?;

        let payload: DandelionArgs = bson::from_slice(args).map_err(|_| ())?;
        let function_name = String::from_utf8(string.to_vec()).map_err(|_| ())?;

        Ok(DandelionRPCRequest {
            function_name,
            payload,
        })
    }
}

impl RpcEncode for DandelionRPCRequest {
    fn rpc_encode(&self, out_buf: &mut [u8]) -> Result<usize, ()> {
        let args_bson = bson::to_vec(&self.payload).map_err(|_| ())?;
        let func_name = self.function_name.as_bytes();

        let total_size = args_bson.len() + func_name.len();

        if out_buf.len() < total_size {
            return Err(());
        }

        // write name into buffer
        out_buf[0..func_name.len()].copy_from_slice(func_name);
        out_buf[func_name.len()..total_size].copy_from_slice(args_bson.as_slice());

        Ok(total_size)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DandelionNestedResponse {
    pub idx: usize
}

impl RpcDecode for DandelionNestedResponse {
    fn rpc_decode(in_buf: &[u8]) -> Result<Self, ()> {
        let value: DandelionNestedResponse = match bson::from_slice(in_buf) {
            Ok(v) => v,
            Err(_) => return Err(())
        };
        Ok(value)
    }
}

impl RpcEncode for DandelionNestedResponse {
    fn rpc_encode(&self, out_buf: &mut [u8]) -> Result<usize, ()> {
        let bytes = bson::to_vec(self).map_err(|_| ())?; 
        xdr_write_opaque(bytes.as_slice(), out_buf.as_ptr() as *mut u8, 1500)
    }
}
