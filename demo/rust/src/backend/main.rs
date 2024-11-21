use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use aws_sdk_dynamodb::{types::AttributeValue, Client as DynamoDbClient};
use lambda_otel_utils::{HttpOtelLayer, HttpTracerProviderBuilder};
use lambda_runtime::{
    service_fn, tower::ServiceBuilder, Error as LambdaError, LambdaEvent, Runtime,
};
use lazy_static::lazy_static;
use opentelemetry::trace::SpanKind;
use serde_dynamo::{from_item, to_attribute_value};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::sync::Arc;
use tracing::{instrument, Instrument};

use lambda_lw_http_router::{define_router, route};

macro_rules! dynamodb_span {
    ($request:expr, $operation:expr) => {{
        let table_name = ($request).as_input().get_table_name()
            .as_deref()
            .map(|t| vec![t])
            .unwrap_or_default();

        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let endpoint = format!("dynamodb.{}.amazonaws.com", region);
        let span_name = format!("DynamoDB.{}", $operation);

        tracing::info_span!(
            $operation,

            // Required DB semantic conventions
            "db.system" = "dynamodb",
            "db.operation" = $operation,

            // Network attributes
            "net.peer.name" = endpoint,

            // AWS specific attributes
            "aws.region" = region,
            "cloud.provider" = "aws",
            "cloud.region" = region,

            // Required span attributes
            "otel.kind" = ?SpanKind::Client,
            "otel.name" = span_name,

            // RPC attributes
            "rpc.system" = "aws-api",
            "rpc.service" = "DynamoDB",
            "rpc.method" = $operation,

            // Keep these for additional context
            "aws.dynamodb.table_names" = ?table_name
        )
    }};
}

lazy_static! {
    static ref TABLE_NAME: String = env::var("TABLE_NAME").expect("TABLE_NAME must be set");
}

#[derive(Clone)]
struct AppState {
    dynamodb_client: DynamoDbClient,
}

define_router!(event = ApiGatewayProxyRequest, state = AppState);

#[route(path = "/quotes", method = "POST")]
async fn handle_post_quotes(ctx: RouteContext) -> Result<Value, LambdaError> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let post_body = match ctx.event.body.as_ref() {
        Some(body) => match serde_json::from_str(body) {
            Ok(parsed_body) => parsed_body,
            Err(e) => {
                return Ok(json!({
                    "statusCode": 400,
                    "headers": {"Content-Type": "application/json"},
                    "body": json!({
                        "error": "Invalid JSON in request body",
                        "details": e.to_string()
                    }).to_string()
                }))
            }
        },
        None => {
            return Ok(json!({
                "statusCode": 400,
                "headers": {"Content-Type": "application/json"},
                "body": json!({
                    "error": "Missing request body"
                }).to_string()
            }))
        }
    };

    let id = {
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_string(&post_body)?.as_bytes());
        format!("{:x}", hasher.finalize())
    };

    write_item(&id, &timestamp, &post_body, &ctx.state).await?;

    Ok(json!({
        "statusCode": 200,
        "headers": {"Content-Type": "application/json"},
        "body": serde_json::to_string(&json!({
            "message": "Item successfully written to DynamoDB",
            "received": post_body,
            "timestamp": timestamp
        }))?
    }))
}

#[route(path = "/quotes", method = "GET")]
async fn handle_get_quotes(ctx: RouteContext) -> Result<Value, LambdaError> {
    let items = list_items(&ctx.state).await?;
    Ok(json!({
        "statusCode": 200,
        "headers": {"Content-Type": "application/json"},
        "body": serde_json::to_string(&items)?
    }))
}

#[route(path = "/quotes/{id}", method = "GET")]
async fn handle_get_quote(ctx: RouteContext) -> Result<Value, LambdaError> {
    let id = ctx.params.get("id").expect("id parameter is required");
    let item = read_item(id, &ctx.state).await?;

    match item {
        Some(value) => Ok(json!({
            "statusCode": 200,
            "headers": {"Content-Type": "application/json"},
            "body": serde_json::to_string(&value)?
        })),
        None => Ok(json!({
            "statusCode": 404,
            "headers": {"Content-Type": "application/json"},
            "body": json!({"message": "Item not found"}).to_string()
        })),
    }
}

#[instrument(skip(state))]
async fn write_item(
    pk: &str,
    timestamp: &str,
    payload: &Option<Value>,
    state: &AppState,
) -> Result<(), anyhow::Error> {
    let mut request = state
        .dynamodb_client
        .put_item()
        .table_name(TABLE_NAME.clone())
        .item("pk", AttributeValue::S(pk.to_string()))
        .item(
            "expiry",
            AttributeValue::N((chrono::Utc::now().timestamp() + 86400).to_string()),
        )
        .item("timestamp", AttributeValue::S(timestamp.to_string()));

    if let Some(payload_value) = payload {
        let attribute_value = to_attribute_value(payload_value)?;
        request = request.item("payload", attribute_value);
    }

    let span = dynamodb_span!(&request, "PutItem");
    request
        .send()
        .instrument(span)
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from)
}

#[instrument(skip(state))]
async fn read_item(pk: &str, state: &AppState) -> Result<Option<Value>, anyhow::Error> {
    let request = state
        .dynamodb_client
        .get_item()
        .table_name(TABLE_NAME.clone())
        .key("pk", AttributeValue::S(pk.to_string()));
    let span = dynamodb_span!(&request, "GetItem");
    let response = request.send().instrument(span).await?;
    if let Some(item) = response.item {
        from_item(item).map_err(anyhow::Error::from).map(Some)
    } else {
        tracing::info!(name = "item not found", pk = %pk, table = TABLE_NAME.as_str());
        Ok(None)
    }
}

#[instrument(skip(state))]
async fn list_items(state: &AppState) -> Result<Vec<Value>, LambdaError> {
    let request = state.dynamodb_client.scan().table_name(TABLE_NAME.clone());
    let span = dynamodb_span!(&request, "Scan");
    let scan_output = request
        .send()
        .instrument(span)
        .await
        .map_err(|e| LambdaError::from(format!("Failed to scan DynamoDB table: {}", e)))?;

    let items = scan_output
        .items
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_dynamo::from_item(item).map_err(|e| {
                LambdaError::from(format!("Failed to deserialize DynamoDB item: {}", e))
            })
        })
        .collect::<Result<Vec<Value>, LambdaError>>()?;

    let item_count = items.len();

    tracing::info!(
        name = "retrieved items from dynamo",
        item_count = item_count,
        table = TABLE_NAME.as_str()
    );

    Ok(items)
}

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("server")
        .with_default_text_map_propagator()
        .enable_global(true)
        .enable_fmt_layer(true)
        .with_batch_exporter()
        .build()?;

    let aws_config = aws_config::load_from_env().await;
    let dynamodb_client = DynamoDbClient::new(&aws_config);

    let state = Arc::new(AppState { dynamodb_client });
    let router = Arc::new(RouterBuilder::from_registry().build());

    // Build the service with OpenTelemetry instrumentation
    let service = ServiceBuilder::new()
        .layer(HttpOtelLayer::new(|| {
            tracer_provider.force_flush();
        }))
        .service(service_fn(
            move |event: LambdaEvent<ApiGatewayProxyRequest>| {
                let state = Arc::clone(&state);
                let router = Arc::clone(&router);
                async move { router.handle_request(event, state).await }
            },
        ));

    Runtime::new(service).run().await
}
