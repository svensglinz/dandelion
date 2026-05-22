use crate::lauberhorn::laub_async::{AwaitSet, call_async};
use crate::lauberhorn::types::{
    DandelionArgs, DandelionNestedResponse, DandelionRPCRequest
};
use crate::lauberhorn::*;
use crate::runtime::RuntimeContext;
use bytes::Bytes;
use dandelion_commons::records::Recorder;
use dandelion_commons::FunctionId;
use dandelion_server::DandelionBody;
use dispatcher::dispatcher::DispatcherInput;
use dispatcher::function_registry::{FunctionInfo, FunctionType};
use log::{debug, error};
use machine_interface::composition::{
    Composition, CompositionSet, FunctionDependencies, InputSetDescriptor,
    JoinStrategy, ShardingMode,
};
use machine_interface::function_driver::functions::FunctionAlternative;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::bytes_context::BytesContext;
use machine_interface::memory_domain::{Context, ContextType};
use machine_interface::{DataItem, DataSet};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

// type of pointer lauberhorn returns
type LauberhornExecResult = *mut DandelionBody;

/// Parse a BSON-serialized `DandelionRequest` into a `Context`.
///
/// Lauberhorn RPC delivers each request as a single contiguous buffer,
/// so this is a simplified single-frame variant of `BytesContext::from_bytes_vec`.
/// // Q: is buffer persistent on Lauberhorn until request is finished executing ?
/// Yes: -> implement copy-free version
fn parse_req_ctx(input: &DandelionArgs) -> Result<Context, ()> {
    let mut flat_data = Vec::new();
    let mut content = Vec::new();

    for set in &input.sets {
        let mut buffers = Vec::new();
        for item in &set.items {
            let offset = flat_data.len();
            let size = item.data.len();
            flat_data.extend_from_slice(&item.data);
            buffers.push(DataItem {
                ident: item.identifier.clone(),
                key: item.key,
                data: machine_interface::Position { offset, size },
            });
        }
        content.push(Some(DataSet { ident: set.identifier.clone(), buffers }));
    }

    let bytes = Bytes::from(flat_data);
    let bytes_ctx = BytesContext::new(vec![bytes.clone()]);
    let mut context =
        Context::new(ContextType::Bytes(Box::new(bytes_ctx)), bytes.len());
    context.content = content;
    Ok(context)
}

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
pub fn dandelion_handler<E: Engine>(
    ctx: &mut RuntimeContext<E>,
    request: &mut DandelionRPCRequest,
) -> Result<Vec<Option<CompositionSet>>, ()> {
   
    let function_id = Arc::new(request.function_name.clone());
    debug!("Looking up function '{}' in registry", request.function_name);

    let registry = &ctx.registry;

    // function must exist. If it doesnt, bug, as we
    // have to ensure we register function before we register the service that uses it
    let func = match registry.get_function(&function_id) {
        Ok(f) => f,
        Err(_) => {
            error!("Function not found in registry");
            return Err(())
        }
    };

    // What to return to client ?
    // Vec<Option<CompositionSet>>
    // which we will transform into a dandelion_server::DandelionBody

    let result: Result<Vec<Option<CompositionSet>>, ()> = match func {
        FunctionType::Function(ref func_info) => {
            execute_function(ctx, func_info, request)
        }
        FunctionType::Composition(_comp_info) => {
            // execute_composition(...);
            // composition here gets access to the global results table !
            todo!("Composition execution not implemented yet")
        }
        FunctionType::SystemFunction(ref _func_info) => {
            todo!("System function execution not implemented yet")
        }
    };

    let result = match result {
        Ok(r) => r,
        Err(_) => {
            error!("Function execution failed");
            return Err(())
        }
    };
    Ok(result)
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

// copied from original code
pub fn get_sharding(
    mut sets: Vec<Option<(ShardingMode, CompositionSet)>>,
    mut join_order: Vec<usize>,
    mut join_strategies: Vec<JoinStrategy>,
) -> Vec<Vec<Option<CompositionSet>>> {
    let set_num = sets.len();
    let mut final_sharding = Vec::new();

    if set_num == 0 {
        return final_sharding;
    }

    // make sure every set is in the order and has a strategy
    let mut missing_sets: Vec<_> =
        (0..set_num).map(|index| Some(index)).collect();
    for index in join_order.iter() {
        missing_sets[*index] = None;
    }
    for missing_index in missing_sets {
        if let Some(missing) = missing_index {
            join_order.push(missing);
        }
    }
    join_strategies.resize(set_num - 1, JoinStrategy::Cross);

    let mut join_iter_opt = JoinIterator::new(
        JoinStrategy::Outer,
        None,
        sets[join_order[0]].take(),
        join_order[0],
    );
    for (set_index, startegy) in join_order[1..].iter().zip_eq(join_strategies)
    {
        join_iter_opt = JoinIterator::new(
            startegy,
            join_iter_opt,
            sets[*set_index].take(),
            *set_index,
        );
    }

    if let Some(mut join_iter) = join_iter_opt {
        let mut new_sets = Vec::with_capacity(set_num);
        new_sets.resize(set_num, None);
        join_iter.fill_in(&mut new_sets);
        final_sharding.push(new_sets);
        while join_iter.advance() {
            let mut advance_sets = Vec::with_capacity(set_num);
            advance_sets.resize(set_num, None);
            join_iter.fill_in(&mut advance_sets);
            final_sharding.push(advance_sets);
        }
    }

    final_sharding
}
    
// dispatcher manages execution of individual
struct Dispatcher<E: Engine> {
    ctx: RuntimeContext<E>,
    // handle -> (task_id, shard_idx)
    await_set: AwaitSet<DandelionNestedResponse>,
    in_flight: HashMap<i32, (usize, usize)>,
    tasks: HashMap<usize, Task>,

    // comp_set_id -> task_ids waiting on it
    waiting_tasks: HashMap<usize, Vec<usize>>,
    // waiting_optional_tasks: HashMap<usize, Vec<usize>>, (potentially add this ?)
    pending_shards: HashMap<usize, usize>,
    shard_queue: VecDeque<(usize, usize, Vec<Option<CompositionSet>>)>,
    shard_results: HashMap<usize, Vec<Vec<Option<CompositionSet>>>>,

    // completed composition set outputs
    output_sets: Vec<Option<CompositionSet>>,

    next_task_id: usize,
}

impl<E: Engine> Dispatcher<E> { // TODO(@Sven): can we remove the type param somehow ? 
    fn new(
        ctx: RuntimeContext<E>,
        output_sets: Vec<Option<CompositionSet>>,
    ) -> Self {
        Dispatcher {
            ctx,
            await_set: AwaitSet::new(),
            in_flight: HashMap::new(),
            tasks: HashMap::new(),
            waiting_tasks: HashMap::new(),
            pending_shards: HashMap::new(),
            shard_queue: VecDeque::new(),
            shard_results: HashMap::new(),
            output_sets,
            next_task_id: 0,
        }
    }

    //  else {
    //
    //                 for &missing in &task.missing {
    //                     blocked.entry(missing).or_default().push(task.clone());
    //                 }

    // OPTIONAL MAP NOT ADDED YET !
    //                 for &missing in &task.missing_optional {
    //                     blocked.entry(missing).or_default().push(task.clone());
    //                 }
    //             }
    //

    // provide a task to the dispatcher (before running it)
    fn insert_task(&mut self, task: Task) {
        // take ownership of task
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.insert(task_id, task);
        let task = &self.tasks[&task_id];

        // check if task is ready or not
        if task.is_executable() {
            // if ready, store in task queue
            self.enqueue_task(task_id);
        } else {
            // otherwise, store task_id as waiting on each missing task
            for &missing in &task.missing {
                self.waiting_tasks.entry(missing).or_default().push(task_id);
            }
        }
    }

    // enqueue a ready task to execute its shards
    fn enqueue_task(&mut self, task_id: usize) {
        let task = &self.tasks[&task_id];

        // split the tasks's inputs into shards
        let shards = get_sharding(
            task.inputs.clone(),
            task.join_info.0.clone(),
            task.join_info.1.clone(),
        );

        // add all shards as pending
        let count = shards.len().max(1);
        self.pending_shards.insert(task_id, count);
        self.shard_results.insert(task_id, vec![vec![None]; count]);

        for (shard_idx, shard) in shards.into_iter().enumerate() {
            self.shard_queue.push_back((task_id, shard_idx, shard));
        }
    }

    // run the dispatcher
    // 1.  Dispatch all pending shards until dispatch queue is empty or lauberhorn runs out of
    //     table slots to hold in flight items
    // 2.  Await any dispatched item
    // 3.  Provide the received item to the task
    // 4.  Check if this has produced any newly executable tasks, if yes, shard and push to ready queue
    // 5.  Repeat
    fn run(&mut self) {
        while !self.in_flight.is_empty() || !self.shard_queue.is_empty() {
            // dispatch all possible ready shards
            self.try_drain_queue();

            // await a result
            match self.await_set.await_any() {
                None => continue,
                // provide result
                Some(ref handle) => {
                    
                    let r = handle.take_data().unwrap();

                    // need to be able to MOVE out of it ? 
                    let data =
                        self.ctx.nested_results.get(r.idx).unwrap();

                    // TODO(@Sven): release by index better ? 
                    self.ctx.nested_results.release(&data);

                    let (task_id, shard_idx) =
                        self.in_flight.remove(&(r.idx as i32)).unwrap();

                    self.insert_result(
                        task_id,
                        shard_idx,
                        data,
                    );
                }
            }
        }
    }

    fn try_drain_queue(&mut self) {
        while let Some((task_id, shard_idx, shard)) = self.shard_queue.front() {
            
            let request = DandelionRPCRequest {
                function_name: shard.____,
                payload: // need to transform the data to ... here, or again just COMMUNICATE VIA INTERNAL SLOTS ? 
            }
            // need to build a RpcRequest here with name & all to be supplied to the endpoint ...
            match call_async(&*self.ctx.nested_ep, shard) {
                Ok(handle_idx) => {
                    self.in_flight.insert(handle_idx, (*task_id, *shard_idx));
                    self.shard_queue.pop_front();
                }
                // no more detailed errors so far, but likely out of table slots
                Err(()) => break,
            }
        }
    }

    // insert a shard back - if a new task becomes runnable, shard it and add it to the dispatcher
    fn insert_result(
        &mut self,
        task_id: usize,
        shard_idx: usize,
        result: Vec<Option<CompositionSet>>,
    ) {
        // add the result to the shard results for the task
        self.shard_results.get_mut(&task_id).unwrap()[shard_idx] = result;

        // decrease pending shard counts
        let pending = self.pending_shards.get_mut(&task_id).unwrap();
        *pending -= 1; // or maybe better by indices

        if *pending > 0 {
            return;
        }

        // all shards done - reduce and deliver
        let results = self.shard_results.remove(&task_id).unwrap();
        self.pending_shards.remove(&task_id);
        let task = &self.tasks[&task_id];
        let reduced = reduce_shards(results, &task.output_set_ids);

        for (comp_set_idx, set) in reduced {
            self.provide_to_waiting(comp_set_idx, set); // makes other tasks ready
        }
    }

    // provide the result of a task to other waiting tasks
    // if a task becomes executabke as a result, enqueue this task for execution
    fn provide_to_waiting(
        &mut self,
        comp_set_idx: usize,
        result: Option<CompositionSet>,
    ) {
        if comp_set_idx < self.output_sets.len() {
            self.output_sets[comp_set_idx] = result.clone();
        }

        let waiting = match self.waiting_tasks.get(&comp_set_idx) {
            Some(w) => w.clone(),
            None => return,
        };

        for task_id in waiting {
            let became_ready = self
                .tasks
                .get_mut(&task_id)
                .unwrap()
                .provide_input(comp_set_idx, result.clone());

            if became_ready && self.tasks[&task_id].is_executable() {
                self.enqueue_task(task_id);
            }
        }
    }
}

#[derive(Clone)]
struct Task {
    // node name
    function_id: FunctionId,
    // required inputs for this function to run
    inputs: Vec<Option<(ShardingMode, CompositionSet)>>,
    // still missing inputs
    missing: BTreeSet<usize>,
    id_to_slots: HashMap<usize, Vec<(usize, ShardingMode)>>,
    // missing optional inputs
    missing_optional: BTreeSet<usize>,
    // ???
    join_info: (Vec<usize>, Vec<JoinStrategy>),
    // output set ids ??
    output_set_ids: Vec<Option<usize>>,
}

// One node in the graph
impl Task {
    // Returns None if a required input is empty (short-circuit, all outputs -> None)
    pub fn from_dependency(
        dependency: &FunctionDependencies,
        inputs: &[Option<CompositionSet>],
    ) -> Option<Self> {
        let mut task = Task {
            function_id: dependency.function.clone(),
            inputs: vec![None; dependency.input_set_ids.len()],
            missing: BTreeSet::new(),
            id_to_slots: HashMap::new(),
            missing_optional: BTreeSet::new(),
            join_info: dependency.join_info.clone(),
            output_set_ids: dependency.output_set_ids.clone(),
        };

        for (fn_idx, slot) in dependency.input_set_ids.iter().enumerate() {
            let Some(descriptor) = slot else { continue };
            let InputSetDescriptor { composition_id, sharding, optional } =
                *descriptor;

            // track slot mapping for later provide_input calls
            task.id_to_slots
                .entry(composition_id)
                .or_default()
                .push((fn_idx, sharding));

            match inputs.get(composition_id) {
                // input exists and is non-empty
                Some(Some(set)) if !set.is_empty() => {
                    task.inputs[fn_idx] = Some((sharding, set.clone()));
                }
                // required input is empty/None -> short circuit
                Some(_) if !optional => return None,
                // optional input is empty/None -> leave as None, don't add to missing
                Some(_) => {}
                // not available yet
                None if optional => {
                    task.missing_optional.insert(composition_id);
                }
                None => {
                    task.missing.insert(composition_id);
                }
            }
        }
        Some(task)
    }

    /// Returns true if all required inputs have been provided, false otherwise
    pub fn is_executable(&self) -> bool {
        self.missing.is_empty()
    }

    pub fn is_executed(&self) -> bool {
        // sharding result.size == expctd results ?
        true
    }

    /// provides an input set to the task, updating the missing counts accordingly
    pub fn provide_input(
        &mut self,
        composition_id: usize,
        set: Option<CompositionSet>,
    ) -> bool {
        let is_empty = set.as_ref().map_or(true, |s| s.is_empty());
        let is_required = self.missing.contains(&composition_id);

        // CHECK THIS LOGIC ?
        if is_required && is_empty {
            return false;
        }
        self.missing.remove(&composition_id);
        self.missing_optional.remove(&composition_id);
        if let Some(slots) = self.id_to_slots.get(&composition_id) {
            for &(fn_idx, sharding) in slots {
                self.inputs[fn_idx] = set.clone().map(|s| (sharding, s));
            }
        }
        true
    }
}

// thisshoudl return a set of pending dispatches !!!
/* executes a single sharded function and writes its result to the output channel */
// pub fn execute_function_sharded<E: Engine>(
//     function_id: FunctionId,
//     input_sets: Vec<Option<(ShardingMode, CompositionSet)>>,
//     join_order: Vec<usize>,
//     join_strategies: Vec<JoinStrategy>,
//     output_mapping: Vec<Option<usize>>,
//     caching: bool,
//     // dont use recorder here...
// ) -> lauberhorn::AwaitSet {
//     let mut await_set = lauberhorn::AwaitSet::new();
// 
//     // check if any composition sets are non-empty
//     let is_sharded = input_sets
//         .iter()
//         .any(|o| o.as_ref().map_or(false, |(_, s)| !s.is_empty()));
// 
//     if is_sharded {
//         for shard in get_sharding(input_sets, join_order, join_strategies) {
//             let idx = lauberhorn::call_async();
//             await_set.add(&[idx]);
//         }
//     } else {
//         let idx = lauberhorn::call_async();
//         await_set.add(&[idx]);
//     }
// 
//     // here return the await_set (then await on await_any_of(set1, set2, ....)
//     let sharded_results = lauberhorn::await_all(&await_set);
//     let reduced = reduce_shards(sharded_results, &output_mapping);
// 
//     // do output collection, accumulator etc...
// }

fn reduce_shards(
    shard_results: Vec<Vec<Option<CompositionSet>>>,
    output_mapping: &[Option<usize>],
) -> Vec<(usize, Option<CompositionSet>)> {
    let mut combined: Vec<Option<CompositionSet>> =
        vec![None; output_mapping.len()];

    for shard in shard_results {
        for (slot, set) in combined.iter_mut().zip(shard) {
            if let (Some(old), Some(new)) = (slot.as_mut(), set) {
                old.combine(new).expect("combine failed");
            } else if slot.is_none() {
                *slot = set;
            }
        }
    }

    let mut result = Vec::new();
    for (idx_opt, set) in output_mapping.iter().zip(combined) {
        if let Some(idx) = idx_opt {
            result.push((*idx, set));
        }
    }
    result
}

// ShardExecution struct, where we submit results and know if one is finished
// and we can continue processing it

pub fn execute_composition<E: Engine>(
    ctx: &RuntimeContext<E>,
    composition: Composition,
    inputs: Vec<Option<CompositionSet>>,
    caching: bool,
) -> Result<Vec<Option<CompositionSet>>, ()> {
    // initialize output sets
    let mut output_sets: Vec<Option<CompositionSet>> =
        vec![None; composition.output_map.len()];
    for (input_index, input_set) in inputs.iter().enumerate() {
        if let Some(&out_index) = composition.output_map.get(&input_index) {
            output_sets[out_index] = input_set.clone();
        }
    }

    let mut dispatcher = Dispatcher::new(ctx, output_sets);

    // classify tasks into ready / blocked
    for dep in &composition.dependencies {
        match Task::from_dependency(dep, &inputs) {
            None => {
                // required input was empty, immediately emit None outputs
                for &out_id in dep.output_set_ids.iter().flatten() {
                    if let Some(&out_idx) = composition.output_map.get(&out_id)
                    {
                        output_sets[out_idx] = None;
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
    Ok(dispatcher.output_sets)
}

/// Execute a function on the given engine using the provided request context.
/// // what shoudl we get back here ?
pub fn execute_function<E: Engine>(
    ctx: &RuntimeContext<E>,
    func_info: &FunctionInfo,
    request: &mut DandelionRPCRequest, // will be assembled HERE
) -> Result<Vec<Option<CompositionSet>>, ()> {

    let context = match parse_req_ctx(&request.payload) {
        Ok(ctx) => ctx,
        Err(_) => {
            error!("Failed to parse request context");
            return Err(())
        }
    };

    // grab an available engine
    let mut engine = match ctx.engines.claim() {
        None => {
            log::error!("no free engine available. Should not happen");
            return Err(())
        }
        Some(e) => e
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
    let ctx = unsafe {
        engine
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
