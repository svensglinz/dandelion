use std::{sync::Arc, time::Instant};

use dandelion_commons::records::Recorder;
use dispatcher::{dispatcher::DispatcherInput, function_registry::FunctionInfo};
use log::error;
use machine_interface::{composition::CompositionSet, function_driver::thread_utils::Engine};

use crate::{dispatcher::utils::{
    get_function_variant, make_comp_set, parse_req_ctx_from_input_sets, transfer_input_sets}, 
lauberhorn::marshal::DandelionRPCRequest, runtime::RuntimeContext
};

/// Execute a function on the given engine using the provided request context.
pub fn execute_function<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    func_info: &FunctionInfo,
    request: &mut DandelionRPCRequest,
) -> Result<Vec<Option<CompositionSet>>, ()> {
    
    // TODO(@Sven): turn this into Vec<InputSet> -> Vec<Option<CompositionSet> in one go
    let context = match parse_req_ctx_from_input_sets(&request.data.sets) {
        Ok(ctx) => ctx,
        Err(_) => {
            error!("Failed to parse request context");
            return Err(());
        }
    };

    // grab an available engine
    let mut engine = match ctx.engines.pop() {
        None => {
            log::error!("no free engine available. Should not happen");
            return Err(());
        }
        Some(e) => e,
    };

    let variants = func_info
        .alternatives
        .read()
        .expect("Function registry lock is poisoned");

    let engine_type = engine.get_engine_type();
    let variant = get_function_variant(engine_type, &variants)
        .expect("Requested function not supported on this engine");

    // NEEDED ?
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
    let request_number = context.content.len();
    let request_arc = Arc::new(context);
    let inputs = (0..request_number)
        .map(|set_id| {
            DispatcherInput::Set(CompositionSet::from((
                // dispatcherInput needed ? (transformed again below)
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
    let result_ctx = engine
        .run(
            function.config.clone(),
            function_context,
            &func_info.metadata.output_sets,
        )
        .map_err(|e| {
            error!("Function execution failed: {}", e);
            ()
        })?;
    
    // release engine back to pool
    ctx.engines.push(engine);

    Ok(make_comp_set(result_ctx))
}

