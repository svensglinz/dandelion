
/* register a function with the runtime, ELF already written to memory by HTTP frontend */
/* function is already registered by http frontend as well! */
/* probably need to have everything going via runtime ! HTTP frontend simply has a handle to the runtime ! */
// best: binary is written to disk by http request -> function is registerd in runtime when we call register_service !

// run lauberhorn
// where do we pin workers to core ? not for the moment!
// here we could iterate over resource and acquire them (-> given
// max. number of CPUs that are even scheudlable by Lauberhorn!)
// woudl also directly give us the enum index!

/*
 *context that the RPC handler that is invoked has access to at runtime
 * need access to 1: runtime (eg. for function registry)
 * need access to 2: function_id -> Q: why not directly place function struct ptr into here ?
 * would be more efficinet than getting it from registry
 * Q3: now only have 8 slots -> why not just have a generic slot -> where function_id is
 * represented as an RPC argument -> pick registered function from the registry
 * and run from there !
 * -> would lose the idea of direct call per RPC handler but this is how
 * all other RPC engines handle it as well (?)
*/

// what is relation / structure between input sets / metadata.input_serts ?
// bit stragne taht static-set can override some things ? dont quite get it ?

// ! -> static _sets may be set during compositions ! -> some stuff may be kept constant,
// just put it in as a static set ...
// but isnt metadata fixed once at function install time ?

/* function that is called in worker thread on reception of an RPC call */
/* args are already deserialized by lauberhorn runtime when we call this function ! */
/* this is the handler that is registered for simple functions,
we can have a separate handler that executes compositiosn ?  */
// or is this diff in => execute_function vs execute_composition (TBD) !




async call framework

lauberhorn_call_async(3-tuple, port, host, payload) -> int
- how to deliver msg : if external -> IP packet ? does NIC assemble this ? / if internal , can skip this ? 
- sent to lauberhorn via cache line as call
- lauberhorn stores into in table (int = table slot) -> int
- lauberhorn dispatches call
- receives answer
- buffers answer and marks slot as ready

lauberhorn_await(uint64) -> message in cacheline ? 
- block waits on cache line
- if lauberhorn receives msg, -> replies on cache line // one cache line per worker core for async calls


