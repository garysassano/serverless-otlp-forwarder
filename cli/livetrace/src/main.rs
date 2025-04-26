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
use std::env;
use std::time::Duration;

// External Crates
use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client as ReqwestClient;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// Internal Crate Imports
use crate::aws_setup::setup_aws_resources;
use crate::cli::{parse_event_attr_globs, CliArgs, ColoringMode};
use crate::config::{load_and_resolve_config, save_profile_config, EffectiveConfig, ProfileConfig};
use crate::console_display::{display_console, Theme, get_terminal_width};
use crate::forwarder::{parse_otlp_headers_from_vec, send_batch};
use crate::live_tail_adapter::start_live_tail_task;
use crate::poller::start_polling_task;
use crate::processing::{SpanCompactionConfig, TelemetryData};

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
            compact_display: args.compact_display,
            event_attrs: args.event_attrs.clone(),
            event_severity_attribute: args.event_severity_attribute.clone(),
            poll_interval: args.poll_interval,
            session_timeout: args.session_timeout,
            verbose: args.verbose,
            theme: args.theme.clone(),
            span_attrs: args.span_attrs.clone(),
            color_by: args.color_by,
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
    let event_attr_globs = parse_event_attr_globs(&config.event_attrs); // Now passes the Option<String> directly
    let span_attr_globs = parse_event_attr_globs(&config.span_attrs); // Use the same function for parsing

    

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
    println!("  {:<18}: {}", "Compact Display".dimmed(), if config.compact_display { "Yes" } else { "No" });
    println!("  {:<18}: {}", "Color By".dimmed(), match config.color_by {
        ColoringMode::Service => "Service",
        ColoringMode::Span => "Span ID",
    });
    if let Some(attrs) = &config.event_attrs {
        println!("  {:<18}: {}", "Event Attributes".dimmed(), attrs);
    } else {
        println!("  {:<18}: All", "Event Attributes".dimmed());
    }
    println!("  {:<18}: {}", "Severity Attr".dimmed(), config.event_severity_attribute);
    
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
    let mut telemetry_buffer: Vec<TelemetryData> = Vec::new();
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
                        // Pause spinner when receiving data
                        spinner.set_message("Processing telemetry data...");
                        
                        // Successfully received telemetry data
                        if console_enabled || endpoint_opt.is_some() {
                            telemetry_buffer.push(telemetry);
                        }
                        
                        // Resume spinner
                        spinner.set_message("Waiting for telemetry events...");
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
                if !telemetry_buffer.is_empty() {
                    // Store the batch locally
                    let batch_to_send = std::mem::take(&mut telemetry_buffer);
                    
                    // Temporarily suspend spinner during display 
                    spinner.suspend(|| {
                        tracing::debug!("Timer tick: Processing buffer with {} items.", batch_to_send.len());
    
                        if console_enabled {
                            // Convert string theme to Theme enum
                            let theme = Theme::from_str(&config.theme);
                            if let Err(e) = display_console(
                                &batch_to_send,
                                config.compact_display,
                                &event_attr_globs,
                                config.event_severity_attribute.as_str(),
                                theme,
                                &span_attr_globs,
                                config.color_by,
                            ) {
                                tracing::error!(error = %e, "Error displaying telemetry data");
                            }
                        }
                    });
                    
                    // Process any async work outside of suspend
                    if let Some(endpoint) = endpoint_opt {
                        if let Err(e) = send_batch(
                            &http_client,
                            endpoint,
                            batch_to_send,
                            &compaction_config,
                            otlp_header_map.clone(),
                        ).await {
                            tracing::error!(error = %e, "Error sending telemetry data");
                        }
                    }
                }
            }
        }
    }

    // Finish spinner before final flush
    spinner.finish_and_clear();

    // 11. Final Flush
    if !telemetry_buffer.is_empty() {
        tracing::debug!(
            "Flushing remaining {} items from buffer before exiting.",
            telemetry_buffer.len()
        );
        let final_batch = std::mem::take(&mut telemetry_buffer);

        if console_enabled {
            // Convert string theme to Theme enum
            let theme = Theme::from_str(&config.theme);
            if let Err(e) = display_console(
                &final_batch,
                config.compact_display,
                &event_attr_globs,
                &config.event_severity_attribute,
                theme,
                &span_attr_globs,
                config.color_by,
            ) {
                tracing::error!(error = %e, "Error displaying final telemetry data");
            }
        }

        if let Some(endpoint) = endpoint_opt {
            if let Err(e) = send_batch(
                &http_client,
                endpoint,
                final_batch,
                &compaction_config,
                otlp_header_map.clone(),
            ).await {
                tracing::error!(error = %e, "Error sending final telemetry data");
            }
        }
    }

    tracing::info!("livetrace finished successfully.");
    Ok(())
}
