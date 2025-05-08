# OTLP Stdout livetrace

`livetrace` is a command-line tool designed to enhance local development workflows when working with distributed tracing in serverless environments, particularly those following the **OTLP-stdout Forwarder Architecture**.

## Overview

In architectures where Lambda functions (or other ephemeral compute) log OpenTelemetry (OTLP) trace data to standard output, which is then captured by services like CloudWatch Logs, correlating and visualizing a complete trace during development can be challenging. Logs for different services involved in a single request might be spread across multiple Log Groups.

`livetrace` addresses this by:

1.  **Discovering** relevant CloudWatch Log Groups based on naming patterns or CloudFormation stack resources.
2.  **Validating** the existence of these Log Groups, intelligently handling standard Lambda and Lambda@Edge naming conventions.
3.  **Tailing** or **Polling** these Log Groups simultaneously using either the efficient `StartLiveTail` API or the `FilterLogEvents` API.
4.  **Parsing** OTLP trace data embedded within log messages in the format produced by the _otlp-stdout-span-exporter_ ([npm](https://www.npmjs.com/package/@dev7a/otlp-stdout-span-exporter), [pypi](https://pypi.org/project/otlp-stdout-span-exporter/), [crates.io](https://crates.io/crates/otlp-stdout-span-exporter)).
5.  **Displaying** traces in a user-friendly waterfall view directly in your terminal, including service names, durations, and timelines.
6.  **Showing** span events associated with the trace.
7.  **Optionally forwarding** the raw OTLP protobuf data to a specified OTLP-compatible endpoint (like a local OpenTelemetry Collector or Jaeger instance).

It acts as a local observability companion, giving you immediate feedback on trace behavior without needing to navigate the AWS console or wait for logs to propagate fully to a backend system.

## Features

*   **CloudWatch Log Tailing:** Stream logs in near real-time using `StartLiveTail`.
*   **CloudWatch Log Polling:** Periodically fetch logs using `FilterLogEvents` with `--poll-interval`.
*   **Flexible Log Group Discovery:**
    *   Find log groups matching one or more patterns (`--log-group-pattern`).
    *   Find log groups belonging to a CloudFormation stack (`--stack-name`), including implicitly created Lambda log groups.
    *   **Combine pattern and stack discovery:** Use both options simultaneously to aggregate log groups.
*   **Log Group Validation:** Checks existence and handles Lambda@Edge naming conventions (`/aws/lambda/us-east-1.<function-name>`).
*   **OTLP/stdout Parsing:** Decodes trace data logged via the `otlp-stdout-span-exporter` format (JSON wrapping base64-encoded, gzipped OTLP protobuf).
*   **Console Trace Visualization:**
    *   Waterfall view showing span hierarchy, service names, durations, and relative timing.
    *   Display of span kind (SERVER, CLIENT, etc.) and important span attributes in the waterfall.
    *   Configurable color themes for better service differentiation (`--theme`, `--list-themes`).
*   **Console Event Display:** Lists span events with timestamps, service names, and optional attribute filtering including both event and related span attributes.
*   **OTLP Forwarding:** Optionally send processed trace data to an OTLP HTTP endpoint (`-e`, `-H`, Environment Variables).
*   **Configuration:**
    *   AWS Region/Profile support.
    *   OTLP endpoint and headers configurable via CLI args or standard OTel environment variables.
    *   Session timeout for Live Tail mode (`--session-timeout`).
    *   Configuration profiles (`.livetrace.toml`) for saving common settings (`--config-profile`, `--save-profile`).
*   **User Experience:**
    *   **Detailed Startup Preamble:** Shows a summary of the effective configuration (AWS details, discovery sources, modes, forwarding, display settings, log groups).
    *   **Interactive Spinner:** Displays a spinner while waiting for events.
    *   **Verbosity Control:** Adjust logging detail (`-v`, `-vv`, `-vvv`).

## Installation

### Prerequisites

*   Rust toolchain (latest stable recommended)
*   AWS Credentials configured (via environment variables, shared credentials file, etc.) accessible to the tool.

### From Source

```bash
# Clone the repository (if you haven't already)
# git clone <repository-url>
# cd <repository-path>

# Build and install the livetrace binary
cargo install --path cli/livetrace
```

## Usage

```bash
livetrace [OPTIONS]
```

### Discovery Options (One or Both Required)

You must specify at least one of the following to identify the log groups. They can be used together:

*   `--log-group-pattern <PATTERN>...`: Discover log groups whose names contain *any* of the given patterns (case-sensitive substring search). Can be specified multiple times, or provide multiple patterns after the flag.
    ```bash
    # Single pattern
    livetrace --log-group-pattern "/aws/lambda/my-app-"
    # Multiple patterns
    livetrace --log-group-pattern "/aws/lambda/service-a-" "/aws/lambda/service-b-"
    livetrace --log-group-pattern "pattern1" --log-group-pattern "pattern2"
    ```
*   `--stack-name <STACK_NAME>`: Discover log groups associated with resources (`AWS::Logs::LogGroup`, `AWS::Lambda::Function`) in the specified CloudFormation stack.
    ```bash
    livetrace --stack-name my-production-stack
    ```
*   **Combining:**
    ```bash
    # Find groups in a stack AND those matching a pattern
    livetrace --stack-name my-api-stack --log-group-pattern "/aws/lambda/auth-"
    ```

### Mode Selection (Optional, Mutually Exclusive Group)

You can specify *at most one* of the following:

*   `--poll-interval <SECONDS>`: Use the `FilterLogEvents` API instead of `StartLiveTail`, polling every specified number of seconds.
    ```bash
    # Poll every 15 seconds
    livetrace --stack-name my-dev-stack --poll-interval 15
    ```
*   `--session-timeout <MINUTES>`: (Default: 30) Automatically exit after the specified number of minutes. **Only applicable in Live Tail mode (when `--poll-interval` is *not* used).**
    ```bash
    # Use Live Tail, but exit after 60 minutes
    livetrace --pattern "my-service-" --session-timeout 60
    ```

### OTLP Forwarding (Optional)

Configure forwarding to send traces to another OTLP receiver:

*   `-e, --otlp-endpoint <URL>`: The base HTTP URL for the OTLP receiver (e.g., `http://localhost:4318`). `/v1/traces` will be appended automatically if no path is present.
*   `-H, --otlp-header <KEY=VALUE>`: Add custom HTTP headers (e.g., for authentication). Can be specified multiple times.

**Environment Variables for Forwarding:**

You can also configure the endpoint and headers using standard OpenTelemetry environment variables. The precedence order is:

1.  Command-line arguments (`-e`, `-H`)
2.  Signal-specific environment variables (`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`, `OTEL_EXPORTER_OTLP_TRACES_HEADERS`)
3.  General OTLP environment variables (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_HEADERS`)

*   `OTEL_EXPORTER_OTLP_ENDPOINT=<URL>` / `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=<URL>`: Base URL for the receiver.
*   `OTEL_EXPORTER_OTLP_HEADERS=<KEY1=VAL1,KEY2=VAL2...>` / `OTEL_EXPORTER_OTLP_TRACES_HEADERS=<KEY1=VAL1,KEY2=VAL2...>`: Comma-separated list of key-value pairs for headers.

```bash
# Forward using CLI args
livetrace --stack-name my-stack -e http://localhost:4318 -H "Authorization=Bearer mytoken"

# Forward using environment variables
export OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318
export OTEL_EXPORTER_OTLP_HEADERS="x-api-key=secret123,x-tenant-id=abc"
livetrace --stack-name my-stack
```

### Console Display Options (Optional)

Control the appearance of the console output:

*   `--theme <THEME>`: Select a color theme (e.g., `default`, `tableau`, `monochrome`). Default is `default`.
*   `--list-themes`: List all available color themes with descriptions and exit.
*   `--attrs <GLOB_LIST>`: Comma-separated list of glob patterns (e.g., `"http.*,db.statement,my.custom.*"`) to filter which attributes are displayed. Applied to both span attributes and event attributes. If omitted, all attributes are shown.
*   `--event-severity-attribute <ATTRIBUTE_NAME>`: (Default: `event.severity`) Specify the event attribute key used to determine the severity level for coloring event output.
*   `--events-only`: Only display events in the timeline log, hiding span start information.
*   `--trace-timeout <SECONDS>`: (Default: 5) Maximum time in seconds to wait for spans belonging to a trace before displaying/forwarding it, even if the root span hasn't been received.

### Other Options

*   `--aws-region <AWS_REGION>`: Specify the AWS Region. Defaults to environment/profile configuration.
*   `--aws-profile <AWS_PROFILE>`: Specify the AWS profile name.
*   `-v, -vv, -vvv`: Increase logging verbosity (Info -> Debug -> Trace). Internal logs go to stderr.
*   `--forward-only`: Only forward telemetry via OTLP; do not display traces/events in the console. Requires an endpoint to be configured.
*   `--config-profile <PROFILE_NAME>`: Load configuration from a named profile in `.livetrace.toml`.
*   `--save-profile <PROFILE_NAME>`: Save the current command-line arguments as a named profile in `.livetrace.toml`.

## Console Output

When running in console mode (`--forward-only` not specified), `livetrace` displays:

1.  **Configuration Preamble:** Shows a detailed summary of the effective configuration being used, including AWS details, discovery sources, mode, forwarding settings, display options, and the final list of log groups being monitored.
2.  **Spinner:** An animated spinner indicates when the tool is actively waiting for new log events.
3.  **Trace Waterfall:** For each trace received:
    *   A header `─ Trace ID: <trace_id> ───────────`
    *   A table showing:
        *   Service Name
        *   Span Name (indented based on parent-child relationship)
        *   Span Kind (SERVER, CLIENT, etc.)
        *   Status (OK, ERROR)
        *   Duration (ms)
        *   Span ID (shortened to 8 characters)
        *   Timeline bar visualization (colored based on --color-by setting)
4.  **Timeline Log:** For each trace received:
    *   A header `─ Timeline Log for Trace: <trace_id> ─────` (or `─ Events for Trace: <trace_id> ─────` if `--events-only` is used)
    *   A chronological list of span starts and events showing:
        *   Timestamp (colored dimmed)
        *   Span ID (shortened to 8 characters, colored based on --color-by setting)
        *   Service Name (in square brackets)
        *   Type tag (`[SPAN]` or `[EVENT]`) - `[SPAN]` entries are hidden if `--events-only` is used
        *   Status/Level (colored appropriately: green for OK, red for ERROR, etc.)
        *   Name (Span name or Event name)
        *   Attributes (if present): filtered by `--attrs` if provided

## Configuration Profiles

`livetrace` supports saving and loading configuration profiles to reduce typing for frequently used commands. Profiles are stored in a `.livetrace.toml` file in the current directory.

### Saving a Profile

To save your current command-line options as a named profile:

```bash
# Save the current settings as "dev-profile"
livetrace --pattern "my-service-" --timeline-width 120 --attrs "http.*" --save-profile dev-profile
```

### Using a Profile

To use a saved profile:

```bash
# Load settings from the "dev-profile"
livetrace --config-profile dev-profile
```

You can also override specific settings from the profile by providing additional command-line arguments:

```bash
# Load from profile but override the attribute filter
livetrace --config-profile dev-profile --attrs "db.*,aws.*"
```

### Configuration File Format

The `.livetrace.toml` file follows this structure:

```toml
version = 0.0

# Global settings applied to all profiles
[global]
aws-region = "us-east-1"
event-severity-attribute = "event.severity"

# Profile-specific settings
[profiles.dev-profile]
log-group-pattern = ["my-service-"]
timeline-width = 120
attrs = "http.*"
theme = "solarized"
events-only = true
trace-timeout = 10

[profiles.prod-profile]
stack-name = "production-stack"
forward-only = true
otlp-endpoint = "http://localhost:4318"
```

This file is meant to be local to your project or environment and should typically not be committed to version control.

## Development

```bash
# Build
cargo build -p livetrace

# Run tests
cargo test -p livetrace

# Run clippy checks
cargo clippy -p livetrace -- -D warnings
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.