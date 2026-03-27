use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

/// Spawns a background task that listens for SIGINT (ctrl-c) and SIGTERM,
/// cancelling the returned [`CancellationToken`] on the first signal received.
pub fn shutdown_signal() -> CancellationToken {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT, shutting down");
            }
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM, shutting down");
            }
        }

        cancel_clone.cancel();
    });

    cancel
}
