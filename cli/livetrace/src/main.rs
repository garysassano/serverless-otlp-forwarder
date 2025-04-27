// Crate Modules
mod aws_setup;
mod cli;
mod config;
mod console_display;
mod forwarder;
mod live_tail_adapter;
mod poller;
mod processing;

// Standard Library
use std::collections::HashMap;
use std::env;
use std::time::Duration;

// External Crates
use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use reqwest::Client as ReqwestClient;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// Internal Crate Imports
use crate::aws_setup::setup_aws_resources;
use crate::cli::{parse_attr_globs, CliArgs, ColoringMode};
use crate::config::{load_and_resolve_config, save_profile_config, EffectiveConfig, ProfileConfig};
use crate::console_display::{display_console, Theme, get_terminal_width};
use crate::forwarder::{parse_otlp_headers_from_vec, send_batch};
use crate::live_tail_adapter::start_live_tail_task;
use crate::poller::start_polling_task;
use crate::processing::{SpanCompactionConfig, TelemetryData};

// Constants for trace buffering timeouts
const IDLE_TIMEOUT: Duration = Duration::from_millis(500); // Time to wait after root is seen
const ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(3);  // Max time to wait regardless of root

// Structure to hold state for traces being buffered
#[derive(Debug)]
struct TraceBufferState {
    buffered_payloads: Vec<TelemetryData>,
    has_received_root: bool,
    first_message_received_at: Instant,
    last_message_received_at: Instant,
}

/// livetrace: Tail CloudWatch Logs for OTLP/stdout traces and forward them.
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse Args fully now (clap handles defaults)
    let args = CliArgs::parse();

    // Check if list-themes was specified
    if args.list_themes {
        println!("\nAvailable themes:");
        println!("  * default - OpenTelemetry-inspired blue-purple palette");
        println!("  * tableau - Tableau 12 color palette with distinct hues");
        println!("  * colorbrewer - ColorBrewer Set3 palette (pastel colors)");
        println!("  * material - Material Design palette with bright, modern colors");
        println!("  * solarized - Solarized color scheme with muted tones");
        println!("  * monochrome - Grayscale palette for minimal distraction");
        println!("\nUsage: livetrace --theme <THEME>");
        return Ok(());
    }

    // Save Profile Check
    if let Some(profile_name) = args.save_profile.as_ref() {
        // Convert CliArgs to the savable ProfileConfig format
        let profile_to_save = ProfileConfig::from_cli_args(&args);
        // Call the actual save function
        save_profile_config(profile_name, &profile_to_save)?;
        println!(
            "Configuration saved to profile '{}'. Exiting.",
            profile_name
        );
        return Ok(());
    }
    // End Save Profile Check

    // Load Configuration Profile if Specified
    let config = if args.config_profile.is_some() {
        load_and_resolve_config(args.config_profile.clone(), &args)?
    } else {
        EffectiveConfig {
            log_group_pattern: args.log_group_pattern.clone(),
            stack_name: args.stack_name.clone(),
            otlp_endpoint: args.otlp_endpoint.clone(),
            otlp_headers: args.otlp_headers.clone(),
            aws_region: args.aws_region.clone(),
            aws_profile: args.aws_profile.clone(),
            forward_only: args.forward_only,
            attrs: args.attrs.clone(),
            event_severity_attribute: args.event_severity_attribute.clone(),
            poll_interval: args.poll_interval,
            session_timeout: args.session_timeout,
            verbose: args.verbose,
            theme: args.theme.clone(),
            color_by: args.color_by,
            events_only: args.events_only,
        }
    };

    // Validate discovery parameters - either log_group_pattern or stack_name must be set
    if config.log_group_pattern.is_none() && config.stack_name.is_none() {
        return Err(anyhow::anyhow!(
            "Either --log-group-pattern or --stack-name must be provided on the command line or in the configuration profile"
        ));
    }

    // 2. Initialize Logging
    let log_level = match config.verbose {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::WARN.into())
                .parse_lossy(format!("{}={}", env!("CARGO_PKG_NAME"), log_level)),
        )
        .init();
    tracing::debug!("Starting livetrace with configuration: {:?}", config);

    // 3. Resolve OTLP Endpoint (CONFIG > TRACES_ENV > GENERAL_ENV)
    let resolved_endpoint: Option<String> = config.otlp_endpoint.clone().or_else(|| {
        env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
            .ok()
            .or_else(|| env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
    });
    let endpoint_opt = resolved_endpoint.as_deref();
    tracing::debug!(config_endpoint = ?config.otlp_endpoint, env_traces = ?env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok(), env_general = ?env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(), resolved = ?resolved_endpoint, "Resolved OTLP endpoint");

    // 4. Resolve OTLP Headers (CONFIG > TRACES_ENV > GENERAL_ENV)
    let resolved_headers_vec: Vec<String> = if !config.otlp_headers.is_empty() {
        tracing::debug!(source="config", headers=?config.otlp_headers, "Using headers from configuration");
        config.otlp_headers.clone()
    } else if let Ok(hdr_str) = env::var("OTEL_EXPORTER_OTLP_TRACES_HEADERS") {
        let headers: Vec<String> = hdr_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        tracing::debug!(source="env_traces", headers=?headers, "Using headers from OTEL_EXPORTER_OTLP_TRACES_HEADERS");
        headers
    } else if let Ok(hdr_str) = env::var("OTEL_EXPORTER_OTLP_HEADERS") {
        let headers: Vec<String> = hdr_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        tracing::debug!(source="env_general", headers=?headers, "Using headers from OTEL_EXPORTER_OTLP_HEADERS");
        headers
    } else {
        Vec::new() // No headers specified anywhere
    };

    // 5. Post-Resolution Validation
    if config.forward_only && endpoint_opt.is_none() {
        return Err(anyhow::anyhow!("
            --forward-only requires --otlp-endpoint argument or OTEL_EXPORTER_OTLP_TRACES_ENDPOINT/OTEL_EXPORTER_OTLP_ENDPOINT env var to be set"));
    }
    if !config.forward_only && endpoint_opt.is_none() {
        tracing::debug!("Running in console-only mode. No OTLP endpoint configured.");
    }

    // 6. AWS Setup (Config, Clients, Discovery, Validation, ARN Construction)
    // Pass only the specific parameters needed by setup_aws_resources
    let aws_result = setup_aws_resources(
        &config.log_group_pattern,
        &config.stack_name,
        &config.aws_region,
        &config.aws_profile
    ).await?;
    let cwl_client = aws_result.cwl_client;
    let account_id = aws_result.account_id;
    let region_str = aws_result.region_str;
    let resolved_log_group_arns = aws_result.resolved_arns;

    // 7. Setup HTTP Client & Parse Resolved OTLP Headers
    let http_client = ReqwestClient::builder()
        .build()
        .context("Failed to build Reqwest client")?;
    tracing::debug!("Reqwest HTTP client created.");
    let otlp_header_map = parse_otlp_headers_from_vec(&resolved_headers_vec)?;
    let compaction_config = SpanCompactionConfig::default();

    // 8. Prepare Console Display
    let console_enabled = !config.forward_only;
    let attr_globs = parse_attr_globs(&config.attrs); // Now passes the Option<String> directly

    // Preamble Output (List Style)
    let preamble_width: usize = get_terminal_width(80);
    let config_heading = "Livetrace Configuration";
    let config_padding = preamble_width.saturating_sub(config_heading.len() + 3);

    println!("\n");
    println!(
        "{} {} {}\n",
        "─".dimmed(),
        config_heading.bold(),
        "─".repeat(config_padding).dimmed()
    );
    
    // Basic AWS Information
    println!("  {:<18}: {}", "AWS Account ID".dimmed(), account_id);
    println!("  {:<18}: {}", "AWS Region".dimmed(), region_str);
    if let Some(profile) = &config.aws_profile {
        println!("  {:<18}: {}", "AWS Profile".dimmed(), profile);
    }
    
    // Log Group Sources
    if let Some(patterns) = &config.log_group_pattern {
        println!("  {:<18}: {:?}", "Pattern".dimmed(), patterns);
    }
    if let Some(stack) = &config.stack_name {
        println!("  {:<18}: {}", "CloudFormation".dimmed(), stack);
    }
    
    // Modes & Settings
    println!();
    
    // Operation Mode
    if let Some(poll_secs) = config.poll_interval {
        println!("  {:<18}: Polling", "Mode".dimmed());
        println!("  {:<18}: {} seconds", "Poll Interval".dimmed(), poll_secs);
    } else {
        println!("  {:<18}: Live Tail", "Mode".dimmed());
        println!("  {:<18}: {} minutes", "Session Timeout".dimmed(), config.session_timeout);
    }
    
    // Forwarding Configuration
    println!("  {:<18}: {}", "Forward Only".dimmed(), if config.forward_only { "Yes" } else { "No" });
    if let Some(endpoint) = &resolved_endpoint {
        println!("  {:<18}: {}", "OTLP Endpoint".dimmed(), endpoint);
    } else {
        println!("  {:<18}: Not configured", "OTLP Endpoint".dimmed());
    }
    if !resolved_headers_vec.is_empty() {
        println!("  {:<18}: {} headers", "OTLP Headers".dimmed(), resolved_headers_vec.len());
    }
    
    // Display Settings
    println!("  {:<18}: {}", "Theme".dimmed(), config.theme);
    println!("  {:<18}: {}", "Color By".dimmed(), match config.color_by {
        ColoringMode::Service => "Service",
        ColoringMode::Span => "Span ID",
    });
    if let Some(attrs) = &config.attrs {
        println!("  {:<18}: {}", "Attributes".dimmed(), attrs);
    } else {
        println!("  {:<18}: All", "Attributes".dimmed());
    }
    println!("  {:<18}: {}", "Severity Attr".dimmed(), config.event_severity_attribute);
    println!("  {:<18}: {}", "Events Only".dimmed(), if config.events_only { "Yes" } else { "No" });
    
    // Config Source
    if let Some(profile) = &args.config_profile {
        println!("  {:<18}: {}", "Config Profile".dimmed(), profile);
    }
    
    // Verbosity Level
    let verbosity_str = match config.verbose {
        0 => "Normal",
        1 => "Debug (-v)",
        _ => {
            let v_str = format!("Trace (-v{})", "v".repeat(config.verbose as usize - 1));
            Box::leak(v_str.into_boxed_str()) // Convert to 'static &str
        }
    };
    println!("  {:<18}: {}", "Verbosity".dimmed(), verbosity_str);
    
    // Display Resolved Log Groups - moved to the end
    println!();
    let validated_log_group_names_for_display: Vec<String> = resolved_log_group_arns
        .iter()
        .map(|arn| arn.split(':').next_back().unwrap_or("unknown-name").to_string())
        .collect();
    print!(
        "  {:<18}: ",
        "Log Groups".dimmed()
    );
    if let Some((first, rest)) = validated_log_group_names_for_display.split_first() {
        println!("{}", first.bright_black());
        for name in rest {
            println!("{:<22}{}", "", name.bright_black());
        }
    } else {
        println!("None");
    }
    
    println!("\n");
    // End Preamble

    // 9. Create MPSC Channel and Spawn Event Source Task
    let (tx, mut rx) = mpsc::channel::<Result<TelemetryData>>(100); // Channel for TelemetryData or errors

    if let Some(interval_secs) = config.poll_interval {
        // Polling Mode
        tracing::debug!(
            interval = interval_secs,
            "Using FilterLogEvents polling mode."
        );
        start_polling_task(cwl_client, resolved_log_group_arns, interval_secs, tx);
    } else {
        // Live Tail Mode
        tracing::debug!(
            timeout_minutes = config.session_timeout,
            "Using StartLiveTail streaming mode with timeout."
        );
        start_live_tail_task(
            cwl_client,
            resolved_log_group_arns,
            tx,
            config.session_timeout,
        );
    }

    // 10. Main Event Processing Loop
    tracing::debug!("Waiting for telemetry events...");
    // Use a HashMap to buffer telemetry data by trace_id
    let mut trace_buffers: HashMap<String, TraceBufferState> = HashMap::new();
    let mut ticker = interval(Duration::from_secs(1));
    
    // Create a spinner for waiting period
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner} {msg}")
            .unwrap()
    );
    spinner.set_message("Waiting for telemetry events...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    loop {
        tokio::select! {
            // Receive from either the poller or live tail adapter task
            received = rx.recv() => {
                match received {
                    Some(Ok(telemetry)) => {
                        // Decode the OTLP payload to extract the trace ID and check for root span
                        match ExportTraceServiceRequest::decode(telemetry.payload.as_slice()) {
                            Ok(request) => {
                                // Pause spinner when receiving data
                                spinner.set_message("Processing telemetry data...");
                                
                                let mut trace_id_hex_opt: Option<String> = None;
                                let mut is_root_present_in_req = false;

                                // Find trace_id and check for root span in this request
                                for resource_span in &request.resource_spans {
                                    for scope_span in &resource_span.scope_spans {
                                        for span in &scope_span.spans {
                                            if trace_id_hex_opt.is_none() {
                                                trace_id_hex_opt = Some(hex::encode(&span.trace_id));
                                            }
                                            if span.parent_span_id.is_empty() {
                                                is_root_present_in_req = true;
                                                // Could potentially break early here if both found
                                            }
                                        }
                                    }
                                }

                                if let Some(tid) = trace_id_hex_opt {
                                    let now = Instant::now();
                                    let state = trace_buffers
                                        .entry(tid)
                                        .or_insert_with(|| TraceBufferState {
                                            buffered_payloads: Vec::new(),
                                            has_received_root: false,
                                            first_message_received_at: now,
                                            last_message_received_at: now,
                                        });

                                    // Update state
                                    state.buffered_payloads.push(telemetry);
                                    state.has_received_root |= is_root_present_in_req;
                                    state.last_message_received_at = now;
                                } else {
                                    tracing::warn!("Received OTLP request with no spans, cannot determine trace ID.");
                                }
                                
                                // Resume spinner
                                spinner.set_message("Waiting for telemetry events...");
                            }
                            Err(e) => {
                                // Decoding error - log and discard
                                spinner.set_message(format!("Error: {}", e));
                                tracing::warn!(error = %e, "Failed to decode OTLP protobuf payload, skipping item.");
                            }
                        }
                    }
                    Some(Err(e)) => {
                        // An error occurred in the source task (polling or live tail)
                        spinner.set_message(format!("Error: {}", e));
                        tracing::error!(error = %e, "Error received from event source task");
                    }
                    None => {
                        // Channel closed by the sender task
                        spinner.finish_with_message("Event source channel closed");
                        tracing::info!("Event source channel closed. Exiting.");
                        break;
                    }
                }
            }
            // Ticker Branch
            _ = ticker.tick() => {
                let now = Instant::now();
                let mut trace_ids_to_flush: Vec<String> = Vec::new();

                // Identify traces ready for flushing based on timeouts
                for (trace_id, state) in trace_buffers.iter() {
                    let time_since_last = now.duration_since(state.last_message_received_at);
                    let time_since_first = now.duration_since(state.first_message_received_at);

                    let should_flush =
                        (state.has_received_root && time_since_last > IDLE_TIMEOUT) // Root received + Idle
                        || (time_since_first > ABSOLUTE_TIMEOUT);                   // Absolute timeout

                    if should_flush {
                        trace_ids_to_flush.push(trace_id.clone());
                    }
                }

                if !trace_ids_to_flush.is_empty() {
                    // Collect data needed for processing before removing from map
                    let mut batches_to_process: Vec<(String, Vec<TelemetryData>, bool)> = Vec::new();
                    for trace_id in &trace_ids_to_flush {
                        if let Some(state) = trace_buffers.get(trace_id) {
                            // Clone data needed for display/forwarding
                            batches_to_process.push((
                                trace_id.clone(),
                                state.buffered_payloads.clone(),
                                state.has_received_root,
                            ));
                        }
                    }

                    // Suspend spinner for synchronous display processing
                    if console_enabled && !batches_to_process.is_empty() {
                        spinner.suspend(|| {
                            for (trace_id, batch, root_received) in &batches_to_process {
                                tracing::debug!(%trace_id, count = batch.len(), "Displaying flushed trace buffer.");
                                let theme = Theme::from_str(&config.theme);
                                if let Err(e) = display_console(
                                    batch,
                                    &attr_globs,
                                    config.event_severity_attribute.as_str(),
                                    theme,
                                    config.color_by,
                                    config.events_only,
                                    *root_received, // Pass the root_received flag
                                ) {
                                    tracing::error!(error = %e, %trace_id, "Error displaying telemetry data");
                                }
                            }
                        });
                    }

                    // Handle asynchronous forwarding outside suspend
                    if let Some(endpoint) = endpoint_opt {
                        for (trace_id, batch, _root_received) in batches_to_process {
                            tracing::debug!(%trace_id, count = batch.len(), "Forwarding flushed trace buffer.");
                            // Clone necessary elements for the async task
                            let client = http_client.clone();
                            let endpoint_str = endpoint.to_string();
                            let headers = otlp_header_map.clone();
                            let cfg = compaction_config.clone();

                            tokio::spawn(async move {
                                if let Err(e) = send_batch(
                                    &client,
                                    &endpoint_str,
                                    batch,
                                    &cfg,
                                    headers,
                                ).await {
                                    tracing::error!(error = %e, %trace_id, "Error sending telemetry data");
                                }
                            });
                        }
                    }

                    // Now remove the flushed traces from the main buffer
                    for trace_id in trace_ids_to_flush {
                        trace_buffers.remove(&trace_id);
                    }
                }
            }
        }
    }

    // Finish spinner before final flush
    spinner.finish_and_clear();

    // 11. Final Flush
    if !trace_buffers.is_empty() {
        tracing::debug!(
            "Flushing remaining {} traces from buffer before exiting.",
            trace_buffers.len()
        );
        
        // Move all remaining states out of the map
        let final_batches: Vec<(String, Vec<TelemetryData>, bool)> = trace_buffers
            .into_iter()
            .map(|(id, state)| (id, state.buffered_payloads, state.has_received_root))
            .collect();

        let mut forward_tasks = Vec::new(); // Collect futures for forwarding

        // Process synchronously for console display first
        if console_enabled {
            for (trace_id, final_batch, root_received) in &final_batches {
                tracing::debug!(%trace_id, count = final_batch.len(), "Displaying final trace buffer.");
                let theme = Theme::from_str(&config.theme);
                if let Err(e) = display_console(
                    final_batch,
                    &attr_globs,
                    &config.event_severity_attribute,
                    theme,
                    config.color_by,
                    config.events_only,
                    *root_received, // Pass the root_received flag
                ) {
                    tracing::error!(error = %e, %trace_id, "Error displaying final telemetry data");
                }
            }
        }

        // Queue forwarding tasks if needed
        if let Some(endpoint) = endpoint_opt {
            for (trace_id, final_batch, _root_received) in final_batches {
                tracing::debug!(%trace_id, count = final_batch.len(), "Queueing final trace buffer for forwarding.");
                // Clone necessary elements
                let client = http_client.clone();
                let endpoint_str = endpoint.to_string();
                let headers = otlp_header_map.clone();
                let cfg = compaction_config.clone();

                // Add the future to a list to be awaited
                forward_tasks.push(async move {
                    if let Err(e) = send_batch(
                        &client,
                        &endpoint_str,
                        final_batch,
                        &cfg,
                        headers,
                    ).await {
                        tracing::error!(error = %e, %trace_id, "Error sending final telemetry data");
                    }
                });
            }
            
            // Await all forwarding tasks concurrently
            if !forward_tasks.is_empty() {
                tracing::info!("Waiting for final telemetry forwarding to complete...");
                join_all(forward_tasks).await;
                tracing::info!("Final telemetry forwarding complete.");
            }
        }
    }

    tracing::info!("livetrace finished successfully.");
    Ok(())
}
