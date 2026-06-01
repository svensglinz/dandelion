use std::sync::Arc;

use dispatcher::function_registry::FunctionInfo;
use machine_interface::{composition::CompositionSet, function_driver::thread_utils::Engine};

use crate::{lauberhorn::marshal::DandelionRPCRequest, runtime::RuntimeContext};

pub fn execute_system_function<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    func_info: &FunctionInfo,
    request: &mut DandelionRPCRequest,
) -> Result<Vec<Option<CompositionSet>>, ()> {
    // TODO(@Sven): implement system function execution
    todo!("System function execution not implemented yet")
    // run_http_system_function is key here ? 
    // problem is also cannot do this async as : -> response will not be sth we can deserialize...
    // should the runtime spawn a separate tokio runtime where worker threads can submit stuff to
    // and this will live on a separate thread / core which we dont care about ?

}
