use std::{
    sync::Arc,
};

use dispatcher::function_registry::CompositionInfo;
use machine_interface::{
    composition::{
        CompositionSet
    },
    function_driver::thread_utils::Engine,
};

use crate::{
    dispatcher::{dispatcher::Dispatcher, utils::input_sets_to_comp_sets},
    lauberhorn::{marshal::{DandelionRPCRequest}}, runtime::RuntimeContext
};

pub fn execute_composition<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    comp_info: CompositionInfo,
    request: &mut DandelionRPCRequest,
) -> Result<Vec<Option<CompositionSet>>, ()> {

    let inputs = input_sets_to_comp_sets(&request.data.sets);
    // set up dispatcher, insert tasks and run
    let mut dispatcher = Dispatcher::new(ctx.clone(), inputs, comp_info);
    dispatcher.run();
    Ok(dispatcher.get_result())
}
