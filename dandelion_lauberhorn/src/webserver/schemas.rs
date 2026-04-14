use std::sync::Arc;

use machine_interface::{
    DataItem, DataSet, Position, composition::CompositionSet, 
    function_driver::Metadata, memory_domain::read_only::ReadOnlyContext
};
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

impl RegisterFunction {
    pub fn into_metadata(self) -> Metadata {
            let input_sets = self
        .input_sets
        .into_iter()
        .map(|(name, data)| {
            if let Some(static_data) = data {
                let data_contexts = static_data
                    .into_iter()
                    .map(|(item_name, data_vec)| {
                        let item_size = data_vec.len();
                        let mut new_context =
                            ReadOnlyContext::new(data_vec.into_boxed_slice()).unwrap();
                        new_context.content.push(Some(DataSet {
                            ident: name.clone(),
                            buffers: vec![DataItem {
                                ident: item_name,
                                data: Position {
                                    offset: 0,
                                    size: item_size,
                                },
                                key: 0,
                            }],
                        }));
                        Arc::new(new_context)
                    })
                    .collect();
                let composition_set = CompositionSet::from((0, data_contexts));
                (name, Some(composition_set))
            } else {
                (name, None)
            }
        })
        .collect();

    Metadata {
        input_sets,
        output_sets: self.output_sets,
    }
}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterService {
    pub function_id: String, // how to refer to Compositions ? 
    pub prog_num: u32, 
    pub prog_ver: u32,
    pub proc_num: u32,
    pub listen_port: u16
}

#[derive(Debug, Deserialize)]
pub struct RegisterChain {
    pub composition: String,
}

#[derive(Serialize, Deserialize)]
pub struct DandelionRequest {
    pub name: String,
    pub sets: Vec<InputSet>,
}

#[derive(Serialize, Deserialize)]
pub struct InputSet {
    pub identifier: String,
    pub items: Vec<InputItem>,
}

#[derive(Serialize, Deserialize)]
pub struct InputItem {
    pub identifier: String,
    pub key: u32,
    pub data: Vec<u8>,
}
