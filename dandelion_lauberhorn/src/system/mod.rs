use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};

use core_affinity::CoreId;
use dandelion_commons::records::Archive;
use log::{error, info, warn};
use machine_interface::machine_config::DomainType;
use machine_interface::memory_domain::MemoryResource;
use tokio::runtime::Builder;

// constants
pub static TRACING_ARCHIVE: OnceLock<Archive> = OnceLock::new();
pub const FUNCTION_FOLDER_PATH: &str = "/tmp/dandelion_server";

pub fn init_tracing_archive() {
    TRACING_ARCHIVE.set(Archive::init()).map_err(|_: Archive| {
        error!("Failed to initialize tracing archive");
        std::process::exit(1);
    });
}

pub fn init_memory_pool() -> BTreeMap<DomainType, MemoryResource> {
    let max_ram = crate::platform::memory::get_max_ram().unwrap();

    BTreeMap::from([
        #[cfg(feature = "cheri")]
        (DomainType::Cheri, MemoryResource::Anonymous { size: max_ram }),
        #[cfg(feature = "kvm")]
        (DomainType::Kvm, MemoryResource::Anonymous { size: max_ram }),
        #[cfg(feature = "mmu")]
        (DomainType::Process, MemoryResource::Shared { id: 0, size: max_ram }),
    ])
}

pub fn build_frontend_runtime(frontend_cores: Vec<u8>) -> tokio::runtime::Runtime {
    let mut builder = Builder::new_multi_thread();
    builder.enable_io();
    builder.worker_threads(frontend_cores.len());
    builder.on_thread_start(move || {
        static ATOMIC_INDEX: AtomicUsize = AtomicUsize::new(0);
        let core_index = ATOMIC_INDEX.fetch_add(1, Ordering::SeqCst);
        if !core_affinity::set_for_current(CoreId {
            id: frontend_cores[core_index].into(),
        }) {
            return;
        }
        info!("Frontend thread running on core {}", frontend_cores[core_index]);
    });
    builder.global_queue_interval(10);
    builder.event_interval(10);
    builder.build().unwrap()
}

pub fn get_configured_features() -> String {
    let mut features = Vec::new();
    #[cfg(feature = "cheri")]
    features.push("cheri");
    #[cfg(feature = "mmu")]
    features.push("mmu");
    #[cfg(feature = "kvm")]
    features.push("kvm");
    features.join(", ")
}

pub fn cleanup() {
    if let Err(err) = std::fs::remove_dir_all(FUNCTION_FOLDER_PATH) {
        warn!("Removing function folder failed with: {}", err);
    }
    for entry in std::fs::read_dir("/dev/shm/").unwrap().flatten() {
        if entry.file_name().to_string_lossy().starts_with("shm_") {
            warn!("Found leftover shared memory file: {:?}", entry.file_name());
            if std::fs::remove_file(entry.path()).is_err() {
                warn!("Failed to remove shared memory file {:?}", entry.path());
            }
        }
    }
}
