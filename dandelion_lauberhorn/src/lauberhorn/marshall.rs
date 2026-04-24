use bytes::{Buf, Bytes};
use dandelion_server::{DandelionBody, DandelionRequest};
use crate::{lauberhorn::types::{DandelionArgs, DandelionRPCRequest}, webserver::schemas::DandelionDeserializeResponse};
use log::{debug, error};
use machine_interface::memory_domain::bytes_context::BytesContext;
pub use machine_interface::{
    memory_domain::{Context, ContextType},
    DataItem, DataSet, Position,
};

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

// ---------------------------------------------------------------------------
// Request parsing
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// FFI-facing marshal / unmarshal
// ---------------------------------------------------------------------------

/// Unmarshal an incoming byte buffer into a dandelion `Context`.
/// incoming: XDR { string, blob }
pub fn dandelion_unmarshal(
    out_ctx: *mut DandelionRPCRequest,
    name: &String,
    blob: &Vec<u8>,
) -> bool {
    let req: DandelionArgs = match bson::from_slice(blob) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let rpc_req = DandelionRPCRequest {
        function_name: name.clone(),
        payload: req,
    };
    unsafe { std::ptr::write(out_ctx, rpc_req) };
    true
}

/// Marshal a `Context` into an outgoing byte buffer.
/// * `in_ctx`: DandelionBody to be serialized
/// * `out_buf`: 
/// * `out_bytes`: size of the output buffer in bytes
/// TODO: serialize the context back into BSON for the RPC response.
pub fn dandelion_marshal(
    in_ctx: &mut DandelionBody,
    out_buf: *mut u8,
    out_bytes: u32,
) -> Vec<u8> {
    debug!("dandelion_marshal: marshaling {:?}", in_ctx);

    // 1. Linearize the body into a temporary Rust Vec
    let lin_resp = linearize_dandelion_body(unsafe { &mut *in_ctx });
    let deserialized: DandelionDeserializeResponse = bson::from_slice(&lin_resp).unwrap();
    
    debug!("Marshalled response to {:?}", &lin_resp);
    // deserialize for printing
    debug!("Deserialized response: {:?}", deserialized);

    // 2. Copy the linearized bytes into the output buffer
   lin_resp
}
