use otlp_stdout_span_exporter::clickhouse_formatter::transform_otlp_to_clickhouse;
use std::env;
use std::fs;
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get input file from command line arguments or read from stdin
    let input = if let Some(file_path) = env::args().nth(1) {
        fs::read_to_string(file_path)?
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    };

    // Transform OTLP JSON to ClickHouse format
    let clickhouse_json = transform_otlp_to_clickhouse(&input)?;

    // Print the result to stdout
    println!("{}", clickhouse_json);

    Ok(())
}
