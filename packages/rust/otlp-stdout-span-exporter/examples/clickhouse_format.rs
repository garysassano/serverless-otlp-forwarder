use opentelemetry::trace::{Tracer, TracerProvider};
use opentelemetry_sdk::trace::SdkTracerProvider;
use otlp_stdout_span_exporter::{OtlpStdoutSpanExporter, OutputFormat};

#[tokio::main]
async fn main() {
    // Create a new stdout exporter with ClickHouse format
    let exporter = OtlpStdoutSpanExporter::with_format(OutputFormat::ClickHouse);

    // Create a new tracer provider with batch export
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    // Create a tracer
    let tracer = provider.tracer("hello-world");

    // Create spans
    tracer.in_span("parent-operation", |_cx| {
        println!("Doing work in parent span...");

        // Create nested spans
        tracer.in_span("child1", |_cx| {
            println!("Doing work in child1 span...");
        });

        tracer.in_span("child2", |_cx| {
            println!("Doing work in child2 span...");
        });
    });

    // Shut down the provider
    let _ = provider.shutdown();
}
