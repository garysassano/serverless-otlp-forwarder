use aws_sdk_kinesis::Client as KinesisClient;
use lambda_extension::{
    service_fn, tracing, Error, Extension, LambdaEvent, LambdaTelemetry, LambdaTelemetryRecord,
    LogBuffering, NextEvent, SharedService,
};
use opentelemetry::{trace::SpanId, Value as OtelValue};
use opentelemetry_sdk::trace::{SpanData, SpanExporter};

// Add nix for mkfifo (Re-add these)
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use nix::errno::Errno;
use std::path::Path;

use lambda_otel_lite::resource::get_lambda_resource;
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

// Import AsyncWriteExt trait AND io module
use tokio::io::AsyncWriteExt;

// Add the modules
mod aggregation;
mod config;
mod events;
mod kinesis;
mod pipe_reader;
mod types;
mod xray;

// Use the types from the modules
use aggregation::SpanAggregator;
use config::Config;
use events::{ParsedPlatformEvent, PlatformEventData, TelemetrySpan};
use kinesis::{KinesisBatch, RECORD_PREFIX};
use pipe_reader::pipe_reader_task;
use types::ProcessorInput;

// Re-add chrono for timeout logic
use chrono::{Duration, Utc};

// Define the pipe path constant
const PIPE_PATH: &str = "/tmp/otlp-stdout-span-exporter.pipe";

// Application state
struct AppState {
    kinesis_client: KinesisClient,
    stream_name: Option<String>,
    batch: Mutex<KinesisBatch>,
    aggregations: Mutex<HashMap<String, SpanAggregator>>,
    exporter: OtlpStdoutSpanExporter,
    processor_input_rx: Mutex<mpsc::Receiver<ProcessorInput>>,
}
impl AppState {
    async fn flush_batch(&self) -> Result<(), Error> {
        if self.stream_name.is_none() {
            tracing::debug!("Kinesis stream not configured, skipping flush.");
            let mut batch = self.batch.lock().await;
            if !batch.is_empty() {
                 tracing::warn!("Clearing {} records from Kinesis batch because Kinesis is disabled.", batch.records.len());
                 batch.clear();
            }
            return Ok(());
        }
        let stream_name = self.stream_name.as_ref().unwrap();

        let mut batch = self.batch.lock().await;
        if batch.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            "Sending batch of {} records to Kinesis stream {}",
            batch.records.len(),
            stream_name
        );

        let result = self
            .kinesis_client
            .put_records()
            .stream_name(stream_name)
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
    tx: mpsc::Sender<ProcessorInput>,
) -> Result<(), Error> {
    for event in events {
        let timestamp = event.time;
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
            if let Err(e) = tx.send(ProcessorInput::PlatformTelemetry(parsed_event)).await {
                tracing::error!("Failed to send platform event to processor channel: {}", e);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();
    tracing::debug!("Starting OTLP Stdout Kinesis Extension");

    // --- Create Named Pipe ---
    let pipe_path = Path::new(PIPE_PATH);
    if !pipe_path.exists() {
        let pipe_path_str = PIPE_PATH.to_string();
        tokio::task::spawn_blocking(move || {
            match mkfifo(pipe_path_str.as_str(), Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO) {
                Ok(_) => tracing::info!("Created named pipe: {}", pipe_path_str),
                Err(Errno::EEXIST) => {
                    tracing::info!("Named pipe already exists: {}", pipe_path_str);
                }
                Err(e) => {
                    panic!("Failed to create named pipe {}: {}", pipe_path_str, e);
                }
            }
        }).await.map_err(|e| Error::from(format!("Pipe creation task failed: {}", e)))?;
    } else {
        tracing::info!("Named pipe already exists: {}", PIPE_PATH);
    }
    // --- Create Named Pipe --- END ---

    let config = Config::from_env()?;

    let aws_config = aws_config::from_env().load().await;
    let kinesis_client = KinesisClient::new(&aws_config);

    // --- Create Unified Channel ---
    let (processor_input_tx, processor_input_rx) = mpsc::channel::<ProcessorInput>(2048);
    // --- Create Unified Channel --- END ---

    // --- Spawn Pipe Reader Task ---
    tokio::spawn(pipe_reader_task(processor_input_tx.clone()));
    // --- Spawn Pipe Reader Task --- END ---

    let exporter = OtlpStdoutSpanExporter::builder()
        .resource(get_lambda_resource())
        .build();

    let aggregations = Mutex::new(HashMap::<String, SpanAggregator>::new());

    let app_state = Arc::new(AppState {
        kinesis_client,
        stream_name: config.kinesis_stream_name.clone(),
        batch: Mutex::new(KinesisBatch::default()),
        aggregations,
        exporter,
        processor_input_rx: Mutex::new(processor_input_rx),
    });

    let telemetry_tx = processor_input_tx.clone();
    let telemetry_handler_fn = move |events: Vec<LambdaTelemetry>| {
        let tx = telemetry_tx.clone();
        async move { telemetry_handler(events, tx).await }
    };

    let processor_state = app_state.clone();

    // Define timeout duration (e.g., 30 minutes)
    // TODO: Make this configurable?
    let aggregation_timeout = Duration::try_minutes(30).unwrap_or_else(|| Duration::max_value());

    let events_processor = service_fn(move |event: LambdaEvent| {
        let state = processor_state.clone();

        async move {
            // --- Wait for first message (blocking) then drain rest (non-blocking) --- START ---
            let mut received_first = false;
            loop {
                let mut receiver_guard = state.processor_input_rx.lock().await;
                let maybe_input = if !received_first {
                    // Block waiting for the first message for this INVOKE cycle
                    // This relies on the pipe reader eventually sending something (data or signal)
                    receiver_guard.recv().await
                } else {
                    // After receiving the first, drain others non-blockingly
                    match receiver_guard.try_recv() {
                        Ok(input) => Some(input),
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None, // Drain complete
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => None, // Channel closed
                    }
                };

                // Process the received message (if any)
                match maybe_input {
                    Some(ProcessorInput::OtlpJson(line)) => {
                        drop(receiver_guard); // Drop lock ASAP
                        if !received_first {
                            tracing::debug!("EventsProcessor: Received first OtlpJson");
                            received_first = true;
                        } else {
                            tracing::debug!("EventsProcessor: Drained OtlpJson");
                        }
                        if state.stream_name.is_some() {
                            let mut kinesis_batch = state.batch.lock().await;
                            if let Err(e) = kinesis_batch.add_record(line) {
                                tracing::error!("Failed to add record to Kinesis batch: {}", e);
                            }
                        } else {
                            println!("{}", line);
                        }
                    }
                    Some(ProcessorInput::PlatformTelemetry(parsed_event)) => {
                        drop(receiver_guard); // Drop lock ASAP
                        if !received_first {
                            tracing::debug!(
                                "EventsProcessor: Received first PlatformTelemetry for request_id: {}",
                                parsed_event.request_id
                            );
                            received_first = true;
                        } else {
                            tracing::debug!(
                                "EventsProcessor: Drained PlatformTelemetry for request_id: {}",
                                parsed_event.request_id
                            );
                        }
                        let mut aggregations_map = state.aggregations.lock().await;
                        let key = parsed_event.request_id.clone();
                        let mut completed_spans: Vec<SpanData> = Vec::new();

                        if let Some(agg) = aggregations_map.get_mut(&key) {
                            agg.update_from_event(&parsed_event);
                            if agg.is_complete() {
                                if let Some(span_data) = agg.to_otel_span_data() {
                                    completed_spans.push(span_data);
                                    completed_spans.append(&mut agg.child_spans_data);
                                }
                                aggregations_map.remove(&key);
                            }
                        } else {
                            let mut new_agg = SpanAggregator::new(key, parsed_event.timestamp);
                            new_agg.update_from_event(&parsed_event);
                            if new_agg.is_complete() {
                                if let Some(span_data) = new_agg.to_otel_span_data() {
                                    completed_spans.push(span_data);
                                    completed_spans.append(&mut new_agg.child_spans_data);
                                }
                            } else {
                                aggregations_map.insert(new_agg.request_id.clone(), new_agg);
                            }
                        }
                        drop(aggregations_map); // Drop lock before await

                        if !completed_spans.is_empty() {
                            tracing::debug!("Exporting {} completed/aggregated spans", completed_spans.len());
                            match state.exporter.export(completed_spans).await {
                                Ok(_) => tracing::debug!("Successfully exported aggregated spans"),
                                Err(e) => tracing::error!("Failed to export aggregated spans: {:?}", e),
                            }
                        }
                    }
                    None => {
                        // No message received (timed out waiting for first, or drain complete, or disconnected)
                        drop(receiver_guard);
                        if !received_first {
                            tracing::debug!("No input received on channel for this cycle.");
                        } else {
                            tracing::debug!("Finished draining channel for this cycle.");
                        }
                        break; // Exit the processing loop
                    }
                }
            } // End loop
            // --- Wait for first message / Drain Unified Channel Loop --- END ---

            // --- Handle Aggregation Timeouts --- START ---
            let mut timed_out_spans: Vec<SpanData> = Vec::new();
            {
                let mut aggregations_map = state.aggregations.lock().await;
                let now = Utc::now();

                // Use retain to efficiently remove timed-out aggregations while checking
                aggregations_map.retain(|key, agg| {
                    if (now - agg.first_seen_timestamp) > aggregation_timeout {
                        tracing::warn!("Aggregation for request_id '{}' timed out after {:?}. Emitting.", key, aggregation_timeout);
                        // Collect spans to export if timed out
                        if let Some(span_data) = agg.to_otel_span_data() {
                            timed_out_spans.push(span_data);
                            timed_out_spans.append(&mut agg.child_spans_data);
                        }
                        false // Remove from map
                    } else {
                        true // Keep in map
                    }
                });
            } // Aggregation map lock released

            // Export timed-out spans (if any) via the exporter (writes to pipe)
            if !timed_out_spans.is_empty() {
                tracing::debug!("Exporting {} timed-out spans", timed_out_spans.len());
                 match state.exporter.export(timed_out_spans).await {
                    Ok(_) => tracing::debug!("Successfully exported timed-out spans"),
                    Err(e) => tracing::error!("Failed to export timed-out spans: {:?}", e),
                 }
            }
            // --- Handle Aggregation Timeouts --- END ---

            // --- Handle Lambda Lifecycle Events --- START ---
            match event.next {
                NextEvent::Invoke(_) => {
                    tracing::debug!("Received INVOKE event, flushing Kinesis batch if needed");
                    if let Err(e) = state.flush_batch().await {
                        tracing::error!("Error flushing Kinesis batch on INVOKE: {}", e);
                    }
                }
                NextEvent::Shutdown(_) => {
                    tracing::info!(
                        "Received SHUTDOWN event, flushing final aggregations and Kinesis batch"
                    );

                    // --- Final Aggregation Flush --- START ---
                    let mut final_spans_to_export: Vec<SpanData> = Vec::new();
                    {
                        let mut aggregations_map = state.aggregations.lock().await;
                        tracing::debug!("Draining {} remaining aggregations on shutdown", aggregations_map.len());
                        // Drain the map, converting each remaining aggregator to spans
                        for (key, mut agg) in aggregations_map.drain() {
                            tracing::debug!("Flushing remaining agg for request_id '{}' on shutdown", key);
                            if let Some(span_data) = agg.to_otel_span_data() {
                                final_spans_to_export.push(span_data);
                                final_spans_to_export.append(&mut agg.child_spans_data);
                            }
                        }
                    } // Aggregations map lock released

                    // Export the final spans (if any) via the exporter (writes to pipe)
                    if !final_spans_to_export.is_empty() {
                        tracing::debug!("Exporting {} final spans on shutdown", final_spans_to_export.len());
                         match state.exporter.export(final_spans_to_export).await {
                            Ok(_) => tracing::debug!("Successfully exported final spans on shutdown"),
                            Err(e) => {
                                tracing::error!("Failed to export final spans on shutdown: {:?}", e)
                            }
                         }
                    }
                    // --- Final Aggregation Flush --- END ---

                    // Final Kinesis Flush (already implemented)
                    if let Err(e) = state.flush_batch().await {
                        tracing::error!("Error flushing Kinesis batch on SHUTDOWN: {}", e);
                    }
                }
            }
            // --- Handle Lambda Lifecycle Events --- END ---

            Ok::<(), Error>(())
        }
    });

    Extension::new()
        .with_events(&["INVOKE", "SHUTDOWN"])
        .with_events_processor(events_processor)
        .with_telemetry_processor(SharedService::new(service_fn(telemetry_handler_fn)))
        .with_telemetry_types(&["platform"])
        .with_telemetry_buffering(LogBuffering {
            timeout_ms: config.buffer_timeout_ms as usize,
            max_bytes: config.buffer_max_bytes,
            max_items: config.buffer_max_items,
        })
        .run()
        .await?;

    Ok(())
}
