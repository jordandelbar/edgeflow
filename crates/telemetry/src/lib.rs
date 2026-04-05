//! OpenTelemetry setup for edgeflow services.
//!
//! Call [`init`] once at service startup, before any other work. It sets up:
//! - A `tracing` subscriber with `EnvFilter` (reads `RUST_LOG`, falls back to
//!   `default_filter`)
//! - JSON log format in production (`EDGEFLOW_ENV=production`), pretty-print
//!   otherwise — both formats inject OTel `trace_id`/`span_id` when available
//! - OTLP span exporter for traces (best-effort; degrades gracefully when the
//!   collector is unreachable)
//! - OTLP metrics exporter with a 10 s periodic reader
//!
//! The OTLP endpoint is read from the standard `OTEL_EXPORTER_OTLP_ENDPOINT`
//! variable. If it is unset or the collector is unreachable, the pod continues
//! to run with stdout-only observability.

mod json_format;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use edgeflow_config::Environment;

/// Initialise tracing and metrics for the given service.
///
/// `default_filter` is used when `RUST_LOG` is not set (e.g.
/// `"edgeflow_inference=info"`).
pub fn init(service_name: &str, default_filter: &str) -> anyhow::Result<()> {
    let env = Environment::from_env();
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let otel_provider = build_tracer_provider(service_name).ok();
    let has_otel = otel_provider.is_some();

    let otel_layer = otel_provider.map(|provider| {
        // Register globally — this keeps the provider (and its batch exporter)
        // alive for the process lifetime. Without this the provider is dropped
        // at the end of the closure and the batch exporter shuts down immediately.
        opentelemetry::global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer(service_name.to_owned());
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    let fmt_layer = if env.is_production() {
        tracing_subscriber::fmt::layer()
            .event_format(json_format::JsonWithTraceId)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer().pretty().boxed()
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    if let Ok(meter_provider) = build_meter_provider(service_name) {
        opentelemetry::global::set_meter_provider(meter_provider);
    }

    if has_otel {
        tracing::info!("telemetry initialised — OTLP exporter active");
    } else {
        tracing::warn!("OTLP collector unreachable — stdout only");
    }

    Ok(())
}

fn build_tracer_provider(service_name: &str) -> anyhow::Result<SdkTracerProvider> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name.to_owned())
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    Ok(provider)
}

fn build_meter_provider(service_name: &str) -> anyhow::Result<SdkMeterProvider> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .build()?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(10))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name.to_owned())
                .build(),
        )
        .build();

    Ok(provider)
}
