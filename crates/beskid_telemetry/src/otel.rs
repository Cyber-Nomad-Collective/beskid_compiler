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
    !matches!(
        std::env::var("OTEL_SDK_DISABLED").as_deref(),
        Ok("true") | Ok("1") | Ok("TRUE")
    ) && std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
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
