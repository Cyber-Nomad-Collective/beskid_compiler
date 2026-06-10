//! In-memory trace ring buffer and [`tracing_subscriber::Layer`] for hi developer tooling.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing::{Event, Id, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::{LookupSpan, SpanRef};
use tracing_subscriber::Layer;

const DEFAULT_CAPACITY: usize = 4_096;

static GLOBAL_BUFFER: OnceLock<Arc<TelemetryBuffer>> = OnceLock::new();

/// Shared handle to the process-wide telemetry ring buffer.
pub fn telemetry_buffer() -> Arc<TelemetryBuffer> {
    GLOBAL_BUFFER
        .get_or_init(|| Arc::new(TelemetryBuffer::new(DEFAULT_CAPACITY)))
        .clone()
}

/// One completed span captured for the developer trace widget.
#[derive(Debug, Clone)]
pub struct TelemetrySpan {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub target: String,
    pub level: Level,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub fields: Vec<(String, String)>,
}

/// One log-style event (inside a span or at root).
#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub at_ms: u64,
    pub level: Level,
    pub target: String,
    pub message: String,
    pub span_id: Option<u64>,
    pub fields: Vec<(String, String)>,
}

/// Point-in-time view of buffered spans and events.
#[derive(Debug, Clone, Default)]
pub struct TelemetrySnapshot {
    pub spans: Vec<TelemetrySpan>,
    pub events: Vec<TelemetryEvent>,
}

struct ActiveSpan {
    id: u64,
    parent_id: Option<u64>,
    name: String,
    target: String,
    level: Level,
    started_at_ms: u64,
    fields: Vec<(String, String)>,
}

struct BufferState {
    next_id: u64,
    id_map: Vec<(Id, u64)>,
    active: Vec<ActiveSpan>,
    spans: VecDeque<TelemetrySpan>,
    events: VecDeque<TelemetryEvent>,
}

struct FieldVisitor {
    fields: Vec<(String, String)>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self { fields: Vec::new() }
    }

    fn span_name(metadata_name: &str, fields: &[(String, String)]) -> String {
        fields
            .iter()
            .find(|(k, _)| k == "message" || k == "name")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| metadata_name.to_string())
    }

    fn event_message(metadata_name: &str, fields: &[(String, String)]) -> String {
        fields
            .iter()
            .find(|(k, _)| k == "message")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| metadata_name.to_string())
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Thread-safe ring buffer of spans and events for developer UI.
pub struct TelemetryBuffer {
    inner: Mutex<BufferState>,
    capacity: usize,
}

impl TelemetryBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(BufferState {
                next_id: 1,
                id_map: Vec::new(),
                active: Vec::new(),
                spans: VecDeque::new(),
                events: VecDeque::new(),
            }),
            capacity,
        }
    }

    pub fn clear(&self) {
        let mut state = self.inner.lock().expect("telemetry buffer");
        state.id_map.clear();
        state.active.clear();
        state.spans.clear();
        state.events.clear();
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        let state = self.inner.lock().expect("telemetry buffer");
        TelemetrySnapshot {
            spans: state.spans.iter().cloned().collect(),
            events: state.events.iter().cloned().collect(),
        }
    }

    fn lookup_numeric_id(state: &BufferState, span_id: &Id) -> Option<u64> {
        state
            .id_map
            .iter()
            .find(|(id, _)| id == span_id)
            .map(|(_, n)| *n)
    }

    fn on_new_span<S>(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: &Context<'_, S>)
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let mut visitor = FieldVisitor::new();
        attrs.record(&mut visitor);
        let metadata = attrs.metadata();
        let mut state = self.inner.lock().expect("telemetry buffer");
        let numeric_id = state.next_id;
        state.next_id += 1;
        if let Some(span) = ctx.span(id) {
            state.id_map.push((span.id().clone(), numeric_id));
        }
        let parent_id = state.active.last().map(|s| s.id);
        state.active.push(ActiveSpan {
            id: numeric_id,
            parent_id,
            name: FieldVisitor::span_name(metadata.name(), &visitor.fields),
            target: metadata.target().to_string(),
            level: *metadata.level(),
            started_at_ms: now_ms(),
            fields: visitor.fields,
        });
    }

    fn on_close<S>(&self, id: Id, ctx: Context<'_, S>)
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let mut state = self.inner.lock().expect("telemetry buffer");
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let registry_id = span.id().clone();
        let Some(numeric_id) = Self::lookup_numeric_id(&state, &registry_id) else {
            return;
        };
        state.id_map.retain(|(sid, _)| sid != &registry_id);
        let Some(index) = state.active.iter().position(|s| s.id == numeric_id) else {
            return;
        };
        let active = state.active.remove(index);
        state.spans.push_back(TelemetrySpan {
            id: active.id,
            parent_id: active.parent_id,
            name: active.name,
            target: active.target,
            level: active.level,
            started_at_ms: active.started_at_ms,
            ended_at_ms: Some(now_ms()),
            fields: active.fields,
        });
        while state.spans.len() > self.capacity {
            state.spans.pop_front();
        }
    }

    fn on_event<S>(&self, event: &Event<'_>, ctx: &Context<'_, S>)
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);
        let span_id = event_span_numeric_id(&self.inner, ctx, event);
        let mut state = self.inner.lock().expect("telemetry buffer");
        state.events.push_back(TelemetryEvent {
            at_ms: now_ms(),
            level: *metadata.level(),
            target: metadata.target().to_string(),
            message: FieldVisitor::event_message(metadata.name(), &visitor.fields),
            span_id,
            fields: visitor.fields,
        });
        while state.events.len() > self.capacity {
            state.events.pop_front();
        }
    }
}

fn event_span_numeric_id<S>(
    inner: &Mutex<BufferState>,
    ctx: &Context<'_, S>,
    event: &Event<'_>,
) -> Option<u64>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    let span: SpanRef<'_, S> = ctx.event_span(event)?;
    let state = inner.lock().expect("telemetry buffer");
    let registry_id = span.id();
    TelemetryBuffer::lookup_numeric_id(&state, &registry_id)
}

/// [`tracing_subscriber::Layer`] that records spans and events into a [`TelemetryBuffer`].
pub struct BufferLayer {
    buffer: Arc<TelemetryBuffer>,
}

impl BufferLayer {
    pub fn new(buffer: Arc<TelemetryBuffer>) -> Self {
        Self { buffer }
    }

    pub fn global() -> Self {
        Self::new(telemetry_buffer())
    }
}

impl<S> Layer<S> for BufferLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        self.buffer.on_new_span(attrs, id, &ctx);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        self.buffer.on_close(id, ctx);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        self.buffer.on_event(event, &ctx);
    }
}
