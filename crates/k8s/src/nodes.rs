//! List cluster nodes.

use k8s_openapi::api::core::v1::Node;
use kube::api::{Api, ListParams};

use crate::client::client;

/// List all node names in the cluster.
/// Returns an empty vec if the cluster is unreachable.
pub async fn list_nodes() -> Vec<String> {
    let Some(client) = client("list_nodes").await else {
        return vec![];
    };
    let api: Api<Node> = Api::all(client);
    match api.list(&ListParams::default()).await {
        Ok(list) => list
            .items
            .into_iter()
            .filter_map(|n| n.metadata.name)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to list k8s nodes");
            vec![]
        }
    }
}
