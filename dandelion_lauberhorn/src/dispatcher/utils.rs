use std::sync::Arc;

use bytes::Bytes;
use machine_interface::{
    DataItem, DataSet, composition::CompositionSet, function_driver::{
        Metadata, functions::FunctionAlternative},
        machine_config::EngineType, memory_domain::{
        Context, ContextTrait, ContextType, bytes_context::BytesContext
    }
};

use crate::webserver::schemas::{InputItem, InputSet};

/// helper function that transforms a CompositionSet into an InputSet
pub fn comp_set_to_input_set(comp_set: &CompositionSet) -> InputSet {
    let mut items = Vec::new();
    let mut ident = String::new();

    for (key, item_idx, ctx) in comp_set {
        let data_set = ctx.content[comp_set.set_index].as_ref().unwrap();

        // identifier should be the same across all items
        if ident.is_empty() {
            ident = data_set.ident.clone();
        }
        let data_item = &data_set.buffers[item_idx];
        let pos = data_item.data;

        let bytes = ctx
            .context
            .get_chunk_ref(pos.offset, pos.size)
            .expect("failed to read item bytes");

        items.push(InputItem {
            identifier: data_item.ident.clone(),
            key: key as u32,
            data: bytes.to_vec(),
        });
    }
    InputSet { identifier: ident, items }
}

/// helper function to store the InputSets  into a Context
pub fn parse_req_ctx_from_input_sets(
    input: &Vec<InputSet>,
) -> Result<Context, ()> {
    let mut flat_data = Vec::new();
    let mut content = Vec::new();

    for set in input {
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

/// helper function that creates a Vec<Option<CompositionSet>> from a context
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

// TODO(@Sven): rename comp_sets_to_input_sets ?
pub fn comp_sets_to_input_sets(
    shard: &Vec<Option<CompositionSet>>,
) -> Vec<InputSet> {
    shard
        .iter()
        .map(|opt| match opt {
            Some(comp_set) => comp_set_to_input_set(comp_set),
            // TODO(@Sven): InputSet::empty(),
            None => InputSet { identifier: String::new(), items: vec![] },
        })
        .collect()
}

pub fn input_sets_to_comp_sets(
    sets: &Vec<InputSet>,
) -> Vec<Option<CompositionSet>> {
    let ctx = parse_req_ctx_from_input_sets(sets).unwrap();
    let n = ctx.content.len();
    let arc = Arc::new(ctx);
    (0..n)
        .map(|set_id| {
            arc.content[set_id]
                .as_ref()
                .map(|_| CompositionSet::from((set_id, vec![arc.clone()])))
        })
        .collect()
}

/// helper function to reduce shards
pub fn reduce_shards(
    shard_results: Vec<Vec<Option<CompositionSet>>>,
    output_mapping: &[Option<usize>],
) -> Vec<(usize, Option<CompositionSet>)> {
    let mut combined: Vec<Option<CompositionSet>> =
        vec![None; output_mapping.len()];

    for shard in shard_results {
        for (slot, set) in combined.iter_mut().zip(shard) {
            match (slot.as_mut(), set) {
                (Some(old), Some(new)) => {
                    old.combine(new).expect("combine failed")
                }
                (None, some) => *slot = some,
                _ => {}
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

/// Copy input sets from the incoming context into the isolation context.
pub fn transfer_input_sets(
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
pub fn get_function_variant(
    engine_type: EngineType,
    alternatives: &[Arc<FunctionAlternative>],
) -> Option<Arc<FunctionAlternative>> {
    alternatives.iter().find(|a| a.engine == engine_type).cloned()
}
