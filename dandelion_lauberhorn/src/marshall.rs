use bytes::Bytes;
use dandelion_server::DandelionRequest;
use machine_interface::memory_domain::bytes_context::BytesContext;
pub use machine_interface::{
    memory_domain::{Context, ContextType},
    DataItem, DataSet, Position,
};

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
pub fn dandelion_unmarshal(out_ctx: *mut Context, in_buf: *const u8, in_bytes: i32) -> bool {
    let input = unsafe { std::slice::from_raw_parts(in_buf, in_bytes as usize) };
    match parse_req_ctx(input) {
        Ok((_function_name, context)) => {
            unsafe { *out_ctx = context };
            true
        }
        Err(_) => false,
    }
}

/// Marshal a `Context` into an outgoing byte buffer.
///
/// TODO: serialize the context back into BSON for the RPC response.
pub fn dandelion_marshal(_in_ctx: *const Context, _out_buf: *mut u8, _out_buf_size: i32) -> bool {
    true
}
