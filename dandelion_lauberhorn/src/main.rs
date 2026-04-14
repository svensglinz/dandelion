use dandelion_lauberhorn::platform;
use dandelion_lauberhorn::runtime::create_runtime;
use log::{error, info, warn};
use std::sync::Arc;

use dandelion_lauberhorn::system;
use dandelion_lauberhorn::utils;
use dandelion_lauberhorn::webserver;

// ---------------------------------------------------------------------------
// Initialization helpers
// ---------------------------------------------------------------------------

/// main entry point
fn main() {
    utils::logging::init_logging();

    system::init_tracing_archive();
    system::init_memory_pool();

    let config = dandelion_server::config::DandelionConfig::get_config();
    info!("Loaded configuration:\n{:?}", config);

    if platform::cpu::is_hyperthreading() {
        warn!(
            "Hyperthreading might be enabled — {} logical / {} physical cores",
            platform::cpu::get_virt_cores(),
            platform::cpu::get_phys_cores(),
        );
    }

    let frontend_cores = config.get_frontend_cores();
    let tokio_runtime = system::build_frontend_runtime(frontend_cores);

    let memory_pool = system::init_memory_pool();

    // creating runtime
    info!("Creating Runtime with lauberhorn backend");
    let mut runtime = match create_runtime(memory_pool) {
        Ok(rt) => {
            rt // hack... should transform to arc already
               // here and solve mutability issues in runtime instead of here
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

    let features = system::get_configured_features();
    info!("Supported features: {:?}", features);

    tokio_runtime
        .block_on(webserver::service_loop(Arc::new(runtime), config.port));

    info!("Shutting down...");
    system::cleanup();
}
