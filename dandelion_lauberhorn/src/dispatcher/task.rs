use std::collections::{BTreeSet, HashMap};

use dandelion_commons::FunctionId;
use machine_interface::composition::{
    CompositionSet, FunctionDependencies, InputSetDescriptor, JoinStrategy,
    ShardingMode,
};

/// Task represents a single node (function) in a composition graph
/// Each task keeps track of its required input dependencies,
/// (optional or required)
#[derive(Clone)]
pub struct Task {
    // node name
    pub function_id: FunctionId,
    // required inputs for this function to run
    pub inputs: Vec<Option<(ShardingMode, CompositionSet)>>,
    // still missing inputs
    pub missing: BTreeSet<usize>,
    pub id_to_slots: HashMap<usize, Vec<(usize, ShardingMode)>>,
    // missing optional inputs
    pub missing_optional: BTreeSet<usize>,
    pub join_info: (Vec<usize>, Vec<JoinStrategy>),
    // output set ids ??
    pub output_set_ids: Vec<Option<usize>>,
}

impl Task {
    /// create a new task from a function dependency in a Composition
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

    /// provides an input set to the task, updating the missing dependencies accordingly
    pub fn provide_input(
        &mut self,
        composition_id: usize,
        set: Option<CompositionSet>,
    ) -> bool {
        let is_empty = match set {
            None => true,
            Some(ref s) => s.is_empty(),
        };
        let is_required = self.missing.contains(&composition_id);

        // CHECK LOGIC AGIAN
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
