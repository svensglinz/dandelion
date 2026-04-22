use std::sync::Arc;

use dandelion_commons::FunctionId;
use dandelion_server::DandelionRequest;
use dispatcher::function_registry::FunctionRegistry;
use machine_interface::function_driver::thread_utils::Engine;

use crate::webserver::schemas::InputSet;

/// Per-service context shared between lauberhorn FFI callbacks and the runtime.
pub struct LauberhornServiceCtx<E: Engine> {
    pub function_registry: Arc<FunctionRegistry>,
    /// One engine pointer per core — lauberhorn guarantees no concurrent access
    /// to the same index.
    pub engines: Vec<*mut E>,
    // pub function_id: FunctionId, // 
    pub id: usize,
}

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