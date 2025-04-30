#![allow(clippy::needless_return)] // Keep the style consistent

use crate::types::ProcessorInput; // Use the enum from the new types module
use lambda_extension::tracing;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

// Define the pipe path constant (can be moved to config or shared constants later)
const PIPE_PATH: &str = "/tmp/otlp-stdout-span-exporter.pipe";

/// Task to continuously read from the named pipe and send lines to the processor channel.
pub(crate) async fn pipe_reader_task(tx: mpsc::Sender<ProcessorInput>) {
    tracing::info!("Starting pipe reader task for {}", PIPE_PATH);
    let mut line_buffer = String::new();

    loop {
        // Attempt to open the pipe for reading
        match File::open(PIPE_PATH).await {
            Ok(pipe_file) => {
                tracing::info!("Named pipe opened successfully: {}", PIPE_PATH);
                let mut reader = BufReader::new(pipe_file);

                // Inner loop to read lines from the opened pipe
                loop {
                    match reader.read_line(&mut line_buffer).await {
                        Ok(0) => {
                            // EOF reached. Pipe might have been closed/recreated.
                            tracing::info!(
                                "EOF reached on named pipe {}, attempting to reopen...",
                                PIPE_PATH
                            );
                            break; // Break inner loop to reopen the pipe
                        }
                        Ok(_) => {
                            let line = line_buffer.trim_end();
                            if !line.is_empty() {
                                tracing::debug!("Read line from pipe: {}", line);
                                if let Err(e) = tx.send(ProcessorInput::OtlpJson(line.to_string())).await {
                                    tracing::error!("Failed to send OTLP JSON line to processor channel: {}", e);
                                    // If the receiver is dropped, the processor task likely terminated.
                                    // We should probably stop the pipe reader task too.
                                    tracing::error!("Processor channel closed, stopping pipe reader task.");
                                    return; // Exit the task
                                }
                            }
                            line_buffer.clear();
                        }
                        Err(e) => {
                            tracing::error!("Error reading line from named pipe {}: {}. Attempting to reopen...", PIPE_PATH, e);
                            break; // Break inner loop to reopen the pipe
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to open named pipe {}: {}. Retrying in 5 seconds...", PIPE_PATH, e);
                // Wait before retrying to open the pipe
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
} 