/// Trigger creation of an inference pod for `target`.
///
/// Full k8s integration (via the `kube` crate) can be wired in here.
/// For now we log the intent so the lifecycle still works in environments
/// where the pod is created manually or by an external operator.
pub async fn create_inference_pod(target: &str) {
    let server_addr = std::env::var("EDGEFLOW_ADDR")
        .unwrap_or_else(|_| "http://edgeflow-server:5000".into());
    let image = std::env::var("EDGEFLOW_INFERENCE_IMAGE")
        .unwrap_or_else(|_| "edgeflow-inference:latest".into());

    tracing::info!(
        target = %target,
        server = %server_addr,
        image = %image,
        "k8s pod creation requested — set EDGEFLOW_INFERENCE_IMAGE and ensure the pod \
         is launched with EDGEFLOW_SERVER and EDGEFLOW_TARGET env vars"
    );

    // TODO: wire in the `kube` crate here.
    // let client = kube::Client::try_default().await?;
    // create_deployment_resource(client, target, &server_addr, &image).await?;
}
