use std::sync::Arc;
use std::time::Instant;
use log::{debug, error};
use dandelion_commons::records::Recorder;
use dispatcher::function_registry::{FunctionInfo, FunctionType};
use machine_interface::function_driver::functions::FunctionAlternative;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::Context;
use machine_interface::DataSet;

use crate::lauberhorn::types::LauberhornServiceCtx;

/// Entry point called from the FFI handler — looks up the function, runs it,
/// and returns the result context on the heap for marshalling.
pub fn execute_lauberhorn_function<E: Engine>(
    ctx: *mut LauberhornServiceCtx<E>,
    req_ctx: *mut Context,
    _xid: i32,
) -> *mut Context {

    debug!("Received request to execute function with context at {:p}", req_ctx);

    // Q: how to handle errors here ? 
    // we can't return a Result, but we also don't want to just panic and leak memory on the Rust side if something goes wrong
    
    let service_ctx = unsafe { &*ctx };
    let registry = &service_ctx.function_registry;

    let func = registry
        .get_function(&service_ctx.function_id)
        .expect("Function not found in registry");

    // SAFETY: lauberhorn guarantees only one thread accesses each engine at a time.
    let engine: *mut E = service_ctx.engines[service_ctx.id];

    let result_ctx = match func {
        FunctionType::Function(ref func_info) => {
            execute_function(engine, func_info, unsafe { &*req_ctx }).unwrap()
        }
        // or maybe return nullptr ? depends on how we want to handle errors in the FFI layer
        _ => panic!("Unsupported function type"),
    };

    Box::into_raw(Box::new(result_ctx))
}


/// Execute a function on the given engine using the provided request context.
pub fn execute_function<E: Engine>(
    engine: *mut E,
    func_info: &FunctionInfo,
    req_ctx: &Context,
) -> Result<Context, ()> {
    let variants = func_info
        .alternatives
        .read()
        .expect("Function registry lock is poisoned");

    let engine_type = unsafe { (*engine).get_engine_type() };
    let variant = get_function_variant(engine_type, &variants)
        .expect("Requested function not supported on this engine");

    let mut recorder = Recorder::new(Arc::new("lauberhorn".to_string()), Instant::now());
    let function = variant
        .load_function(false, &mut recorder)
        .expect("Failed to load function");

    let mut function_context = function
        .load(&variant.domain, variant.context_size)
        .expect("Failed to create function context");

    transfer_input_sets(&mut function_context, &func_info.metadata, &req_ctx.content);

    let ctx = unsafe {
        (*engine).run(
            function.config.clone(),
            function_context,
            &vec!["test".to_string()]
        ).unwrap()
    };
    Ok(ctx)
}

/// Copy input sets from the incoming context into the isolation context.
fn transfer_input_sets(
    function_context: &mut Context,
    metadata: &Metadata,
    input_content: &[Option<DataSet>],
) {
    for (set_index, (input_set_name, _static_set)) in
        metadata.input_sets.iter().enumerate()
    {
        let input_set =
            input_content.get(set_index).and_then(|opt| opt.as_ref());

        function_context.content.push(Some(DataSet {
            ident: input_set_name.clone(),
            buffers: input_set.map_or(vec![], |set| set.buffers.clone()),
        }));
    }
}

/// Find the function variant matching the given engine type.
fn get_function_variant(
    engine_type: EngineType,
    alternatives: &[Arc<FunctionAlternative>],
) -> Option<Arc<FunctionAlternative>> {
    alternatives.iter().find(|a| a.engine == engine_type).cloned()
}
