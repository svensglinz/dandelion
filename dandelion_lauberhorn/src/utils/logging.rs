/// initialize logging with env_logger, default level is warn, debug in debug builds
pub fn init_logging() {
    let default_warn_level =
        if cfg!(debug_assertions) { "debug" } else { "info" };
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(default_warn_level),
    )
    .init();
}
