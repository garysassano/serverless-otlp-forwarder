use otlp_stdout_client::StdoutClient;
use opentelemetry_sdk::trace::{TracerProvider, Config};
use opentelemetry_otlp::new_exporter;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry::trace::Tracer;
use opentelemetry::global;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::KeyValue;
/// Initialize the tracer provider with the StdoutClient
fn init_tracer_provider() -> Result<TracerProvider, Box<dyn std::error::Error>> {
    let exporter = new_exporter()
        .http()
        .with_http_client(StdoutClient::default())
        .build_span_exporter()?;
    
    let tracer_provider = TracerProvider::builder()
        .with_config(Config::default())
        .with_batch_exporter(exporter, Tokio)
        .build();

    Ok(tracer_provider)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize and set the global tracer provider
    let tracer_provider = init_tracer_provider()?;
    global::set_tracer_provider(tracer_provider);
    
    // Get a tracer from the provider
    let tracer = global::tracer("basic-example");
    
    // Create a sample span
    tracer.in_span("hello_world", |cx| {
        // Emit an event with attributes
        cx.span().add_event(
            "span_started",
            vec![KeyValue::new("message", "Hello from inside the span!")],
        );
        
        // Create a nested span
        tracer.in_span("nested_operation", |cx| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            // Emit an event with attributes in the nested span
            cx.span().add_event(
                "operation_progress",
                vec![
                    KeyValue::new("message", "Doing some work in a nested span..."),
                    KeyValue::new("duration_ms", 100),
                ],
            );
        });
    });

    // Ensure all spans are exported before exit
    global::shutdown_tracer_provider();
    
    Ok(())
}
