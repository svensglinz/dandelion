use serde::{Serialize, Deserialize};

fn default_path() -> String {
    String::new()
}

/// Struct containing registration information for new function
#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterFunction {
    /// String name of the function
    pub name: String,
    /// Default size for context to allocate to execute function
    pub context_size: u64,
    /// Which engine the function should be executed on
    pub engine_type: String,
    /// Optional local path to the binary if it is already on local disc
    #[serde(default = "default_path")]
    pub local_path: String,
    /// Binary representation of the function, ignored if a local path is given
    pub binary: Vec<u8>,
    /// Metadata for the sets and optionally static items to pass into the function for that set
    pub input_sets: Vec<(String, Option<Vec<(String, Vec<u8>)>>)>,
    /// output set names
    pub output_sets: Vec<String>,
}


#[derive(Debug, Deserialize)]
pub struct RegisterChain {
    pub composition: String,
}