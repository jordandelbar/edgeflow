pub mod backoff;
mod shutdown;
pub mod tensor;

pub use shutdown::shutdown_signal;
pub use tokio_util::sync::CancellationToken;

/// Parse `"mqtt://host:port"` (or bare `"host:port"`) into `(host, port)`.
pub fn parse_broker_addr(url: &str) -> (String, u16) {
    let stripped = url
        .trim_start_matches("mqtt://")
        .trim_start_matches("tcp://");
    if let Some((host, port_str)) = stripped.rsplit_once(':') {
        let port = port_str.parse().unwrap_or(1883);
        (host.to_string(), port)
    } else {
        (stripped.to_string(), 1883)
    }
}
