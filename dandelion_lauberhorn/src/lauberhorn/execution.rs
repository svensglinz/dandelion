use crate::lauberhorn::types::{DandelionArgs, DandelionRPCRequest, LauberhornServiceCtx};
use dandelion_commons::FunctionId;
use dandelion_commons::records::Recorder;
use dandelion_server::DandelionBody;
use dispatcher::dispatcher::DispatcherInput;
use dispatcher::function_registry::{FunctionInfo, FunctionType};
use log::{debug, error};
use machine_interface::composition::{CompositionSet, JoinStrategy};
use machine_interface::function_driver::functions::FunctionAlternative;
use machine_interface::function_driver::thread_utils::Engine;
use machine_interface::function_driver::Metadata;
use machine_interface::machine_config::EngineType;
use machine_interface::memory_domain::bytes_context::BytesContext;
use machine_interface::memory_domain::{Context, ContextType};
use machine_interface::{DataItem, DataSet};
use std::sync::Arc;
use std::time::Instant;
use machine_interface::composition::FunctionDependencies; 
use bytes::Bytes;

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
pub fn execute_lauberhorn_function<E: Engine>(
    ctx: *mut LauberhornServiceCtx<E>,
    req_ctx: *mut DandelionRPCRequest,
    _xid: i32,
) -> LauberhornExecResult {
    // debug!(
    //     "Received request to execute function with context at {:?}",
    //     unsafe { &*req_ctx }
    // );

    // parse RPC Request into context
    let rpc_req = unsafe { &*req_ctx };
    let context = match parse_req_ctx(&rpc_req.payload) {
        Ok(ctx) => ctx,
        Err(_) => {
            error!("Failed to parse request context");
            return std::ptr::null_mut();
        }
    };
    let function_id = Arc::new(rpc_req.function_name.clone());
    let service_ctx = unsafe { &*ctx };

    debug!("Looking up function '{}' in registry", rpc_req.function_name); // service ctx has no more function_id!

    let registry = &service_ctx.function_registry;

    // function must exist. If it doesnt, bug, as we
    // have to ensure we register function before we register the service that uses it
    let func = match registry
        .get_function(&function_id) {
        Ok(f) => f,
        Err(_) => {
            error!("Function not found in registry");
            return std::ptr::null_mut();
        }
    };

    // SAFETY: lauberhorn guarantees only one thread accesses each engine at a time.
    let engine: *mut E = service_ctx.engines[service_ctx.id];

    // What to return to client ?
    // Vec<Option<CompositionSet>>
    // which we will transform into a dandelion_server::DandelionBody

    let result_ctx = match func {
        FunctionType::Function(ref func_info) => {
            execute_function(engine, func_info, context)
        },
        FunctionType::Composition(_comp_info) => {
            // execute_composition() ?
            todo!("Composition execution not implemented yet")
        },
        FunctionType::SystemFunction(ref _func_info) => {
            todo!("System function execution not implemented yet")
        }
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

    // ISSUE: compositions expect a Vec<Option<CompositionSet>>,
    // but we want to return a DandelionBody here, as this is what the user expects and what the marshaller can handle
    // Q: do function result composition sets contain and information from other
    // functions or can we deserialize to this from dandelionBody which we receive as RPC answer ? 
    let result = DandelionBody::new(comp_set, &Recorder {});
    Box::into_raw(Box::new(result))

    // efficiency of serializing to network response -> building context from it again,
    // then builidng as composition set again
    // does this make sense

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

#[derive(Clone)]
struct Task {
    function_id: FunctionId,

    inputs: Vec<Option<CompositionSet>>, // maybe also need sharingInfo in here ? check with dispatcher
    missing_input_ids: Vec<usize>, // do we need to store InputSetDescriptor::sharding ? 
    missing_input_ids_optional: Vec<usize>,
    join_info: (Vec<usize>, Vec<JoinStrategy>),
    output_set_ids: Vec<Option<usize>>
}

impl Task {
    pub fn from_dependency(
        dependency: &FunctionDependencies,
        _inputs: &Vec<Option<CompositionSet>>,
    ) -> Self {
        let mut task = Task {
            function_id: dependency.function.clone(),
            inputs: vec![None; dependency.input_set_ids.len()],
            missing_input_ids: Vec::new(),
            missing_input_ids_optional: Vec::new(),
            join_info: (Vec::new(), Vec::new()),
            output_set_ids: Vec::new(),
        };

        for input_set in &dependency.input_set_ids {
        // process each input set
        if let Some(descriptor) = input_set {
            if descriptor.optional {
                task.missing_input_ids_optional.push(descriptor.composition_id);
            } else {
                task.missing_input_ids.push(descriptor.composition_id);
            }
        }
    }
        task
    }

    /// Returns true if all required inputs have been provided, false otherwise
    pub fn is_ready(&self) -> bool {
        self.missing_input_ids.is_empty()
    }

    /// provides an input set to the task, updating the missing counts accordingly
    pub fn provide_input(&mut self, input_index: usize, set: Option<CompositionSet>) {
        if let Some(set) = set {
            self.inputs[input_index] = Some(set); // not sure if this is correct, as input_index may be large ? maybe just append ? 
            if self.missing_input_ids.contains(&input_index) {
                self.missing_input_ids.retain(|&id| id != input_index);
            } else if self.missing_input_ids_optional.contains(&input_index) {
                self.missing_input_ids_optional.retain(|&id| id != input_index);
            }
        }
    }
}

// pub fn execute_composition<E: Engine>(
//     composition: Composition,
//     inputs: Vec<Option<CompositionSet>>,
//     caching: bool,
// ) {
// 
//     // TODO: inject as dependency
//     let rpc_client = OncRpcClient::new(UdpSocket::bind("1.1.1.1:0").unwrap());
// 
//     let (done_tx, done_rx) = std::sync::mpsc::channel::<(u32, Vec<u8>)>();
//     let mut ready_tasks: VecDeque<Task> = VecDeque::new();
//     let mut blocked: HashMap<usize, Vec<Task>> = HashMap::new();
// 
//     // initialize output sets based on composition output map and input availability
//     let output_number = composition.output_map.len();
//     let mut output_sets: Vec<Option<CompositionSet>> = Vec::with_capacity(output_number);
//     output_sets.resize(output_number, None);
//     for (input_index, input_set) in inputs.iter().enumerate() {
//         if let Some(out_index) = composition.output_map.get(&input_index) {
//             output_sets[*out_index] = input_set.clone();
//         }
//     }
// 
//     //  create initially ready tasks based on composition dependencies and input availability
//     for dep in &composition.dependencies {
//         let task = Task::from_dependency(dep, &inputs); // could produce none ? 
//         if task.is_ready() {
//             ready_tasks.push_back(task);
//         } else {
//             // track which tasks are waiting for which inputs
//             for missing in task.missing_input_ids {
//                 blocked.entry(missing).or_default().push(task.clone());
//             }
//             for missing in task.missing_input_ids_optional {
//                 blocked.entry(missing).or_default().push(task.clone());
//             }
//         }
//     }
// 
//     loop {
//         // blast out all currently ready tasks as RPC calls
//         
//         for task in ready_tasks.drain(..) {
//             // send out function via rpc
//             // .encode() needs to return a DandelionRequest that we can marshal and send out as an RPC request - see test cases
//             // actually here we should call first the equivalent of queue_function_sharded
//             // and then this function will trigger the RPC call where it currently does queue_function
//             // but essentially queue_function_sharded shoudl here already be done arsynchronously, so we can already pass the channel to it, 
//             // and it will simply pass it down to the RPC client ? 
//             rpc_client.send_request_async(100, &task.encode(), "server:port", done_tx.clone());
//         }
// 
//         // block until any one response arrives
//         let (xid, response) = done_rx.recv().unwrap(); // or maybe already return it properly formatted here ? 
//         let (comp_set_idx, set) = parse_response(response); // what we get back from execution ! (probably via sharded...) comp_set_idx
//         // is the index of the composotion it belongs to 
// 
//         // set belongs to end result
//         if let Some(&out_idx) = composition.output_map.get(&comp_set_idx) {
//             output_sets[out_idx] = set.clone();
//        }
//     
//        // see if any other tasks become ready now
//        // remove all tasks form comp-set_idx as this dependency is now resopved
//        if let Some(waiting) = blocked.remove(&comp_set_idx) {
//             for mut task in waiting {
//                 task.provide_input(comp_set_idx, set.clone());
//                 if task.is_ready() {
//                      ready_tasks.push_back(task);
//                 }
//             }
//          }
//          // we somehow need to check how many requests are in flight, how many came back and when we can safely stop, as maybe we wont get answer anymore,
//          // received timeout etc. ? 
//     }
//     // check that all dependencies are resolved, if not, composition is not valid, as we have a cycle or missing input
// 
// }

// mocks queue_function_sharded for composition execution, as we need to trigger the RPC call here, and not in the dispatcher
// tink if we want separate function for this or integrate into execute_composition directly ? 

// async call ! -> call asynchronously functions in here we need to call. and get answers back by awaiting all the calls individually.
// then return via the channel ? 
// pub fn execute_function_sharded<E: Engine>(
// ) {
//     
// }


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
            DispatcherInput::Set(CompositionSet::from(( // dispatcherInput needed ? (transformed again below)
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

// idea to get the nested sets
/*
for t in tasks:
    future = run_node(t) // reutrns a handle with all tasks it needs to run, which ones it has dispatched and which ones may still be waiting
    futures.push(future)

for future in futures: 
    result = future.poll() // returns a list of tasks which are already done ? lots of polling ??? what if we have tasks that wstill need to run ? 

all_nodes = [NodeState { undispatched, in_flight, results } for each node]

struct Task {
    completed
    pending
    not_dispatched
}

for all ready tasks:
    task = prepare_task() // init it with what subtasks are pending (ie. individual shards)


// dispatch all possible subtasks
for t in tasks: 
    for subtask in t:
        map[id, t] = lauberhorn_try_call(subtask)

// await any one
loop:
    id = lauberhorn_await_any()
    t = map[id]
    result = lauberhorn_reap(id)
    t.mark_done(subtask, result)
    if t.is_done():
        // resolve new tasks that may depend on this one as we already have in the graph or resolve their dependnecy
        // get new ready tasks and dispatch them as well
        // have a list of tasks that we failed to spatch because no more free slots - try to disppatch theese now as well

now all dependencies should be resolved
loop:
    // try to dispatch as many tasks as possible across all nodes
    for node in all_nodes:
        while node.has_undispatched() and free_slots > 0:
            id = lauberhorn_async_call(node.next_task())
            id_map[id] = (node, task_idx)
            free_slots -= 1

    // block until at least one completes
    done_ids = lauberhorn_await_any()  // blocks until >=1 task done
    for id in done_ids:
        result = lauberhorn_reap(id)
        node, idx = id_map[id]
        node.results[idx] = result
        free_slots += 1

    if all_nodes.all(|n| n.done()):
        break
*/