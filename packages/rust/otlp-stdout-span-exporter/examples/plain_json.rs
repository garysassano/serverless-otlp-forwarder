use opentelemetry::trace::{Tracer, TracerProvider};
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;

#[tokio::main]
async fn main() {
    // Create a new stdout exporter (now always outputs plain JSON)
    let exporter = OtlpStdoutSpanExporter::new();

    // Create a new tracer provider with batch export
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("hello-world").build())
        .build();

    // Create a tracer
    let tracer = provider.tracer("hello-world");

    // Create spans without any println statements
    tracer.in_span("parent-operation", |_cx| {
        // Create child spans
        tracer.in_span("child1", |_| {});
        tracer.in_span("child2", |_| {});
    });

    // Shut down the provider to ensure all spans are exported
    let _ = provider.shutdown();
}
