use tracing::subscriber::DefaultGuard;
use tracing_log::LogTracer;

/// Utility function to initialize logging in the test environment.
/// Note that you have to keep the `_guard` in scope after calling in test:
///
/// ```rust
/// let _guard = init_tracing();
/// ```
pub fn init_tracing() -> DefaultGuard {
    // converts all log records into tracing events
    // Note: Make sure to initialize without unwrapping, otherwise this causes
    // trouble when running multiple tests.
    let _ = LogTracer::init();

    let global_filter = tracing::Level::WARN;
    let test_filter = tracing::Level::DEBUG;
    let jude_harness_filter = tracing::Level::DEBUG;
    let jude_rpc_filter = tracing::Level::DEBUG;

    use tracing_subscriber::util::SubscriberInitExt as _;
    tracing_subscriber::fmt()
        .with_env_filter(format!(
            "{},test={},jude_harness={},jude_rpc={}",
            global_filter, test_filter, jude_harness_filter, jude_rpc_filter,
        ))
        .set_default()
}
