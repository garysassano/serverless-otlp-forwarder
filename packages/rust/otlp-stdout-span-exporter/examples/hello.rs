use opentelemetry::trace::{Tracer, TracerProvider};
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;

#[tokio::main]
async fn main() {
    // Create a new stdout exporter with plain JSON output
    let exporter = OtlpStdoutSpanExporter::with_plain_json();

    // Create a new tracer provider with batch export
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("hello-world").build())
        .build();

    // Create a tracer
    let tracer = provider.tracer("hello-world");

    // Shut down the provider to ensure all spans are exported
    let _ = provider.shutdown();
}
