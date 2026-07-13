use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use dispatcher::function_registry::CompositionInfo;
use log::debug;
use machine_interface::{
    composition::{get_sharding, CompositionSet},
    function_driver::thread_utils::Engine,
};

use crate::{
    dispatcher::{
        task::Task,
        utils::{comp_sets_to_input_sets, input_sets_to_comp_sets, reduce_shards}
    },
    lauberhorn::{
        laub_async::{AwaitSet, call_async},
        marshal::{DandelionRPCRequest, DandelionRPCResponse, InputSets},
    },
    runtime::RuntimeContext,
};

/// dispatcher drives the execution of a Composition
/// by breaking down individual Nodes (Tasks) into shards,
/// executing the shards, joining them once all shards form a task
/// are ready and then checks if further tasks can be activated
/// until the composition is fully executed
///
pub struct Dispatcher<E: Engine> {
    pub ctx: Arc<RuntimeContext<E>>,

    // handle -> (task_id, shard_idx)
    pub await_set: AwaitSet<DandelionRPCResponse>,
    pub in_flight: HashMap<i32, (usize, usize)>,

    // task_id -> task
    pub tasks: HashMap<usize, Task>,

    // comp_set_id -> task_ids waiting on it
    pub waiting_tasks: HashMap<usize, Vec<usize>>,
    pub waiting_optional_tasks: HashMap<usize, Vec<usize>>,
    pub pending_shards: HashMap<usize, usize>,
    pub shard_queue: VecDeque<(usize, usize, Vec<Option<CompositionSet>>)>,
    pub shard_results: HashMap<usize, Vec<Vec<Option<CompositionSet>>>>,

    // completed composition set outputs
    pub output_sets: Vec<Option<CompositionSet>>,
    pub composition_info: CompositionInfo,
    pub next_task_id: usize,
}

impl<E: Engine> Dispatcher<E> {
    /// create a new dispatcher for the given composition
    pub fn new(
        ctx: Arc<RuntimeContext<E>>,
        inputs: Vec<Option<CompositionSet>>,
        comp_info: CompositionInfo,
    ) -> Self {

        // initialize output sets
        // we need #(highest index output_map maps to) + 1
        // number of slots to store intermediate and final outputs
        let num_slots = comp_info.composition.output_map.keys().max().copied().unwrap_or(0) + 1;
        let mut output_sets = vec![None; num_slots];

        //  check if some of the inputs are outputs, if yes, store directly
        for (input_index, input_set) in inputs.iter().enumerate() {
            if let Some(&out_index) = comp_info.composition.output_map.get(&input_index) {
                output_sets[out_index] = input_set.clone();
            }
        }

        let mut dispatcher = Dispatcher {
            ctx,
            await_set: AwaitSet::new(),
            in_flight: HashMap::new(),
            tasks: HashMap::new(),
            waiting_tasks: HashMap::new(),
            waiting_optional_tasks: HashMap::new(),
            pending_shards: HashMap::new(),
            shard_queue: VecDeque::new(),
            shard_results: HashMap::new(),
            output_sets,
            composition_info: comp_info.clone(),
            next_task_id: 0,
        };

        // classify tasks into ready / blocked
        for dep in &comp_info.composition.dependencies {
            match Task::from_dependency(dep, &inputs) {
                None => {
                    // required input was empty, emit None outputs
                    for &out_id in dep.output_set_ids.iter().flatten() {
                        if let Some(&out_idx) = comp_info.composition.output_map.get(&out_id)
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
        dispatcher
    }

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
            for &missing in &task.missing_optional {
                self.waiting_optional_tasks
                    .entry(missing)
                    .or_default()
                    .push(task_id);
            }
        }
    }

    // enqueue a ready task to execute its shards
    fn enqueue_task(&mut self, task_id: usize) {
        // retreieve task
        let task = &self.tasks[&task_id];

        // split the task's inputs into shards
        let shards = get_sharding(
            task.inputs.clone(),
            task.join_info.0.clone(),
            task.join_info.1.clone(),
        );
        debug!("[dispatcher::enqueue_task] task_id={} function={} num_shards={}", task_id, task.function_id, shards.len());
        // add all shards as pending
        let count = shards.len().max(1);
        self.pending_shards.insert(task_id, count);
        self.shard_results.insert(task_id, vec![vec![]; count]);

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
    pub fn run(&mut self) {
        debug!(
            "[dispatcher::run] start shard_queue={} in_flight={}",
            self.shard_queue.len(),
            self.in_flight.len()
        );

        // drain queue and execute RPC requests until
        // 1. shard queue is empty
        // 2. no more in flight requests
        while !self.in_flight.is_empty() || !self.shard_queue.is_empty() {
            debug!(
                "[dispatcher::run] loop shard_queue={} in_flight={}",
                self.shard_queue.len(),
                self.in_flight.len()
            );

            self.try_drain_queue();

            debug!(
                "[dispatcher::run] after drain shard_queue={} in_flight={}",
                self.shard_queue.len(),
                self.in_flight.len()
            );

            // await a result
            match self.await_set.await_any() {
                None => {
                    debug!("[dispatcher::run] await_any returned None");
                    continue;
                }
                // provide result
                Some(ref mut handle) => {
                    debug!(
                        "[dispatcher::run] got result handle.id={}",
                        handle.id
                    );
                    // extract response
                    let r = handle.take_data().unwrap();
                    let (task_id, shard_idx) =
                        self.in_flight.remove(&(handle.id as i32)).unwrap();
                    debug!("[dispatcher::run] result for task_id={} shard_idx={} sets={}", task_id, shard_idx, r.sets.len());
                    // transform deserialized result (Vec<InputSet> to Vec<Option<CompositionSet>>
                    let result = input_sets_to_comp_sets(&r.sets);

                    self.insert_result(task_id, shard_idx, result);
                }
            }
        }
        debug!(
            "[dispatcher::run] done output_sets={}",
            self.output_sets.len()
        );
    }

    // get the final output sets after execution is done, ordered by the output index in the composition
    pub fn get_result(&self) -> Vec<Option<CompositionSet>> {
        let mut final_output: Vec<(usize, Option<CompositionSet>)> = self.output_sets
            .iter()
            .enumerate()
            .filter(|(slot_idx, _)| self.composition_info.composition.output_map.contains_key(slot_idx))
            .map(|(slot_idx, set)| (slot_idx, set.clone()))
            .collect();
        
        final_output.sort_by_key(|(slot_idx, _)| self.composition_info.composition.output_map[slot_idx]);
        final_output.into_iter().map(|(_, set)| set).collect()
    }

    /// execute as many requests as lauberhorn permits
    fn try_drain_queue(&mut self) {
        while let Some((task_id, shard_idx, shard)) = self.shard_queue.front() {
            let task = self.tasks.get(task_id).unwrap();
            debug!("[dispatcher::drain] dispatching task_id={} shard_idx={} function={}", task_id, shard_idx, task.function_id);

            let request = DandelionRPCRequest {
                function_name: task.function_id.to_string(),
                data: InputSets {
                    sets: comp_sets_to_input_sets(shard), // todo implement this
                },
            };
            match call_async(&*self.ctx.nested_ep, &request) {
                Ok(handle) => {
                    debug!(
                        "[dispatcher::drain] dispatched handle.id={}",
                        handle.id
                    );

                    // TODO(@Sven): in-flight, await-set could be combined ?
                    self.in_flight
                        .insert(handle.id as i32, (*task_id, *shard_idx));
                    self.await_set.add(handle);
                    self.shard_queue.pop_front();
                }
                // no more detailed errors so far, but likely out of table slots
                Err(()) => {
                    debug!(
                        "[dispatcher::drain] call_async failed — table full?"
                    );
                    break;
                }
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
        debug!("[dispatcher::insert_result] task_id={} shard_idx={} result_sets={}", task_id, shard_idx, result.len());

        // add the result to the shard results for the task
        self.shard_results.get_mut(&task_id).unwrap()[shard_idx] = result;

        // decrease pending shard counts
        let pending = self.pending_shards.get_mut(&task_id).unwrap();
        *pending -= 1; // or maybe better by indices

        debug!(
            "[dispatcher::insert_result] pending shards remaining={}",
            pending
        );
        if *pending > 0 {
            return;
        }

        // all shards done - reduce and deliver
        let results = self.shard_results.remove(&task_id).unwrap();
        self.pending_shards.remove(&task_id);
        let task = &self.tasks[&task_id];

        debug!(
            "[dispatcher::insert_result] all shards done, reducing task_id={}",
            task_id
        );
        let reduced = reduce_shards(results, &task.output_set_ids);
        debug!(
            "[dispatcher::insert_result] reduced into {} outputs",
            reduced.len()
        );

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
        debug!(
            "[dispatcher::provide_to_waiting] comp_set_idx={} waiting_tasks={}",
            comp_set_idx,
            self.waiting_tasks.get(&comp_set_idx).map_or(0, |v| v.len())
        );

        // insert result into output_sets
        if comp_set_idx < self.output_sets.len() {
            self.output_sets[comp_set_idx] = result.clone();
        }

        // notify required waiters
        let waiting =
            self.waiting_tasks.get(&comp_set_idx).cloned().unwrap_or_default();
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

        // notify optional waiters
        let waiting_optional = self
            .waiting_optional_tasks
            .get(&comp_set_idx)
            .cloned()
            .unwrap_or_default();
        for task_id in waiting_optional {
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
