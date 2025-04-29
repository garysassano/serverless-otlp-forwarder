use aws_sdk_kinesis::Client as KinesisClient;
use chrono::{Duration, Utc};
use lambda_extension::{
    service_fn, tracing, Error, Extension, LambdaEvent, LambdaTelemetry, LambdaTelemetryRecord,
    LogBuffering, NextEvent, SharedService,
};
use opentelemetry::{trace::SpanId, Value as OtelValue};
use opentelemetry_sdk::trace::{SpanData, SpanExporter};

use lambda_otel_lite::resource::get_lambda_resource;
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// Add the modules
mod aggregation;
mod config;
mod events;
mod kinesis;
mod xray;

// Use the types from the modules
use aggregation::SpanAggregator;
use config::Config;
use events::{ParsedPlatformEvent, PlatformEventData, TelemetrySpan};
use kinesis::{KinesisBatch, RECORD_PREFIX};

// Application state
struct AppState {
    kinesis_client: KinesisClient,
    stream_name: String,
    batch: Mutex<KinesisBatch>,
    platform_event_tx: mpsc::Sender<ParsedPlatformEvent>,
    platform_event_rx: Arc<Mutex<mpsc::Receiver<ParsedPlatformEvent>>>,
    aggregations: Mutex<HashMap<String, SpanAggregator>>,
    exporter: OtlpStdoutSpanExporter,
}
impl AppState {
    async fn flush_batch(&self) -> Result<(), Error> {
        let mut batch = self.batch.lock().await;
        if batch.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            "Sending batch of {} records to Kinesis",
            batch.records.len()
        );

        let result = self
            .kinesis_client
            .put_records()
            .stream_name(&self.stream_name)
            .set_records(Some(batch.records.clone()))
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Kinesis batch error: {}", e);
                Error::from(format!("Failed to send records to Kinesis: {}", e))
            })?;

        let failed_count = result.failed_record_count.unwrap_or(0);
        if failed_count > 0 {
            tracing::warn!("Failed to put {} records", failed_count);

            let records = result.records();
            for (i, record) in records.iter().enumerate() {
                if let Some(error_code) = &record.error_code {
                    tracing::warn!(
                        "Record {} failed with error: {} - {}",
                        i,
                        error_code,
                        record
                            .error_message
                            .as_deref()
                            .unwrap_or("No error message")
                    );
                }
            }
        } else {
            tracing::debug!("Successfully sent all records to Kinesis");
        }

        batch.clear();
        Ok(())
    }
}

async fn telemetry_handler(
    events: Vec<LambdaTelemetry>,
    state: Arc<AppState>,
) -> Result<(), Error> {
    for event in events {
        let timestamp = event.time;

        if let LambdaTelemetryRecord::Function(record_str) = &event.record {
            if record_str.starts_with(RECORD_PREFIX) {
                let mut kinesis_batch = state.batch.lock().await;
                if let Err(e) = kinesis_batch.add_record(record_str.clone()) {
                    tracing::error!("Failed to add record to Kinesis batch: {}", e);
                }
                drop(kinesis_batch);
            }
            continue;
        }
        tracing::warn!("Received event: {:?}", event);
        let parsed_event_opt = match event.record {
            LambdaTelemetryRecord::PlatformStart {
                request_id,
                version,
                tracing,
            } => {
                let mut parsed_context = tracing
                    .as_ref()
                    .and_then(|tc| xray::parse_xray_header_value(&tc.value).ok());
                if let (Some(ref mut ctx), Some(platform_tracing)) = (&mut parsed_context, &tracing)
                {
                    ctx.platform_span_id = platform_tracing
                        .span_id
                        .as_deref()
                        .and_then(|s| SpanId::from_hex(s).ok());
                }
                Some(ParsedPlatformEvent {
                    timestamp,
                    request_id,
                    data: PlatformEventData::Start {
                        version,
                        trace_context: parsed_context,
                    },
                })
            }
            LambdaTelemetryRecord::PlatformRuntimeDone {
                request_id,
                status,
                error_type,
                metrics,
                spans,
                tracing,
            } => {
                let mut parsed_context = tracing
                    .as_ref()
                    .and_then(|tc| xray::parse_xray_header_value(&tc.value).ok());
                if let (Some(ref mut ctx), Some(platform_tracing)) = (&mut parsed_context, &tracing)
                {
                    ctx.platform_span_id = platform_tracing
                        .span_id
                        .as_deref()
                        .and_then(|s| SpanId::from_hex(s).ok());
                }
                let telemetry_spans = spans.into_iter().map(TelemetrySpan::from).collect();
                let mut attributes = HashMap::new();
                if let Some(m) = metrics {
                    attributes.insert(
                        "runtime.durationMs".to_string(),
                        OtelValue::F64(m.duration_ms),
                    );
                    if let Some(pb) = m.produced_bytes {
                        attributes.insert(
                            "runtime.producedBytes".to_string(),
                            OtelValue::I64(pb as i64),
                        );
                    }
                }

                Some(ParsedPlatformEvent {
                    timestamp,
                    request_id,
                    data: PlatformEventData::RuntimeDone {
                        status,
                        error_type,
                        metrics: attributes,
                        spans: telemetry_spans,
                        trace_context: parsed_context,
                    },
                })
            }
            LambdaTelemetryRecord::PlatformReport {
                request_id,
                status,
                error_type,
                metrics,
                spans,
                tracing,
            } => {
                let mut parsed_context = tracing
                    .as_ref()
                    .and_then(|tc| xray::parse_xray_header_value(&tc.value).ok());
                if let (Some(ref mut ctx), Some(platform_tracing)) = (&mut parsed_context, &tracing)
                {
                    ctx.platform_span_id = platform_tracing
                        .span_id
                        .as_deref()
                        .and_then(|s| SpanId::from_hex(s).ok());
                }
                let telemetry_spans = spans.into_iter().map(TelemetrySpan::from).collect();
                let mut attributes = HashMap::new();
                attributes.insert(
                    "report.durationMs".to_string(),
                    OtelValue::F64(metrics.duration_ms),
                );
                attributes.insert(
                    "report.billedDurationMs".to_string(),
                    OtelValue::I64(metrics.billed_duration_ms as i64),
                );
                attributes.insert(
                    "report.memorySizeMB".to_string(),
                    OtelValue::I64(metrics.memory_size_mb as i64),
                );
                attributes.insert(
                    "report.maxMemoryUsedMB".to_string(),
                    OtelValue::I64(metrics.max_memory_used_mb as i64),
                );
                if let Some(id) = metrics.init_duration_ms {
                    attributes.insert("report.initDurationMs".to_string(), OtelValue::F64(id));
                }
                if let Some(rd) = metrics.restore_duration_ms {
                    attributes.insert("report.restoreDurationMs".to_string(), OtelValue::F64(rd));
                }

                Some(ParsedPlatformEvent {
                    timestamp,
                    request_id,
                    data: PlatformEventData::Report {
                        status,
                        error_type,
                        metrics: attributes,
                        spans: telemetry_spans,
                        trace_context: parsed_context,
                    },
                })
            }
            // Ignore all init phase and other events
            _ => None,
        };

        if let Some(parsed_event) = parsed_event_opt {
            if let Err(e) = state.platform_event_tx.send(parsed_event).await {
                tracing::error!("Failed to send parsed platform event to aggregator: {}", e);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();
    tracing::debug!("Starting OTLP Stdout Kinesis Extension");

    let config = Config::from_env()?;

    let aws_config = aws_config::from_env().load().await;
    let kinesis_client = KinesisClient::new(&aws_config);

    let (platform_event_tx, platform_event_rx) = mpsc::channel::<ParsedPlatformEvent>(1000);

    // Wrap the receiver for AppState
    let platform_event_rx_arc = Arc::new(Mutex::new(platform_event_rx));

    let exporter = OtlpStdoutSpanExporter::builder()
        .resource(get_lambda_resource())
        .build();

    let aggregations = Mutex::new(HashMap::<String, SpanAggregator>::new());

    let app_state = Arc::new(AppState {
        kinesis_client,
        stream_name: config.kinesis_stream_name.clone(),
        batch: Mutex::new(KinesisBatch::default()),
        platform_event_tx,
        platform_event_rx: platform_event_rx_arc,
        aggregations,
        exporter,
    });

    let telemetry_state = app_state.clone();
    let events_state = app_state.clone();
    let aggregation_state = app_state.clone();

    let handler_fn = move |events: Vec<LambdaTelemetry>| {
        let state = telemetry_state.clone();
        async move { telemetry_handler(events, state).await }
    };

    let events_processor = service_fn(move |event: LambdaEvent| {
        let state = events_state.clone();
        let agg_state = aggregation_state.clone();

        async move {
            let mut spans_to_export: Vec<SpanData> = Vec::new();
            {
                // --- Process buffered platform events ---
                let mut events_to_process = Vec::new();
                // Lock the receiver to drain messages
                let mut receiver_guard = agg_state.platform_event_rx.lock().await;
                loop {
                    match receiver_guard.try_recv() {
                        Ok(evt) => events_to_process.push(evt),
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            tracing::warn!("Platform event channel disconnected.");
                            break;
                        }
                    }
                }
                // Drop the receiver lock BEFORE locking the aggregations map
                // to avoid potential deadlocks if telemetry_handler is also trying to lock something.
                drop(receiver_guard);

                tracing::debug!("Processing {} events", events_to_process.len());
                // Lock aggregations map
                let mut aggregations_map = agg_state.aggregations.lock().await;
                let now = Utc::now();

                let timeout_duration = Duration::try_minutes(30).unwrap_or(Duration::MAX);
                aggregations_map.retain(|key, agg| {
                    if (now - agg.first_seen_timestamp) > timeout_duration {
                        tracing::warn!("Aggregation for '{}' timed out. Emitting.", key);
                        if let Some(span_data) = agg.to_otel_span_data() {
                            spans_to_export.push(span_data);
                            spans_to_export.append(&mut agg.child_spans_data);
                        }
                        false
                    } else {
                        true
                    }
                });

                let mut newly_completed_keys = Vec::new();
                for parsed_event in events_to_process {
                    let key = parsed_event.request_id.clone();

                    if let Some(agg) = aggregations_map.get_mut(&key) {
                        agg.update_from_event(&parsed_event);
                        if agg.is_complete() {
                            if let Some(span_data) = agg.to_otel_span_data() {
                                spans_to_export.push(span_data);
                                spans_to_export.append(&mut agg.child_spans_data);
                            }
                            newly_completed_keys.push(key);
                        }
                    } else {
                        let mut new_agg = SpanAggregator::new(
                            parsed_event.request_id.clone(),
                            parsed_event.timestamp,
                        );
                        new_agg.update_from_event(&parsed_event);
                        if new_agg.is_complete() {
                            if let Some(span_data) = new_agg.to_otel_span_data() {
                                spans_to_export.push(span_data);
                                spans_to_export.append(&mut new_agg.child_spans_data);
                            }
                            newly_completed_keys.push(key);
                        } else {
                            aggregations_map.insert(key, new_agg);
                        }
                    }
                }
                for key in newly_completed_keys {
                    aggregations_map.remove(&key);
                }
            }

            // Export completed/timed-out spans if any
            if !spans_to_export.is_empty() {
                tracing::debug!("Exporting spans via stdout");
                // Assuming exporter instance in agg_state is usable (Send+Sync or Cloned)
                match agg_state.exporter.export(spans_to_export).await {
                    Ok(_) => tracing::debug!("Successfully exported spans via stdout"),
                    Err(e) => tracing::error!("Failed to export spans via stdout: {:?}", e),
                }
            } else {
                tracing::debug!("No spans to export");
            }

            // Existing Kinesis Flush Logic & Shutdown Flush
            match event.next {
                NextEvent::Invoke(_) => {
                    tracing::debug!("Received INVOKE event, flushing Kinesis batch");
                    if let Err(e) = state.flush_batch().await {
                        tracing::error!("Error flushing Kinesis batch on INVOKE: {}", e);
                    }
                }
                NextEvent::Shutdown(_) => {
                    tracing::info!(
                        "Received SHUTDOWN event, flushing Kinesis batch and final aggregations"
                    );
                    // Flush Kinesis
                    if let Err(e) = state.flush_batch().await {
                        tracing::error!("Error flushing Kinesis batch on SHUTDOWN: {}", e);
                    }
                    // Declare vector outside the lock scope
                    let mut final_spans_to_export: Vec<SpanData>;
                    {
                        let mut aggregations_map = agg_state.aggregations.lock().await;
                        // Initialize vector inside the scope
                        final_spans_to_export = Vec::new();
                        for (key, mut agg) in aggregations_map.drain() {
                            tracing::debug!("Flushing remaining agg for '{}' on shutdown", key);
                            if let Some(span_data) = agg.to_otel_span_data() {
                                final_spans_to_export.push(span_data);
                                final_spans_to_export.append(&mut agg.child_spans_data);
                            }
                        }
                    } // Lock released

                    // Now final_spans_to_export is accessible here
                    if !final_spans_to_export.is_empty() {
                        match agg_state.exporter.export(final_spans_to_export).await {
                            Ok(_) => tracing::debug!("Exported final spans on shutdown"),
                            Err(e) => {
                                tracing::error!("Failed to export final spans on shutdown: {:?}", e)
                            }
                        }
                    }
                }
            }
            Ok::<(), Error>(())
        }
    });

    Extension::new()
        .with_events(&["INVOKE", "SHUTDOWN"])
        .with_events_processor(events_processor)
        .with_telemetry_processor(SharedService::new(service_fn(handler_fn)))
        .with_telemetry_types(&["platform", "function"])
        .with_telemetry_buffering(LogBuffering {
            timeout_ms: config.buffer_timeout_ms as usize,
            max_bytes: config.buffer_max_bytes,
            max_items: config.buffer_max_items,
        })
        .run()
        .await?;

    Ok(())
}
