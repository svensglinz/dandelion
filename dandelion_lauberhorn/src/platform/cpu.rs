/// Check if hyperthreading appears to be enabled.
pub fn is_hyperthreading() -> bool {
    num_cpus::get_physical() < num_cpus::get()
}

pub fn get_phys_cores() -> usize {
    num_cpus::get_physical()
}

pub fn get_virt_cores() -> usize {
    num_cpus::get()
}