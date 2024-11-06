//! Header management for the Lambda OTLP forwarder.
//! 
//! This module handles two types of headers:
//! - Environment-based headers from OTEL configuration
//! - Log record specific headers for forwarding requests
//! 
//! The headers are used when forwarding log records to their respective collectors.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use tracing::instrument;
use std::sync::Arc;

use otlp_stdout_client::{LogRecord, CONTENT_ENCODING_HEADER, CONTENT_TYPE_HEADER};

/// Headers builder for outgoing log record requests.
/// Uses the builder pattern to construct the final set of headers
/// from various sources (environment, log record, collector auth).
pub(crate) struct LogRecordHeaders(HeaderMap);

impl LogRecordHeaders {
    /// Creates a new empty set of headers.
    pub(crate) fn new() -> Self {
        LogRecordHeaders(HeaderMap::new())
    }

    /// Adds environment-based headers to the request.
    pub(crate) fn with_env_headers(mut self, env_headers: &EnvHeaders) -> Self {
        self.0.extend(env_headers.0.clone());
        self
    }

    /// Adds headers from the log record itself.
    /// This includes both custom headers and content-type/encoding headers.
    pub(crate) fn with_log_record(mut self, log_record: &LogRecord) -> Result<Self> {
        self.extract_headers(&log_record.headers)?;
        self.add_content_headers(log_record)?;
        Ok(self)
    }

    /// Adds authentication headers from the collector configuration.
    /// The auth string should be in the format "HeaderName=HeaderValue".
    pub(crate) fn with_collector_auth(mut self, auth: &Option<String>) -> Result<Self> {
        if let Some(auth) = auth {
            let (header_name, header_value) = auth
                .split_once('=')
                .context("Invalid auth format in collector config")?;
            self.0.insert(
                HeaderName::from_str(header_name)?,
                HeaderValue::from_str(header_value)?,
            );
        }
        Ok(self)
    }

    /// Finalizes the headers and returns the underlying HeaderMap.
    pub(crate) fn build(self) -> HeaderMap {
        self.0
    }

    /// Helper method to extract and normalize custom headers from a HashMap
    fn extract_headers(&mut self, headers: &HashMap<String, String>) -> Result<()> {
        for (key, value) in headers {
            let normalized_key = key.to_lowercase();
            let header_name = HeaderName::from_str(&normalized_key)
                .with_context(|| format!("Invalid header name: {}", normalized_key))?;
            let header_value = HeaderValue::from_str(value).with_context(|| {
                format!("Invalid header value for {}: {}", normalized_key, value)
            })?;

            self.0.insert(header_name, header_value);
        }

        Ok(())
    }

    /// Helper method to add content-type and content-encoding headers
    fn add_content_headers(&mut self, log_record: &LogRecord) -> Result<()> {
        if !log_record.content_type.is_empty() {
            self.0.insert(
                HeaderName::from_static(CONTENT_TYPE_HEADER),
                HeaderValue::from_str(&log_record.content_type)?,
            );
            tracing::debug!(
                "Added/Updated content-type header: {}",
                log_record.content_type
            );
        }
        if let Some(content_encoding) = &log_record.content_encoding {
            self.0.insert(
                HeaderName::from_static(CONTENT_ENCODING_HEADER),
                HeaderValue::from_str(content_encoding)?,
            );
        }
        Ok(())
    }
}

/// Environment-based headers loaded from OTEL configuration.
/// These headers are loaded once at startup and shared across all requests.
pub(crate) struct EnvHeaders(HeaderMap);

impl EnvHeaders {
    /// Creates a new EnvHeaders instance by reading from OTEL_EXPORTER_OTLP_HEADERS.
    /// 
    /// The environment variable should contain comma-separated key=value pairs, e.g.:
    /// "header1=value1,header2=value2"
    /// 
    /// Returns an Arc-wrapped instance for thread-safe sharing across async tasks.
    /// 
    /// # Errors
    /// 
    /// Returns an error if the headers are malformed or cannot be parsed.
    #[instrument]
    pub(crate) fn from_env() -> Result<Arc<Self>, anyhow::Error> {
        let mut headers = HeaderMap::new();
        if let Ok(env_headers) = env::var("OTEL_EXPORTER_OTLP_HEADERS") {
            tracing::debug!("Parsing OTEL headers from environment");
            for header_pair in env_headers.split(',') {
                let parts: Vec<&str> = header_pair.split('=').collect();
                if parts.len() != 2 {
                    tracing::warn!("Invalid header pair: {}", header_pair);
                    continue;
                }

                let (name, value) = (parts[0].trim(), parts[1].trim());

                if let Ok(header_name) = HeaderName::from_str(name) {
                    if let Ok(header_value) = HeaderValue::from_str(value) {
                        headers.insert(header_name, header_value);
                        tracing::debug!("Added header from env: {}={}", name, value);
                    } else {
                        tracing::warn!("Invalid header value from env: {}", value);
                    }
                } else {
                    tracing::warn!("Invalid header name from env: {}", name);
                }
            }
        }
        Ok(Arc::new(EnvHeaders(headers)))
    }
}
