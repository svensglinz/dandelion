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
    dispatcher::{dispatcher::Dispatcher, task::Task, utils::input_sets_to_comp_sets},
    lauberhorn::{marshal::{DandelionRPCRequest}}, runtime::RuntimeContext
};

pub fn execute_composition<E: Engine>(
    ctx: Arc<RuntimeContext<E>>,
    comp_info: CompositionInfo,
    request: &mut DandelionRPCRequest,
    // inputs: Vec<Option<CompositionSet>>,
    // caching: bool,
) -> Result<Vec<Option<CompositionSet>>, ()> {
    let composition = comp_info.composition;
    let inputs = input_sets_to_comp_sets(&request.data.sets);

    // initialize output sets
    // let mut output_sets: Vec<Option<CompositionSet>> =
    //     vec![None; composition.output_map.len()];
    let num_slots = composition.output_map.keys().max().copied().unwrap_or(0) + 1;
    let mut output_sets = vec![None; num_slots];

    //  check if some of the inputs are outputs
    for (input_index, input_set) in inputs.iter().enumerate() {
        if let Some(&out_index) = composition.output_map.get(&input_index) {
            output_sets[out_index] = input_set.clone();
        }
    }

    // set up dispatcher, insert tasks and run
    let mut dispatcher = Dispatcher::new(ctx.clone(), output_sets);

    // classify tasks into ready / blocked
    for dep in &composition.dependencies {
        match Task::from_dependency(dep, &inputs) {
            None => {
                // required input was empty, emit None outputs
                for &out_id in dep.output_set_ids.iter().flatten() {
                    if let Some(&out_idx) = composition.output_map.get(&out_id)
                    {
                        dispatcher.output_sets[out_idx] = None;
                    }
                }
            }
            Some(task) => {
                dispatcher.insert_task(task);
            }
        }
    }
    dispatcher.run();

    // TODO(@Sven): check if result is complete or if we aborted early
    // TODO(@Sven): include this as dispatcher.get_result() or similar ? 
    let mut final_output: Vec<(usize, Option<CompositionSet>)> = dispatcher.output_sets
        .into_iter()
        .enumerate()
        .filter(|(slot_idx, _)| composition.output_map.contains_key(slot_idx))
        .collect();

    final_output.sort_by_key(|(slot_idx, _)| composition.output_map[slot_idx]);    

    Ok(final_output.into_iter().map(|(_, set)| set).collect())

}
