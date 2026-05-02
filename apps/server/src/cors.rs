//! Build a `tower_http::CorsLayer` from a [`CorsPolicy`].
//!
//! Returns `None` when the policy is [`CorsPolicy::Disabled`] so the caller
//! can skip layering entirely - that yields the most locked-down posture
//! (no `Access-Control-Allow-Origin` header at all; the browser's
//! Same-Origin Policy is the only thing in play).

use axum::http::HeaderValue;
use edgeflow_config::CorsPolicy;
use tower_http::cors::{Any, CorsLayer};

pub fn build_layer(policy: &CorsPolicy) -> Option<CorsLayer> {
    match policy {
        CorsPolicy::Disabled => None,
        CorsPolicy::Any => {
            tracing::warn!(
                "CORS: any origin allowed (EDGEFLOW_CORS_ALLOW_ORIGINS=*) - intended for dev only"
            );
            Some(CorsLayer::permissive())
        }
        CorsPolicy::Allowlist(origins) => {
            let parsed: Vec<HeaderValue> = origins
                .iter()
                .filter_map(|o| HeaderValue::from_str(o).ok())
                .collect();
            tracing::info!(?origins, "CORS: allowlist active");
            Some(
                CorsLayer::new()
                    .allow_origin(parsed)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_yields_no_layer() {
        assert!(build_layer(&CorsPolicy::Disabled).is_none());
    }

    #[test]
    fn any_yields_a_layer() {
        assert!(build_layer(&CorsPolicy::Any).is_some());
    }

    #[test]
    fn allowlist_yields_a_layer() {
        let policy = CorsPolicy::Allowlist(vec!["https://example.com".into()]);
        assert!(build_layer(&policy).is_some());
    }

    #[test]
    fn allowlist_skips_invalid_header_values_without_panicking() {
        // `HeaderValue::from_str` rejects strings with control chars or
        // non-visible ASCII. The builder filters those out and still returns
        // a layer rather than panicking.
        let policy = CorsPolicy::Allowlist(vec!["https://ok.com".into(), "bad\norigin".into()]);
        assert!(build_layer(&policy).is_some());
    }

    #[test]
    fn allowlist_with_only_invalid_origins_still_yields_a_layer() {
        // Pathological input - all origins rejected. The layer still builds
        // (it just allows nothing); we don't fall back to permissive.
        let policy = CorsPolicy::Allowlist(vec!["bad\norigin".into()]);
        assert!(build_layer(&policy).is_some());
    }
}
