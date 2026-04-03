use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialise the global tracing subscriber.
///
/// Log level — first match wins:
///   1. `RUST_LOG` env var (standard override for developers)
///   2. `default_filter` argument (caller provides a sensible per-service default)
///
/// Format — controlled by `LOG_FORMAT` env var:
///   unset / "text" → human-readable (good for terminals and `dev.sh`)
///   "json"         → JSON lines (good for k8s log aggregators)
pub fn init_logging(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init();
    }
}
