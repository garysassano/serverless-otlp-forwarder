use crate::events::{ParsedPlatformEvent, PlatformEventData, TelemetrySpan};
use chrono::{DateTime, Utc};
use lambda_extension::Status as LambdaStatus;
use opentelemetry::{
    trace::{SpanContext, SpanId, SpanKind, Status as OtelStatus, TraceFlags, TraceId, TraceState},
    InstrumentationScope, KeyValue,
};
use opentelemetry_sdk::trace::{SpanData, SpanEvents, SpanLinks};
use rand::Rng;
use std::time::{Duration as StdDuration, SystemTime};
use tracing;

#[derive(Debug)]
pub struct SpanAggregator {
    pub request_id: String,

    pub trace_id: Option<TraceId>,
    pub span_id: Option<SpanId>,
    pub parent_id: Option<SpanId>,
    pub trace_flags: TraceFlags,

    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
    pub status: OtelStatus,

    pub name: String,
    pub kind: SpanKind,
    pub attributes: Vec<KeyValue>,
    pub child_spans_data: Vec<SpanData>,

    pub received_event_types: Vec<String>,
    pub first_seen_timestamp: DateTime<Utc>,
    pub last_updated_timestamp: DateTime<Utc>,
}

impl SpanAggregator {
    pub fn new(request_id: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            request_id,
            trace_id: None,
            span_id: None,
            parent_id: None,
            trace_flags: TraceFlags::NOT_SAMPLED,
            start_time: None,
            end_time: None,
            status: OtelStatus::Unset,
            name: "Lambda Invoke".to_string(),
            kind: SpanKind::Server,
            attributes: Vec::new(),
            child_spans_data: Vec::new(),
            received_event_types: Vec::new(),
            first_seen_timestamp: timestamp,
            last_updated_timestamp: timestamp,
        }
    }

    /// Updates the aggregator state based on a received platform event.
    pub fn update_from_event(&mut self, event: &ParsedPlatformEvent) {
        self.last_updated_timestamp = event.timestamp;
        let event_type_str = match &event.data {
            PlatformEventData::Start { .. } => "platform.start",
            PlatformEventData::RuntimeDone { .. } => "platform.runtimeDone",
            PlatformEventData::Report { .. } => "platform.report",
        };
        self.received_event_types.push(event_type_str.to_string());

        match &event.data {
            PlatformEventData::Start {
                version,
                trace_context,
            } => {
                if self.start_time.is_none() {
                    self.start_time = Some(event.timestamp.into());
                }
                if let Some(v) = version {
                    self.attributes
                        .push(KeyValue::new("faas.instance", v.clone()));
                }
                if self.trace_id.is_none() {
                    if let Some(tc) = trace_context {
                        self.trace_id = Some(tc.trace_id);
                        self.parent_id = tc.parent_id;
                        self.trace_flags = if tc.sampled {
                            TraceFlags::SAMPLED
                        } else {
                            TraceFlags::NOT_SAMPLED
                        };
                        if self.span_id.is_none() {
                            self.span_id = tc.platform_span_id;
                        }
                    }
                }
            }
            PlatformEventData::RuntimeDone {
                status,
                error_type,
                metrics,
                spans,
                trace_context,
            } => {
                if self.status == OtelStatus::Unset {
                    self.set_otel_status(status.clone(), error_type.as_deref());
                }
                for (k, v) in metrics {
                    self.attributes
                        .push(KeyValue::new(format!("lambda.runtime.{}", k), v.clone()));
                }
                self.add_child_spans(spans);

                if self.trace_id.is_none() {
                    if let Some(tc) = trace_context {
                        self.trace_id = Some(tc.trace_id);
                        self.parent_id = tc.parent_id;
                        self.trace_flags = if tc.sampled {
                            TraceFlags::SAMPLED
                        } else {
                            TraceFlags::NOT_SAMPLED
                        };
                        if self.span_id.is_none() {
                            self.span_id = tc.platform_span_id;
                        }
                    }
                }
            }
            PlatformEventData::Report {
                status,
                error_type,
                metrics,
                spans,
                trace_context,
            } => {
                self.end_time = Some(event.timestamp.into());
                self.set_otel_status(status.clone(), error_type.as_deref());
                for (k, v) in metrics {
                    self.attributes
                        .push(KeyValue::new(format!("lambda.report.{}", k), v.clone()));
                }
                self.attributes
                    .push(KeyValue::new("faas.execution", self.request_id.clone()));
                self.add_child_spans(spans);

                if self.trace_id.is_none() {
                    if let Some(tc) = trace_context {
                        self.trace_id = Some(tc.trace_id);
                        self.parent_id = tc.parent_id;
                        self.trace_flags = if tc.sampled {
                            TraceFlags::SAMPLED
                        } else {
                            TraceFlags::NOT_SAMPLED
                        };
                        if self.span_id.is_none() {
                            self.span_id = tc.platform_span_id;
                        }
                    }
                }
            }
        }
    }

    pub fn is_complete(&self) -> bool {
        self.received_event_types
            .contains(&"platform.runtimeDone".to_string())
            && self
                .received_event_types
                .contains(&"platform.report".to_string())
    }

    pub fn to_otel_span_data(&self) -> Option<SpanData> {
        let trace_id = self.trace_id?;
        let span_id = self.span_id?;
        let start_time = self.start_time?;
        let end_time = self
            .end_time
            .unwrap_or_else(|| self.last_updated_timestamp.into());

        let span_context = SpanContext::new(
            trace_id,
            span_id,
            self.trace_flags,
            false,
            TraceState::default(),
        );

        Some(SpanData {
            span_context,
            parent_span_id: self.parent_id.unwrap_or_else(|| SpanId::from_bytes([0; 8])),
            span_kind: self.kind.clone(),
            name: self.name.clone().into(),
            start_time,
            end_time,
            attributes: self.attributes.clone(),
            events: SpanEvents::default(),
            links: SpanLinks::default(),
            status: self.status.clone(),
            dropped_attributes_count: 0,
            instrumentation_scope: InstrumentationScope::default(),
        })
    }

    fn set_otel_status(&mut self, lambda_status: LambdaStatus, error_type: Option<&str>) {
        match lambda_status {
            LambdaStatus::Success => {
                if matches!(self.status, OtelStatus::Unset) {
                    self.status = OtelStatus::Ok;
                }
            }
            LambdaStatus::Failure | LambdaStatus::Error | LambdaStatus::Timeout => {
                self.status = OtelStatus::Error {
                    description: error_type
                        .unwrap_or("Lambda platform error")
                        .to_string()
                        .into(),
                };
            }
        }
    }

    fn add_child_spans(&mut self, spans: &[TelemetrySpan]) {
        // Check for trace_id first
        let trace_id = match self.trace_id {
            Some(id) => id,
            None => {
                tracing::warn!(
                    "Cannot create child spans without a trace ID for aggregate invoke span."
                );
                return; // Cannot proceed without trace_id
            }
        };

        // Check for parent_span_id (this aggregate span's ID)
        let parent_span_id = match self.span_id {
            Some(id) => id,
            None => {
                // This case should be less likely as we generate span_id in `new`
                tracing::error!(
                    "Cannot create child spans without a parent span ID for aggregate span."
                );
                return;
            }
        };

        let mut rng = rand::rng();

        for span in spans {
            let child_start_time: SystemTime = span.start.into();
            let child_duration = StdDuration::from_secs_f64(span.duration_ms / 1000.0);
            let child_end_time = child_start_time + child_duration;
            let child_span_id = SpanId::from_bytes(rng.random::<[u8; 8]>());

            let child_span_context = SpanContext::new(
                trace_id,
                child_span_id,
                self.trace_flags,
                false,
                TraceState::default(),
            );

            let child_span_data = SpanData {
                span_context: child_span_context,
                parent_span_id,
                span_kind: SpanKind::Internal,
                name: span.name.clone().into(),
                start_time: child_start_time,
                end_time: child_end_time,
                attributes: vec![],
                events: SpanEvents::default(),
                links: SpanLinks::default(),
                status: OtelStatus::Unset,
                dropped_attributes_count: 0,
                instrumentation_scope: InstrumentationScope::default(),
            };
            self.child_spans_data.push(child_span_data);
        }
    }
}
