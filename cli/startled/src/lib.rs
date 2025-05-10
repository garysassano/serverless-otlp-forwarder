#![doc = include_str!("../README.md")]
//! `startled` is a Command Line Interface (CLI) tool designed for comprehensive benchmarking
//! of AWS Lambda functions. It provides insights into performance, cold starts, and invocation durations.

pub mod benchmark;
pub mod console;
pub mod lambda;
pub mod report;
pub mod screenshot;
pub mod stats;
pub mod telemetry;
pub mod types;
pub mod utils;
