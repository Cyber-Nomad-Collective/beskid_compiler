//! Optional OpenTelemetry OTLP export via [`tracing-opentelemetry`].

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;

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

/// OTLP export is opt-in: only `BESKID_TELEMETRY=1` (or `true`/`on`/`yes`) turns it on. The generic
/// `OTEL_EXPORTER_OTLP_ENDPOINT` is often set for other tools and span fields carry project
/// paths, so it alone never enables export; it only names the endpoint once export is on.
/// `OTEL_SDK_DISABLED` still wins.
pub fn otel_enabled() -> bool {
    otel_enabled_from(std::env::var("OTEL_SDK_DISABLED").ok(), std::env::var("BESKID_TELEMETRY").ok())
}

fn otel_enabled_from(sdk_disabled: Option<String>, telemetry: Option<String>) -> bool {
    if sdk_disabled.is_some_and(|value| matches!(value.to_lowercase().as_str(), "true" | "1" | "on")) {
        return false;
    }
    telemetry.and_then(parse_bool).unwrap_or(false)
}

fn parse_bool(value: String) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

pub fn install_otel_guard(service_name: &str) -> Result<OtelGuard, opentelemetry_otlp::ExporterBuildError> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".into());
    let exporter = opentelemetry_otlp::SpanExporter::builder().with_http().with_endpoint(endpoint).build()?;
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder().with_attributes([KeyValue::new("service.name", service_name.to_string())]).build(),
        )
        .build();
    Ok(OtelGuard { provider })
}

pub fn otel_tracer(guard: &OtelGuard) -> opentelemetry_sdk::trace::SdkTracer {
    guard.provider.tracer("beskid")
}

#[cfg(test)]
mod tests {
    use super::otel_enabled_from;

    fn some(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    #[test]
    fn export_is_off_unless_beskid_telemetry_opts_in() {
        assert!(!otel_enabled_from(None, None), "no variable: no export, no network traffic");
        assert!(otel_enabled_from(None, some("1")));
        assert!(otel_enabled_from(None, some("true")));
        assert!(!otel_enabled_from(None, some("0")));
        assert!(!otel_enabled_from(None, some("banana")));
    }

    #[test]
    fn sdk_disabled_wins_over_the_opt_in() {
        assert!(!otel_enabled_from(some("true"), some("1")));
        assert!(otel_enabled_from(some("false"), some("1")));
    }
}
