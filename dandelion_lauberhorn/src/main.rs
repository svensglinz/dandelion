
use core::error;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use dandelion_lauberhorn::platform;
use core_affinity::{self, CoreId};
use dandelion_commons::records::Archive;
use dandelion_lauberhorn::runtime::create_runtime;
use dandelion_lauberhorn::webserver::web_server::{FUNCTION_FOLDER_PATH, TRACING_ARCHIVE, service_loop};
use log::{info, warn, error, debug};
use machine_interface::machine_config::DomainType;
use machine_interface::memory_domain::MemoryResource;
use tokio::runtime::Builder;

// ---------------------------------------------------------------------------
// Initialization helpers
// ---------------------------------------------------------------------------

/// initialize logging with env_logger, default level is warn, debug in debug builds
fn init_logging() {
    let default_warn_level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "warn"
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_warn_level))
        .init();
}

fn build_frontend_runtime(frontend_cores: Vec<u8>) -> tokio::runtime::Runtime {
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

fn init_memory_pool() -> BTreeMap<DomainType, MemoryResource> {
    let max_ram = platform::memory::get_max_ram().unwrap();

    BTreeMap::from([
        #[cfg(feature = "cheri")]
        (DomainType::Cheri, MemoryResource::Anonymous { size: max_ram }),
        #[cfg(feature = "kvm")]
        (DomainType::Kvm, MemoryResource::Anonymous { size: max_ram }),
        #[cfg(feature = "mmu")]
        (DomainType::Process, MemoryResource::Shared { id: 0, size: max_ram }),
    ])
}

fn get_configured_feature() -> String {
    let mut features = Vec::new();
    #[cfg(feature = "cheri")]
    features.push("cheri");
    #[cfg(feature = "mmu")]
    features.push("mmu");
    #[cfg(feature = "kvm")]
    features.push("kvm");
    features.join(", ")
}

fn print_features() {
    print!("Server start with features:");
    #[cfg(feature = "cheri")]
    print!(" cheri");
    #[cfg(feature = "mmu")]
    print!(" mmu");
    #[cfg(feature = "kvm")]
    print!(" kvm");
    println!();
}

// ensure thsi is run even with ctrl+c etc... -> or again on startup ??
fn cleanup() {
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

/// main entry point
fn main() {
    init_logging();

    let config = dandelion_server::config::DandelionConfig::get_config();
    info!("Loaded configuration:\n{:?}", config);

    TRACING_ARCHIVE
        .set(Archive::init())
        .map_err(|e| {
            error!("Failed to initialize tracing archive");
            std::process::exit(1);
        });

    if platform::cpu::is_hyperthreading() {
        warn!(
            "Hyperthreading might be enabled — {} logical / {} physical cores",
            platform::cpu::get_virt_cores(),
            platform::cpu::get_phys_cores(),
        );
    }

    let frontend_cores = config.get_frontend_cores();
    let tokio_runtime = build_frontend_runtime(frontend_cores);

    let memory_pool = init_memory_pool();
    
    // creating runtime 
    info!("Creating Runtime with lauberhorn backend");
    let mut runtime = match create_runtime(memory_pool) {
        Ok(rt) => {
            rt // hack... should transform to arc already here and solve mutability issues in runtime instead of here
        }
        Err(e) => {
            error!("Failed to create runtime: {}", e);
            std::process::exit(1)
        }
    };
    info!("Starting runtime");
    runtime.run().unwrap(); 

    info!("Starting frontend HTTP server on port {}", config.port);
    let _guard = tokio_runtime.enter();
    print_features();
    tokio_runtime.block_on(service_loop(Arc::new(runtime), config.port));
}