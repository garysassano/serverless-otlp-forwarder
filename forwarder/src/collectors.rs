//! Collector configuration management for the Lambda OTLP forwarder.
//! 
//! This module handles:
//! - Loading collector configurations from AWS Secrets Manager
//! - Matching log records to appropriate collectors
//! - Managing collector authentication details
//! 
//! Collectors are initialized once at startup and shared across all requests.

use std::sync::OnceLock;
use std::sync::Arc;
use anyhow::{Context, Result};
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use serde_json::Value;
use std::env;

/// Global storage for initialized collectors
static COLLECTORS: OnceLock<Arc<Collectors>> = OnceLock::new();

/// Represents a single collector configuration.
/// Each collector has a name, endpoint, and optional authentication details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Collector {
    /// Unique name identifying the collector
    pub(crate) name: String,
    /// Base URL endpoint for the collector
    pub(crate) endpoint: String,
    /// Optional authentication header in format "HeaderName=HeaderValue"
    pub(crate) auth: Option<String>,
}

/// Container for managing multiple collector configurations.
/// Provides thread-safe access to collector configurations and matching functionality.
#[derive(Debug)]
pub(crate) struct Collectors {
    items: Vec<Collector>,
}

impl Collectors {
    /// Creates a new Collectors instance with the provided items.
    fn new(items: Vec<Collector>) -> Self {
        Self { items }
    }

    /// Check if collectors are initialized globally.
    /// 
    /// Returns true if collectors have been successfully loaded and initialized.
    pub(crate) fn is_initialized() -> bool {
        COLLECTORS.get().is_some()
    }

    /// Initialize collectors from AWS Secrets Manager.
    /// 
    /// This method will:
    /// 1. Load collector configurations from Secrets Manager
    /// 2. Initialize the global collectors instance if not already initialized
    /// 3. Make collectors available for matching log records
    #[instrument(skip(client))]
    pub(crate) async fn init(client: &SecretsManagerClient) -> Result<()> {
        if !Self::is_initialized() {
            let items = fetch_collectors(client).await?;
            let collectors = Arc::new(Collectors::new(items));
            COLLECTORS
                .set(collectors)
                .map_err(|_| anyhow::anyhow!("Collectors already initialized"))?;
            
            tracing::info!("Initialized {} collectors", 
                COLLECTORS.get().unwrap().items.len());
        }
        Ok(())
    }

    /// Finds a collector matching the given endpoint
    #[instrument(skip_all)]
    pub(crate) fn find_matching(endpoint: &str) -> Option<Collector> {
        let collectors = COLLECTORS.get().expect("Collectors not initialized");
        collectors
            .items
            .iter()
            .find(|c| endpoint.starts_with(&c.endpoint))
            .cloned()
    }
}

/// Fetches collectors configuration from AWS Secrets Manager
#[instrument(skip(client))]
async fn fetch_collectors(client: &SecretsManagerClient) -> Result<Vec<Collector>> {
    let secret_arn = env::var("COLLECTORS_SECRET_ARN")
        .context("COLLECTORS_SECRET_ARN must be set")?;
    
    let secret = client
        .get_secret_value()
        .secret_id(secret_arn)
        .send()
        .await?;
        
    let secret_string = secret.secret_string()
        .context("Secret string not found")?;
        
    let secret_json: Value = serde_json::from_str(&secret_string)
        .context("Invalid JSON in secret")?;
    
    tracing::info!("Loading collectors from secret manager");
    
    serde_json::from_value(secret_json["collectors"].clone())
        .context("Failed to parse collectors from secret")
}
