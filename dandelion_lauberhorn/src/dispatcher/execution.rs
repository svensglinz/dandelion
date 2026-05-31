use crate::dispatcher::composition::execute_composition;
use crate::dispatcher::utils::{comp_sets_to_input_sets, make_comp_set, parse_req_ctx_from_input_sets};
use crate::lauberhorn::marshal::{
    DandelionRPCRequest
};
use crate::runtime::RuntimeContext;
use crate::webserver::schemas::InputSet;
use dandelion_commons::records::Recorder;
use dispatcher::dispatcher::DispatcherInput;
use dispatcher::function_registry::{FunctionInfo, FunctionType};
use log::{debug, error};
use machine_interface::composition::{CompositionSet
};
use machine_interface::function_driver::functions::FunctionAlternative;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::Context;
use machine_interface::DataSet;
use std::sync::Arc;
use std::time::Instant;

/// Entry point called from the FFI handler - Runs the function and returns
/// a Vec<Option<CompositionSet>>
pub fn dandelion_handler<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    request: &mut DandelionRPCRequest,
) -> Result<Vec<InputSet>, ()> {
    let function_id = Arc::new(request.function_name.clone());
    debug!("Looking up function '{}' in registry", request.function_name);

    let func = match ctx.registry.get_function(&function_id) {
        Ok(f) => f,
        Err(_) => {
            error!("Function not found in registry");
            return Err(());
        }
    };

    let result = match func {
        // atomic function
        FunctionType::Function(ref func_info) => {
            log::debug!("executing function");
            execute_function(ctx, func_info, request)
        }
        // composition function
        FunctionType::Composition(comp_info) => {
            log::debug!("executing composition");
            execute_composition(ctx, comp_info, request)
        }
        // system function
        FunctionType::SystemFunction(ref _func_info) => {
            todo!("System function execution not implemented yet")
        }
    };

    // verify result
    let result = match result {
        Ok(r) => r,
        Err(_) => {
            error!("Function execution failed");
            return Err(());
        }
    };

    // transform CompositionSet into InputSet to return back to user
    let sets = comp_sets_to_input_sets(&result);
    Ok(sets)
}


/// Execute a function on the given engine using the provided request context.
/// // what shoudl we get back here ?
pub fn execute_function<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    func_info: &FunctionInfo,
    request: &mut DandelionRPCRequest, // will be assembled HERE
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
    let mut engine = match ctx.engines.claim() {
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
    let ctx = engine
        .run(
            function.config.clone(),
            function_context,
            &func_info.metadata.output_sets,
        )
        .map_err(|e| {
            error!("Function execution failed: {}", e);
            ()
        })?;

    Ok(make_comp_set(ctx))
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
