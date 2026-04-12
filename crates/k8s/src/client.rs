//! Shared k8s client + namespace helpers.
//!
//! Every public function in this crate needs a `kube::Client` and the target
//! namespace, with the same "warn and degrade gracefully" behavior when the
//! cluster is unreachable. These helpers centralize that boilerplate.

use kube::Client;

/// Resolve the k8s namespace from `EDGEFLOW_NAMESPACE`, defaulting to `"default"`.
pub(crate) fn namespace() -> String {
    std::env::var("EDGEFLOW_NAMESPACE").unwrap_or_else(|_| "default".into())
}

/// Build a kube `Client`, logging a warning and returning `None` when the
/// cluster is unreachable. `context` is included in the log so the caller is
/// identifiable in operator logs.
pub(crate) async fn client(context: &str) -> Option<Client> {
    Client::try_default()
        .await
        .map_err(|e| tracing::warn!(error = %e, context, "k8s client unavailable"))
        .ok()
}
