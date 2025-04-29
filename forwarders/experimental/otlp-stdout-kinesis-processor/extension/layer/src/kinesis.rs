use aws_sdk_kinesis::primitives::Blob;
use aws_sdk_kinesis::types::PutRecordsRequestEntry;
use lambda_extension::{tracing, Error};
use uuid::Uuid;

// Kinesis limit for a single record
pub const MAX_RECORD_SIZE_BYTES: usize = 1_048_576; // 1MB per record
pub const RECORD_PREFIX: &str = r#"{"__otel_otlp_stdout":"#;

#[derive(Default)]
pub struct KinesisBatch {
    pub records: Vec<PutRecordsRequestEntry>,
}

impl KinesisBatch {
    pub fn add_record(&mut self, record: String) -> Result<(), Error> {
        if record.len() > MAX_RECORD_SIZE_BYTES {
            tracing::warn!(
                "Record size {} bytes exceeds maximum size of {} bytes, skipping",
                record.len(),
                MAX_RECORD_SIZE_BYTES
            );
            return Ok(());
        }

        match PutRecordsRequestEntry::builder()
            .data(Blob::new(record))
            .partition_key(Uuid::new_v4().to_string())
            .build()
        {
            Ok(entry) => {
                self.records.push(entry);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to build Kinesis record entry: {}", e);
                Err(Error::from(e))
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}
