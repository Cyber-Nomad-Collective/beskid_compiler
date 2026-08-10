//! Optional OpenTelemetry OTLP export via [`tracing-opentelemetry`].

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;

pub struct OtelGuard {
    provider: SdkTracerProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.provider.shutdown() {
            eprintln!("beskid telemetry: otel shutdown error: {err}");
        }
    }
}

pub fn otel_enabled() -> bool {
    if std::env::var("OTEL_SDK_DISABLED")
        .ok()
        .is_some_and(|value| matches!(value.to_lowercase().as_str(), "true" | "1" | "on"))
    {
        return false;
    }

    if let Some(explicit) = std::env::var("BESKID_TELEMETRY").ok().and_then(parse_bool) {
        return explicit;
    }

    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
}

fn parse_bool(value: String) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

pub fn install_otel_guard(service_name: &str) -> Result<OtelGuard, opentelemetry_otlp::ExporterBuildError> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4318".into());
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()?;
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_attributes([KeyValue::new("service.name", service_name.to_string())])
                .build(),
        )
        .build();
    Ok(OtelGuard { provider })
}

pub fn otel_tracer(guard: &OtelGuard) -> opentelemetry_sdk::trace::SdkTracer {
    guard.provider.tracer("beskid")
}
