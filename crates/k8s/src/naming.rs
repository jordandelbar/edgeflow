//! k8s resource naming and affinity helpers shared by deployments and pods.

use edgeflow_core::Placement;
use k8s_openapi::api::core::v1::{
    Affinity, PodAffinity, PodAffinityTerm, PodAntiAffinity, WeightedPodAffinityTerm,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;

/// Sanitize a target name into a valid k8s resource name.
/// k8s names: lowercase alphanumeric + `-`, max 63 chars.
pub(crate) fn k8s_name(target: &str) -> String {
    let sanitized: String = target
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    format!(
        "edgeflow-inference-{}",
        &sanitized[..sanitized.len().min(45)]
    )
}

pub(crate) fn label_selector(target: &str) -> LabelSelector {
    LabelSelector {
        match_labels: Some([("edgeflow-target".to_string(), target.to_string())].into()),
        ..Default::default()
    }
}

fn affinity_term(target: &str) -> PodAffinityTerm {
    PodAffinityTerm {
        label_selector: Some(label_selector(target)),
        topology_key: "kubernetes.io/hostname".to_string(),
        ..Default::default()
    }
}

/// Anti-affinity: prefer scheduling each replica on a different node.
fn spread_affinity(target: &str) -> Affinity {
    Affinity {
        pod_anti_affinity: Some(PodAntiAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(vec![
                WeightedPodAffinityTerm {
                    weight: 100,
                    pod_affinity_term: affinity_term(target),
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Affinity: prefer scheduling all replicas on the same node.
fn pack_affinity(target: &str) -> Affinity {
    Affinity {
        pod_affinity: Some(PodAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(vec![
                WeightedPodAffinityTerm {
                    weight: 100,
                    pod_affinity_term: affinity_term(target),
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub(crate) fn placement_affinity(placement: &Placement, target: &str) -> Affinity {
    match placement {
        Placement::Spread => spread_affinity(target),
        Placement::Pack => pack_affinity(target),
    }
}
