use crate::dispatcher::composition::execute_composition;
use crate::dispatcher::function::execute_function;
use crate::dispatcher::system_function::execute_system_function;
use crate::dispatcher::utils::{comp_sets_to_input_sets};
use crate::lauberhorn::marshal::{
    DandelionRPCRequest
};
use crate::runtime::RuntimeContext;
use crate::webserver::schemas::InputSet;
use dispatcher::function_registry::{FunctionType};
use log::{debug, error};
use machine_interface::function_driver::thread_utils::Engine;
use std::sync::Arc;

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
            execute_system_function(ctx, _func_info, request)
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
