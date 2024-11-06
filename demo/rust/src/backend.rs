use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use aws_sdk_dynamodb::{types::AttributeValue, Client as DynamoDbClient};
use lambda_otel_utils::HttpOtelLayer;
use lambda_otel_utils::HttpTracerProviderBuilder;
use lambda_runtime::{
    layers::{OpenTelemetryFaasTrigger, OpenTelemetryLayer},
    service_fn,
    tower::Layer,
    Error as LambdaError, LambdaEvent, Runtime,
};
use lazy_static::lazy_static;
use opentelemetry::trace::Status;
use opentelemetry::trace::TraceContextExt;
use serde_dynamo::{from_item, to_attribute_value};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::sync::Arc;
use tracing::instrument;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

macro_rules! dynamodb_span {
    ($method:expr, $table_names:expr) => {
        tracing::info_span!(
            "dynamodb_operation",
            rpc.system = "aws-api",
            rpc.service = "DynamoDB",
            rpc.method = $method,
            aws.dynamodb.table_names = ?$table_names,
            db.system = "dynamodb"
        )
    };
}

lazy_static! {
    static ref TABLE_NAME: String = env::var("TABLE_NAME").expect("TABLE_NAME must be set");
}

struct AppState {
    dynamodb_client: DynamoDbClient,
}

async fn handle_request(
    event: LambdaEvent<ApiGatewayProxyRequest>,
    state: &AppState,
) -> Result<Value, LambdaError> {
    let span = tracing::Span::current();
    let payload = event.payload;
    let method = payload.http_method.clone();

    let result = match method.as_str() {
        "POST" => {
            let timestamp = chrono::Utc::now().to_rfc3339();
            let post_body = match payload.body.as_ref() {
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

            write_item(&id, &timestamp, &post_body, state).await?;

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
        "GET" => match payload.path_parameters.get("id") {
            Some(id) => {
                let item = read_item(id, state).await?;
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
            None => {
                let items = list_items(state).await?;
                Ok(json!({
                    "statusCode": 200,
                    "headers": {"Content-Type": "application/json"},
                    "body": serde_json::to_string(&items)?
                }))
            }
        },
        _ => Err(LambdaError::from("Unsupported HTTP method")),
    };

    // record the error if any
    if let Err(e) = &result {
        let otel_context = span.context();
        otel_context.span().record_error(e.as_ref());
        otel_context.span().set_status(Status::error(e.to_string()));
    }
    result
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
        tracing::info!(
            name = "written item to dynamo",
            pk = %pk,
            timestamp = %timestamp,
            payload = %payload_value,
            table = TABLE_NAME.as_str()
        );
    }

    request
        .send()
        .instrument(dynamodb_span!("PutItem", [TABLE_NAME.as_str()]))
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

    let response = request
        .send()
        .instrument(dynamodb_span!("GetItem", [TABLE_NAME.as_str()]))
        .await?;
    if let Some(item) = response.item {
        let json_value: Value = from_item(item)?;

        let timestamp = json_value
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

        let payload = json_value
            .get("payload")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "missing".to_string());

        tracing::info!(
            name = "retrieved item from dynamo",
            pk = %pk,
            timestamp = %timestamp,
            payload = %payload,
            table = TABLE_NAME.as_str()
        );

        Ok(Some(json_value))
    } else {
        tracing::info!(name = "item not found", pk = %pk, table = TABLE_NAME.as_str());
        Ok(None)
    }
}

#[instrument(skip(state))]
async fn list_items(state: &AppState) -> Result<Vec<Value>, LambdaError> {
    let span = dynamodb_span!("Scan", [TABLE_NAME.as_str()]);

    let scan_output = state
        .dynamodb_client
        .scan()
        .table_name(TABLE_NAME.clone())
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

    let handler_with_state = {
        let state = Arc::clone(&state);
        move |event| {
            let state = Arc::clone(&state);
            async move { handle_request(event, &state).await }
        }
    };

    let service = HttpOtelLayer::new(|| {
        for result in tracer_provider.force_flush() {
            if let Err(err) = result {
                println!("Error flushing: {:?}", err);
            } else {
                println!("Flushed");
            }
        }
    }).layer(service_fn(handler_with_state));

    Runtime::new(service).run().await
}
