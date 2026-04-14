use machine_interface::machine_config::EngineType;

/// Get engine type from string
pub fn get_engine_type(name: &str) -> Result<EngineType, ()> {
    let engine_type = match name {
        #[cfg(feature = "mmu")]
        "Process" => EngineType::Process,
        #[cfg(feature = "kvm")]
        "Kvm" => EngineType::Kvm,
        #[cfg(feature = "cheri")]
        "Cheri" => EngineType::Cheri,
        _ => return Err(()),
    };
    Ok(engine_type)
}
