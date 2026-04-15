use std::sync::Arc;

use dandelion_commons::FunctionId;
use dispatcher::function_registry::FunctionRegistry;
use machine_interface::function_driver::thread_utils::Engine;

/// Per-service context shared between lauberhorn FFI callbacks and the runtime.
pub struct LauberhornServiceCtx<E: Engine> {
    pub function_registry: Arc<FunctionRegistry>,
    /// One engine pointer per core — lauberhorn guarantees no concurrent access
    /// to the same index.
    pub engines: Vec<*mut E>,
    pub function_id: FunctionId, // 
    pub id: usize,
}
