use bytes::{Buf, Bytes};
use dandelion_server::{DandelionBody, DandelionRequest};
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

/// Parse a BSON-serialized `DandelionRequest` into a `Context`.
///
/// Lauberhorn RPC delivers each request as a single contiguous buffer,
/// so this is a simplified single-frame variant of `BytesContext::from_bytes_vec`.
fn parse_req_ctx(input: &[u8]) -> Result<(String, Context), ()> {
    let req: DandelionRequest = bson::from_slice(input).map_err(|_| ())?;
    let function_name = req.name.clone();

    let mut flat_data = Vec::new();
    let mut content = Vec::new();

    for set in req.sets {
        let mut buffers = Vec::new();
        for item in set.items {
            let offset = flat_data.len();
            let size = item.data.len();
            flat_data.extend_from_slice(item.data);
            buffers.push(DataItem {
                ident: item.identifier,
                key: item.key,
                data: Position { offset, size },
            });
        }
        content.push(Some(DataSet { ident: set.identifier, buffers }));
    }

    let bytes = Bytes::from(flat_data);
    let bytes_ctx = BytesContext::new(vec![bytes.clone()]);
    let mut context = Context::new(ContextType::Bytes(Box::new(bytes_ctx)), bytes.len());
    context.content = content;
    Ok((function_name, context))
}

// ---------------------------------------------------------------------------
// FFI-facing marshal / unmarshal
// ---------------------------------------------------------------------------

/// Unmarshal an incoming byte buffer into a dandelion `Context`.
pub fn dandelion_unmarshal(
    out_ctx: *mut Context,
    in_buf: *const u8,
    in_bytes: i32,
) -> bool {
    let input = unsafe { std::slice::from_raw_parts(in_buf, in_bytes as usize) };
    match parse_req_ctx(input) {
        Ok((_function_name, context)) => {
            // Use ptr::write to avoid dropping the uninitialized memory
            // that the C allocator placed at out_ctx.
            unsafe { std::ptr::write(out_ctx, context) };
            debug!("dandelion_unmarshal: parsed request successfully to {:?}", unsafe {
                &*out_ctx
            });
            true
        }
        Err(_) => false,
    }
}

/// Marshal a `Context` into an outgoing byte buffer.
///
/// TODO: serialize the context back into BSON for the RPC response.
pub fn dandelion_marshal(
    in_ctx: *mut DandelionBody,
    out_buf: *mut u8,
    out_bytes: i32,
) -> bool {
    debug!("dandelion_marshal: marshaling {:?}", unsafe { &*in_ctx });

    // 1. Linearize the body into a temporary Rust Vec
    let lin_resp = linearize_dandelion_body(unsafe { &mut *in_ctx });

    let data_len = lin_resp.len();

    // 2. Safety Check: Does the data fit in the C-provided buffer?
    if data_len > out_bytes as usize {
        error!(
            "Buffer overflow! Data size {} exceeds C buffer size {}",
            data_len, out_bytes
        );
        return false;
    }
    // 3. Copy the data into the C buffer
    unsafe {
        std::ptr::copy_nonoverlapping(lin_resp.as_ptr(), out_buf, data_len);
    }
    true
}
