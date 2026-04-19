use std::fmt;

use opentelemetry::trace::TraceContextExt;
use serde_json::{Map, Value};
use tracing::{Event, Subscriber};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::fmt::format::{self, FormatEvent};
use tracing_subscriber::fmt::{FmtContext, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

/// JSON event formatter that injects OTel `trace_id` and `span_id` into every
/// log line, making logs correlatable with traces in Grafana / Tempo.
pub(crate) struct JsonWithTraceId;

impl<S, N> FormatEvent<S, N> for JsonWithTraceId
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut map = Map::new();

        map.insert(
            "timestamp".into(),
            Value::from(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)),
        );
        map.insert("level".into(), Value::from(meta.level().as_str()));

        // Inject OTel trace context when inside an instrumented span.
        let span = tracing::Span::current();
        let otel_ctx = span.context();
        let span_ctx = otel_ctx.span().span_context().clone();
        if span_ctx.is_valid() {
            map.insert(
                "trace_id".into(),
                Value::from(span_ctx.trace_id().to_string()),
            );
            map.insert(
                "span_id".into(),
                Value::from(span_ctx.span_id().to_string()),
            );
        }

        let mut visitor = FieldVisitor(Map::new());
        event.record(&mut visitor);
        map.insert("fields".into(), Value::Object(visitor.0));
        map.insert("target".into(), Value::from(meta.target()));

        // Span stack - walk leaf-to-root then reverse so output is root-to-leaf.
        let mut spans = Vec::new();
        ctx.visit_spans(|span_ref| {
            let mut span_obj = Map::new();
            span_obj.insert("name".into(), Value::from(span_ref.metadata().name()));
            let ext = span_ref.extensions();
            if let Some(fields) = ext.get::<FormattedFields<N>>() {
                let s = fields.to_string();
                if !s.is_empty() {
                    if let Ok(Value::Object(parsed)) =
                        serde_json::from_str::<Value>(&format!("{{{s}}}"))
                    {
                        span_obj.extend(parsed);
                    }
                }
            }
            drop(ext);
            spans.push(Value::Object(span_obj));
            Ok(())
        })?;
        spans.reverse();

        if let Some(current) = spans.last().cloned() {
            map.insert("span".into(), current);
        }
        if !spans.is_empty() {
            map.insert("spans".into(), Value::Array(spans));
        }

        let json = serde_json::to_string(&Value::Object(map)).map_err(|_| fmt::Error)?;
        writeln!(writer, "{json}")
    }
}

struct FieldVisitor(Map<String, Value>);

impl tracing::field::Visit for FieldVisitor {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.insert(field.name().into(), Value::from(value));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.insert(field.name().into(), Value::from(value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().into(), Value::from(value));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.insert(field.name().into(), Value::from(value));
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().into(), Value::from(value));
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().into(), Value::from(format!("{value:?}")));
    }
}
