use anyhow::Result;
use aws_credential_types::provider::ProvideCredentials;
use lambda_otel_utils::{HttpTracerProviderBuilder, OpenTelemetrySubscriberBuilder};
use opentelemetry_sdk::trace::TracerProvider;
use otlp_sigv4_client::SigV4ClientBuilder;
use reqwest::Client as ReqwestClient;

/// Initialize OpenTelemetry with configuration from environment variables
///
/// Environment variables:
/// - OTEL_EXPORTER_OTLP_ENDPOINT: The OTLP endpoint URL (default: AWS App Signals endpoint)
/// - OTEL_SERVICE_NAME: Service name for telemetry (default: "lambda-benchmark")
/// - OTEL_EXPORTER_OTLP_PROTOCOL: Protocol to use (http/protobuf or http/json)
/// - AWS_REGION: AWS region for signing requests
/// - RUST_LOG: Log level (e.g. "info" to see telemetry data)
pub async fn init_telemetry() -> Result<TracerProvider> {
    let config = aws_config::load_from_env().await;
    let region = config.region()
        .expect("AWS region is required")
        .to_string();
    
    let credentials = config
        .credentials_provider()
        .expect("AWS credentials provider is required")
        .provide_credentials()
        .await?;

    // Build HTTP client with AWS SigV4 signing
    let http_client = SigV4ClientBuilder::new()
        .with_client(ReqwestClient::new())
        .with_credentials(credentials)
        .with_region(region)
        .with_service("xray") // For AWS App Signals
        .with_signing_predicate(Box::new(|request| {
            // Only sign requests to AWS endpoints
            request
                .uri()
                .host()
                .map_or(false, |host| host.ends_with(".amazonaws.com"))
        }))
        .build()?;

    // Build and initialize tracer provider
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_http_client(http_client)
        .with_default_text_map_propagator()
        .with_batch_exporter()
        .enable_global(true)
        .build()?;

    // Initialize the OpenTelemetry subscriber
    OpenTelemetrySubscriberBuilder::new()
        .with_env_filter(true)
        .with_env_filter_string("info")
        .with_tracer_provider(tracer_provider.clone())
        .with_service_name(env!("CARGO_PKG_NAME"))
        .init()
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(tracer_provider)
} 