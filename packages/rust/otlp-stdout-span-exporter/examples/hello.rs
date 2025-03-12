use opentelemetry::{
    trace::{Tracer, TracerProvider},
    KeyValue,
};
use opentelemetry_sdk::{trace::TracerProvider as SdkTracerProvider, Resource};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Create a new stdout exporter with plain JSON output
    let exporter = OtlpStdoutSpanExporter::with_plain_json();

    // Create a new tracer provider with batch export
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            "hello-world",
        )]))
        .build();

    // Create a tracer
    let tracer = provider.tracer("hello-world");

    println!("Starting parent operation");
    tracer.in_span("parent-operation", |cx| {
        // Do some work in the parent span

        // Create child spans
        tracer.in_span("child1", |_| {
            println!("In child1");
        });

        tracer.in_span("child2", |_| {
            println!("In child2");
        });
    });
    println!("Finishing parent operation");

    // Shut down the provider to ensure all spans are exported
    provider.shutdown();
}

use opentelemetry::KeyValue;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;

#[tokio::main]
async fn main() {
    // Create a new stdout exporter with plain JSON output
    let exporter = OtlpStdoutSpanExporter::with_plain_json();

    // Create a new tracer provider with batch export
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::builder().add_service_name("hello-world").build())
        .build();

    // Create a tracer
    let tracer = provider.tracer("hello-world");

    println!("Starting parent operation");
    tracer.in_span("parent-operation", |_cx| {
        // Create child spans
        tracer.in_span("child1", |_| {
            println!("In child1");
        });

        tracer.in_span("child2", |_| {
            println!("In child2");
        });
    });
    println!("Finishing parent operation");

    // Shut down the provider to ensure all spans are exported
    provider.shutdown();
}
