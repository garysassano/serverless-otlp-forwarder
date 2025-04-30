use lambda_extension::{tracing, Error};
use std::env;

// Environment variable name for Kinesis stream
pub const ENV_VAR_STREAM_NAME: &str = "OTLP_STDOUT_KINESIS_STREAM_NAME";

// Default buffering values
pub const DEFAULT_BUFFER_TIMEOUT_MS: u32 = 100;
pub const DEFAULT_BUFFER_MAX_BYTES: usize = 256 * 1024; // 256KB
pub const DEFAULT_BUFFER_MAX_ITEMS: usize = 1000;

// Environment variable names for buffering config
pub const ENV_VAR_BUFFER_TIMEOUT_MS: &str = "OTLP_STDOUT_KINESIS_BUFFER_TIMEOUT_MS";
pub const ENV_VAR_BUFFER_MAX_BYTES: &str = "OTLP_STDOUT_KINESIS_BUFFER_MAX_BYTES";
pub const ENV_VAR_BUFFER_MAX_ITEMS: &str = "OTLP_STDOUT_KINESIS_BUFFER_MAX_ITEMS";

#[derive(Debug, Clone)]
pub struct Config {
    pub kinesis_stream_name: Option<String>,
    pub buffer_timeout_ms: u32,
    pub buffer_max_bytes: usize,
    pub buffer_max_items: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, Error> {
        let kinesis_stream_name = env::var(ENV_VAR_STREAM_NAME).ok();

        if kinesis_stream_name.is_none() {
            tracing::info!("{} not set, disabling Kinesis output. Will write OTLP JSON to stdout.", ENV_VAR_STREAM_NAME);
        } else {
            tracing::info!("Kinesis stream name set: {}", kinesis_stream_name.as_ref().unwrap());
        }

        let buffer_timeout_ms = env::var(ENV_VAR_BUFFER_TIMEOUT_MS)
            .map(|v| v.parse::<u32>().unwrap_or(DEFAULT_BUFFER_TIMEOUT_MS))
            .unwrap_or(DEFAULT_BUFFER_TIMEOUT_MS);

        let buffer_max_bytes = env::var(ENV_VAR_BUFFER_MAX_BYTES)
            .map(|v| v.parse::<usize>().unwrap_or(DEFAULT_BUFFER_MAX_BYTES))
            .unwrap_or(DEFAULT_BUFFER_MAX_BYTES);

        let buffer_max_items = env::var(ENV_VAR_BUFFER_MAX_ITEMS)
            .map(|v| v.parse::<usize>().unwrap_or(DEFAULT_BUFFER_MAX_ITEMS))
            .unwrap_or(DEFAULT_BUFFER_MAX_ITEMS);

        tracing::debug!(
            "Configuration: buffer_timeout_ms={}, buffer_max_bytes={}, buffer_max_items={}",
            buffer_timeout_ms,
            buffer_max_bytes,
            buffer_max_items
        );

        Ok(Self {
            kinesis_stream_name,
            buffer_timeout_ms,
            buffer_max_bytes,
            buffer_max_items,
        })
    }
}
