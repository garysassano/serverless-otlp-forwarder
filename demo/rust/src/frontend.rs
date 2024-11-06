use anyhow::{Context, Result};
use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
use lambda_lw_router::{define_router, route};
use lambda_otel_utils::{HttpOtelLayer, HttpTracerProviderBuilder};
use lambda_runtime::{
    layers::{OpenTelemetryFaasTrigger, OpenTelemetryLayer},
    service_fn,
    tower::Layer,
    Error, LambdaEvent, Runtime,
};
use opentelemetry::global;
use opentelemetry::trace::{Status, TraceContextExt};
use opentelemetry_http::HeaderInjector;
use opentelemetry_semantic_conventions as semconv;
use reqwest::{header::HeaderMap, Client};
use serde_json::{json, Value};
use std::env;
use std::sync::Arc;
use tera::{Context as TeraContext, Tera};
use tracing::instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use chrono::{DateTime, Utc, Duration};
use tracing::debug;

// Embed the quotes.html template at compile time
const QUOTES_TEMPLATE: &str = include_str!("templates/quotes.html");

define_router!(event = ApiGatewayV2httpRequest);

#[derive(Clone)]
struct AppState {
    http_client: Client,
    base_context: TeraContext,
    target_url: String,
    templates: Tera,
}

#[instrument(skip_all, fields(otel.kind = "client", http.request.method = "GET", url.full, http.response.status_code))]
async fn get_all_quotes(client: &Client, target_url: &str) -> Result<Value> {
    let target_url = format!("{}/quotes", target_url);

    let current_span = tracing::Span::current();
    let cx = current_span.context();
    current_span.record("url.full", target_url.as_str());

    // Inject tracing context into request headers
    let mut headers = HeaderMap::new();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector(&mut headers));
    });

    tracing::debug!(
        http.request.headers = ?headers, "Sending request with headers"
    );

    let request = client
        .get(target_url.as_str())
        .headers(headers)
        .build()
        .with_context(|| format!("Failed to create request for URL: {}", target_url.as_str()))?;

    let response = client
        .execute(request)
        .await
        .with_context(|| format!("Failed to execute request to {}", target_url.as_str()))?;

    let status = response.status();
    current_span.record(semconv::trace::HTTP_RESPONSE_STATUS_CODE, status.as_u16());

    // Set the span status based on the HTTP status code
    let otel_status = if status.is_success() {
        Status::Ok
    } else {
        Status::Error {
            description: format!("HTTP error: {}", status).into(),
        }
    };
    cx.span().set_status(otel_status);

    // Handle non-success status codes
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .context("Failed to read error response body")?;
        return Err(anyhow::anyhow!("HTTP error {}: {}", status, error_body));
    }

    let response_body = response
        .json::<Value>()
        .await
        .context("Failed to parse response body as JSON")?;

    Ok(response_body)
}

#[instrument(skip_all, fields(otel.kind = "client", http.request.method = "GET", url.full, http.response.status_code))]
async fn get_quote(client: &Client, target_url: &str, id: &str) -> Result<Value> {
    let target_url = format!("{}/quotes/{}", target_url, id);

    let current_span = tracing::Span::current();
    let cx = current_span.context();
    current_span.record("url.full", target_url.as_str());

    let mut headers = HeaderMap::new();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector(&mut headers));
    });

    let request = client
        .get(target_url.as_str())
        .headers(headers)
        .build()
        .with_context(|| format!("Failed to create request for URL: {}", target_url.as_str()))?;

    let response = client
        .execute(request)
        .await
        .with_context(|| format!("Failed to execute request to {}", target_url.as_str()))?;

    let status = response.status();
    current_span.record(semconv::trace::HTTP_RESPONSE_STATUS_CODE, status.as_u16());

    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .context("Failed to read error response body")?;
        return Err(anyhow::anyhow!("HTTP error {}: {}", status, error_body));
    }

    let response_body = response
        .json::<Value>()
        .await
        .context("Failed to parse response body as JSON")?;

    Ok(response_body)
}

fn format_relative_time(timestamp: &str) -> String {
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .unwrap_or_else(|_| DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").unwrap());
    let now = Utc::now();
    let duration = now.signed_duration_since(timestamp.with_timezone(&Utc));

    if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    }
}

#[route(method = "GET", path = "/")]
async fn handle_root(ctx: router::RouteContext) -> Result<Value, Error> {
    let time_filter = ctx.event.query_string_parameters
        .first("time")
        .unwrap_or("6h"); // Default to showing quotes up to 6 hours old
    
    // Define the time ranges
    let (start_duration, end_duration) = match time_filter {
        "6h" => (Duration::zero(), Duration::hours(6)),         // 0-6 hours old
        "6_12h" => (Duration::hours(6), Duration::hours(12)),   // 6-12 hours old
        "12_24h" => (Duration::hours(12), Duration::hours(24)), // 12-24 hours old
        "24_48h" => (Duration::hours(24), Duration::hours(48)), // 1-2 days old
        _ => (Duration::zero(), Duration::hours(6)),            // default to 0-6 hours
    };

    let response = get_all_quotes(&ctx.state.http_client, &ctx.state.target_url).await?;
    let mut tera_ctx = ctx.state.base_context.clone();

    if let Value::Array(quotes_array) = response {
        let mut processed_quotes: Vec<Value> = quotes_array.into_iter()
            .filter(|quote| {
                if let Some(timestamp) = quote.get("timestamp").and_then(|t| t.as_str()) {
                    if let Ok(quote_time) = DateTime::parse_from_rfc3339(timestamp) {
                        let age = Utc::now().signed_duration_since(quote_time);
                        return age >= start_duration && age < end_duration;
                    }
                }
                false
            })
            .map(|mut quote| {
                if let Some(timestamp) = quote.get("timestamp").and_then(|t| t.as_str()) {
                    let relative_time = format_relative_time(timestamp);
                    quote.as_object_mut().unwrap().insert(
                        "relative_time".to_string(),
                        Value::String(relative_time),
                    );
                }
                quote
            })
            .collect();

        // Sort quotes by timestamp in descending order (newest first)
        processed_quotes.sort_by(|a, b| {
            let a_time = a.get("timestamp")
                .and_then(|t| t.as_str())
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok());
            let b_time = b.get("timestamp")
                .and_then(|t| t.as_str())
                .and_then(|t| DateTime::parse_from_rfc3339(t).ok());
            
            match (a_time, b_time) {
                (Some(a), Some(b)) => b.cmp(&a),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        tera_ctx.insert("quotes", &processed_quotes);
    }

    tera_ctx.insert("time", time_filter);

    let html_content = ctx.state.templates.render("quotes.html", &tera_ctx)?;

    Ok(json!({
        "statusCode": 200,
        "headers": {"Content-Type": "text/html"},
        "body": html_content
    }))
}

#[route(method = "GET", path = "/quote/{id}")]
async fn handle_quote(ctx: router::RouteContext) -> Result<Value, Error> {
    let quote_id = ctx.params.get("id")
        .ok_or_else(|| Error::from("Quote ID not provided"))?;
    
    let response = get_quote(&ctx.state.http_client, &ctx.state.target_url, quote_id).await?;
    let mut tera_ctx = ctx.state.base_context.clone();
    
    // Process the single quote to add relative_time
    let mut quote = response;
    if let Some(timestamp) = quote.get("timestamp").and_then(|t| t.as_str()) {
        let relative_time = format_relative_time(timestamp);
        quote.as_object_mut()
            .ok_or_else(|| Error::from("Invalid quote format"))?
            .insert("relative_time".to_string(), Value::String(relative_time));
    }
    
    // Wrap in array for template compatibility
    tera_ctx.insert("quotes", &vec![quote]);
    tera_ctx.insert("single_quote", &true);
    tera_ctx.insert("time", "current");  // Add time context even for single quote

    let html_content = ctx.state.templates.render("quotes.html", &tera_ctx)
        .map_err(|e| Error::from(format!("Failed to render template: {}", e)))?;

    Ok(json!({
        "statusCode": 200,
        "headers": {"Content-Type": "text/html"},
        "body": html_content
    }))
}

#[route(method = "GET", path = "/hello")]
async fn handle_hello(ctx: router::RouteContext) -> Result<Value, Error> {
    Ok(json!({
        "statusCode": 200,  
        "headers": {
            "Content-Type": "application/json",
        },
        "body": ctx.event
    }))
}

#[route(method = "GET", path = "/files/{folder}/{proxy+}")]
async fn handle_files(ctx: router::RouteContext) -> Result<Value, Error> {
    Ok(json!({
        "statusCode": 200,
        "headers": {
            "Content-Type": "application/json",
            "X-Custom-Header": "my-header-value"
        },
        "body": json!({
            "folder": ctx.params.get("folder"),
            "path": ctx.params.get("proxy"),
            "raw_path": ctx.path,
            "all_params": ctx.params,
        }).to_string()
    }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let target_url = env::var("TARGET_URL").expect("TARGET_URL must be set");
    // Initialize templates
    let templates = {
        let mut tera = Tera::default();
        tera.add_raw_template("quotes.html", QUOTES_TEMPLATE)
            .expect("Failed to add embedded template");
        tera
    };

    // Initialize application state
    let state = Arc::new(AppState {
        http_client: Client::builder()
            .build()
            .expect("Failed to create HTTP client"),
        base_context: {
            let mut ctx = TeraContext::new();
            ctx.insert("app_name", "Quote Viewer");
            ctx.insert("version", env!("CARGO_PKG_VERSION"));
            ctx
        },
        target_url,
        templates,
    });

    let router = Arc::new(RouterBuilder::from_registry().build());

    // Initialize tracer
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("client")
        .enable_fmt_layer(true)
        .enable_global(true)
        .with_default_text_map_propagator()
        .with_batch_exporter()
        .build()?;

    // Create service with router and state - no need for preliminary clones
    let service = HttpOtelLayer::new(|| {
        // tracer_provider.force_flush();
        for result in tracer_provider.force_flush() {
            if let Err(err) = result {
                println!("Error flushing: {:?}", err);
            } else {
                println!("Flushed");
            }
        }
    })
    .layer(service_fn(
        move |event: LambdaEvent<ApiGatewayV2httpRequest>| {
            let state = Arc::clone(&state);
            let router = Arc::clone(&router);
            async move {
                let (event, context) = event.into_parts();
                router.handle_request(event, context, state).await
            }
        },
    ));

    Runtime::new(service).run().await
}
