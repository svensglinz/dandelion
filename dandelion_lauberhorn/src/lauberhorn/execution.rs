use crate::lauberhorn::types::LauberhornServiceCtx;
use dandelion_commons::records::Recorder;
use dandelion_server::DandelionBody;
use dispatcher::dispatcher::DispatcherInput;
use dispatcher::function_registry::{FunctionInfo, FunctionType};
use log::{debug, error};
use machine_interface::composition::CompositionSet;
use machine_interface::function_driver::functions::FunctionAlternative;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::Context;
use machine_interface::DataSet;
use std::sync::Arc;
use std::time::Instant;

// type of pointer lauberhorn returns
type LauberhornExecResult = *mut DandelionBody;

/// Entry point called from the FFI handler — looks up the function, runs it,
/// and returns the result context on the heap for marshalling.
/// // for now, just executes a standalone function! No compositions yet !
/// // returns a DandelionBody that can be returned to the user (after marshalling ?)
///
/// Alternatively, if better fit with compoisitions,
/// can have this function return a Vec<Option<CompositionSet>>
/// that we can transform into a DandelionBody upon marshalling out or elsewhere ?
///
///
pub fn execute_lauberhorn_function<E: Engine>(
    ctx: *mut LauberhornServiceCtx<E>,
    req_ctx: *mut Context,
    _xid: i32,
) -> LauberhornExecResult {
    debug!(
        "Received request to execute function with context at {:?}",
        unsafe { &*req_ctx }
    );

    // Q: how to handle errors here ?
    // we can't return a Result, but we also don't want to just panic and leak memory on the Rust side if something goes wrong

    let service_ctx = unsafe { &*ctx };

    debug!("Looking up function '{}' in registry", service_ctx.function_id);

    let registry = &service_ctx.function_registry;

    // function must exist. If it doesnt, bug, as we
    // have to ensure we register function before we register the service that uses it
    let func = registry
        .get_function(&service_ctx.function_id)
        .expect("Function not found in registry");

    // SAFETY: lauberhorn guarantees only one thread accesses each engine at a time.
    let engine: *mut E = service_ctx.engines[service_ctx.id];

    // What to return to client ?
    // Vec<Option<CompositionSet>>
    // which we will transform into a dandelion_server::DandelionBody

    let result_ctx = match func {
        FunctionType::Function(ref func_info) => {
            execute_function(engine, func_info, unsafe {
                std::ptr::read(req_ctx)
            })
        }
        // or maybe return nullptr ? depends on how we want to handle errors in the FFI layer
        _ => panic!("Unsupported function type"),
    };

    let ctx = match result_ctx {
        Ok(ctx) => ctx,
        Err(_) => {
            error!("Function execution failed");
            return std::ptr::null_mut();
        }
    };

    // transform result into Vec<Option<CompositionSet>> and return pointer to it, or null on error
    let comp_set = make_comp_set(ctx);

    // now turn it into a message we can return to the client
    // (incl. marshalling ?) or just return plainly ?

    let result = DandelionBody::new(comp_set, &Recorder {});
    Box::into_raw(Box::new(result))

    // return ptr on success, null on error
    // match result_ctx {
    //     Ok(ctx) => Box::into_raw(Box::new(ctx)),
    //     Err(_) => std::ptr::null_mut()
    // }
}

// wrapper that wraps a context into a CompositionSet ? (maybe maek this an attribute of it or how ? )
// first check why we do this in the first place ?
pub fn make_comp_set(ctx: Context) -> Vec<Option<CompositionSet>> {
    let context_arc = Arc::new(ctx);
    let comp_sets = context_arc
        .content
        .iter()
        .enumerate()
        .map(|(function_set_id, data_option)| {
            data_option.as_ref().and_then(|_| {
                Some(CompositionSet::from((
                    function_set_id,
                    vec![context_arc.clone()],
                )))
            })
        })
        .collect();

    comp_sets
}

pub fn execute_composition<E: Engine>() {

    // Q: How are compositions stored ?
    // and called ? by name or only by raw composition -> ie
    // have to parse with queue_unregistered_composition on every call ?
}

/// Execute a function on the given engine using the provided request context.
/// // what shoudl we get back here ?
pub fn execute_function<E: Engine>(
    engine: *mut E,
    func_info: &FunctionInfo,
    req_ctx: Context,
) -> Result<Context, ()> {
    let variants = func_info
        .alternatives
        .read()
        .expect("Function registry lock is poisoned");

    let engine_type = unsafe { (*engine).get_engine_type() };
    let variant = get_function_variant(engine_type, &variants)
        .expect("Requested function not supported on this engine");

    let mut recorder =
        Recorder::new(Arc::new("lauberhorn".to_string()), Instant::now());
    let function = variant
        .load_function(false, &mut recorder)
        .expect("Failed to load function");

    // create function context
    let mut function_context = function
        .load(&variant.domain, variant.context_size)
        .expect("Failed to create function context");

    // FOLLOW DISPATCHER (server - src - main.rs :: serve_request
    let request_number = req_ctx.content.len();
    let request_arc = Arc::new(req_ctx);
    let inputs = (0..request_number)
        .map(|set_id| {
            DispatcherInput::Set(CompositionSet::from((
                set_id,
                vec![request_arc.clone()],
            )))
        })
        .collect::<Vec<_>>();

    // continue: dispatcher.rs :: queue_function_by_name
    let mut input_vec = Vec::with_capacity(inputs.len());
    input_vec.resize(inputs.len(), None);

    for (index, input) in inputs.into_iter().enumerate() {
        match input {
            DispatcherInput::None => (),
            DispatcherInput::Set(set) => {
                input_vec[index] = Some(set);
            }
        }
    }

    // transfer input sets from request context into function context
    transfer_input_sets(&mut function_context, &func_info.metadata, &input_vec);

    // execute function on engine
    let ctx = unsafe {
        (*engine)
            .run(
                function.config.clone(),
                function_context,
                &func_info.metadata.output_sets,
            )
            .map_err(|e| {
                error!("Function execution failed: {}", e);
                ()
            })?
    };
    Ok(ctx)
}

/// Copy input sets from the incoming context into the isolation context.
fn transfer_input_sets(
    function_context: &mut Context,
    metadata: &Metadata,
    input_sets: &Vec<Option<CompositionSet>>,
) {
    for (set_index, (input_set_name, static_set)) in
        metadata.input_sets.iter().enumerate()
    {
        let transfer_option = static_set
            .as_ref()
            .or_else(|| input_sets.get(set_index).and_then(|set| set.as_ref()));

        let capacity = transfer_option.map_or(0, |set| set.len());

        function_context.content.push(Some(DataSet {
            ident: input_set_name.clone(),
            buffers: Vec::with_capacity(capacity),
        }));

        if let Some(transfer_set) = transfer_option {
            for (source_set_index, source_item_index, source_context) in
                transfer_set
            {
                let _ = machine_interface::memory_domain::transfer_data_item(
                    function_context,
                    source_context,
                    set_index,
                    128, // where does this come from ?
                    input_set_name.as_str(),
                    source_set_index,
                    source_item_index,
                );
            }
        }
    }
}

/// Find the function variant matching the given engine type.
fn get_function_variant(
    engine_type: EngineType,
    alternatives: &[Arc<FunctionAlternative>],
) -> Option<Arc<FunctionAlternative>> {
    alternatives.iter().find(|a| a.engine == engine_type).cloned()
}
