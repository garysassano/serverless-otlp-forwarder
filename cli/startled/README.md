# `startled` CLI

**Know your overhead. Fear no extension.**

`startled` (Start Time And Response Timing Latency Evaluation & Diagnostics) is a command-line tool for detailed performance analysis of AWS Lambda functions. It provides comprehensive data on cold starts, warm invocations, memory usage, and critically, the performance impact of Lambda Extensions. This makes it an effective utility for evaluating OpenTelemetry (Otel) solutions, custom layers, and other components that integrate with the Lambda execution environment.

## Key Features

-   **Flexible Benchmarking Targets**:
    -   Analyze individual Lambda functions by name or ARN.
    -   Benchmark a selection of functions within a CloudFormation stack, filterable by regular expression.
-   **Detailed Performance Metrics**:
    -   **Cold Starts**: Captures initialization duration (`initDuration`), execution duration, and total cold start time.
    -   **Warm Starts**: Measures execution duration for initialized environments.
    -   **Extension Overhead**: Extracts the `extensionOverhead` value reported in Lambda platform logs, providing insight into the performance characteristics of Lambda Extensions.
    -   **Client-Side Duration**: Measures invocation duration from the client's perspective through two modes:
        -   **Direct Measurement**: The CLI records the duration of the AWS SDK invocation call.
        -   **Proxied Measurement**: Utilizes a user-deployed proxy Lambda function within AWS to achieve more precise in-network client-side timings, reducing the influence of local network latency.
    -   **Resource Usage**: Reports billed duration and maximum memory used during invocations.
-   **Configurable Benchmark Parameters**:
    -   Temporarily adjust a Lambda function's **memory allocation** for specific benchmark scenarios.
    -   Control the number of **concurrent invocations** to simulate different load levels.
    -   Specify the number of **rounds/repetitions** for warm start analysis.
    -   Send custom **JSON payloads** with each invocation, either as a command-line string or from a file.
    -   Set temporary **environment variables** for the Lambda function during the benchmark.
-   **Comprehensive HTML Reports**:
    -   Generates detailed HTML reports featuring interactive charts (using Apache ECharts) for clear visualization of benchmark data.
    -   Provides statistical summaries (Average, P50, P95, P99) for key metrics across different functions and configurations.
    -   Includes scatter plots to visualize client duration over time for warm starts, helping to identify trends or outliers.
    -   Saves raw benchmark data in **JSON format** for custom analysis or integration with other tools.
    -   Supports custom templates, allowing users to completely customize the report appearance and behavior.
-   **Traceability Support**:
    -   Automatically injects **OpenTelemetry and AWS X-Ray trace context headers** into the Lambda payload, facilitating distributed tracing across the CLI and the benchmarked functions.
-   **Safe and Reversible Operation**:
    -   Captures a Lambda function's original configuration (memory, environment variables) before applying temporary changes for a benchmark.
    -   **Restores the original configuration** after the benchmark concludes or if interrupted.
-   **Optional Chart Screenshots**:
    -   Can generate PNG screenshots of the report charts (requires the `screenshots` compile-time feature and a headless Chrome environment).

## Prerequisites

-   **Rust and Cargo**: Necessary for building and installing `startled` from source. ([Install Rust](https://www.rust-lang.org/tools/install))
-   **AWS CLI**: Must be configured with appropriate credentials and permissions for AWS Lambda and CloudFormation interactions. ([Configure AWS CLI](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-quickstart.html))
-   **Docker**: (If using the `benchmark/testbed/` or building Lambda functions with SAM) SAM CLI relies on Docker for packaging Lambda deployment artifacts.

## Installation

`startled` is installed from source using Cargo:

1.  Navigate to the `benchmark/` directory containing the `startled` source code:
    ```bash
    cd path/to/your/repository/benchmark/
    ```
2.  Install using Cargo:
    ```bash
    cargo install --path .
    ```
    This command builds the `startled` executable and installs it into your Cargo binary directory (typically `~/.cargo/bin/`). Ensure this directory is in your system's PATH.

## Usage

The CLI is invoked using the following general syntax:

```bash
startled <COMMAND> [OPTIONS]
```

### Commands

#### 1. `function`

Benchmarks a single, specified Lambda function.

**Syntax:**
`startled function <FUNCTION_NAME> [OPTIONS]`

**Key Options:**
-   `<FUNCTION_NAME>`: (Required) The name or ARN of the Lambda function to be benchmarked.
-   `--memory <MB>` (`-m <MB>`): Temporarily sets the function's memory allocation to `<MB>` for the benchmark duration.
-   `--concurrent <N>` (`-c <N>`): Specifies the number of concurrent invocations (default: 1).
-   `--rounds <N>` (`-n <N>`): Sets the number of repetitions for warm start measurements. Each round consists of `--concurrent` invocations (default: 1).
-   `--payload <JSON_STRING>`: Provides a JSON payload string for each invocation. Conflicts with `--payload-file`.
-   `--payload-file <PATH>`: Specifies the path to a JSON file containing the payload. Conflicts with `--payload`.
-   `--env <KEY=VALUE>` (`-e <KEY=VALUE>`): Sets an environment variable for the function during the benchmark. This option can be used multiple times.
-   `--proxy <PROXY_FUNCTION_NAME_OR_ARN>`: Specifies the name or ARN of a proxy Lambda function for client-side duration measurements.
-   `--output-dir <PATH>` (`-d <PATH>`): Directory where raw JSON benchmark results will be saved. Results are typically organized as `<PATH>/{memory_size_if_set}/{function_name}.json`.

**Example:**
```bash
startled function my-lambda-function \
    --memory 512 \
    --concurrent 10 \
    --rounds 100 \
    --payload '{\"request_id\":\"123\"}' \
    --env LOG_LEVEL=info \
    --proxy arn:aws:lambda:us-east-1:123456789012:function:my-benchmark-proxy \
    --output-dir /tmp/startled_results/my-lambda-function
```

#### 2. `stack`

Benchmarks Lambda functions defined within a specified AWS CloudFormation stack.

**Syntax:**
`startled stack <STACK_NAME> --select <PATTERN> [OPTIONS]`

**Key Options:**
-   `<STACK_NAME>`: (Required) The name of the deployed CloudFormation stack.
-   `--select <PATTERN>` (`-s <PATTERN>`): (Required) A simple string pattern for substring matching against function names or ARNs within the stack. This pattern is also used to name a subdirectory for the results unless `--select-name` is provided. The pattern must be filesystem-safe if used for directory naming (alphanumeric, underscores, hyphens).
-   `--select-regex <REGEX>`: (Optional) A regular expression to filter functions within the stack. If provided, this regex is used for filtering instead of the `--select <PATTERN>`. This option does not affect directory naming.
-   `--select-name <NAME>`: (Optional) Specifies a custom name for the subdirectory where results for this selection group will be stored. If provided, this name overrides the `--select <PATTERN>` for directory naming purposes. The name must be filesystem-safe (alphanumeric, underscores, hyphens).
-   `--memory <MB>` (`-m <MB>`): Temporarily sets memory for all selected functions.
-   `--concurrent <N>` (`-c <N>`): Number of concurrent invocations (default: 1).
-   `--rounds <N>` (`-n <N>`): Number of warm start repetitions (default: 1).
-   `--payload <JSON_STRING>` / `--payload-file <PATH>`: Payload for invocations, applied to all selected functions.
-   `--env <KEY=VALUE>` (`-e <KEY=VALUE>`): Environment variables for selected functions.
-   `--proxy <PROXY_FUNCTION_NAME_OR_ARN>`: Proxy Lambda for client-side measurements.
-   `--output-dir <PATH>` (`-d <PATH>`): (Optional) Base directory for JSON results. If provided, a subdirectory named after `--select-name` (or `--select <PATTERN>`) will be created within this base directory to store the results. If this option is not specified, no benchmark results will be saved.

**Example:**
```bash
# Benchmark functions in 'my-app-stack' containing "api" in their name/ARN,
# saving results under '/tmp/bench_results/api-group/1024mb/...'
startled stack my-app-stack \
    --select "api" \
    --select-name "api-group" \
    --memory 1024 \
    --concurrent 10 \
    --rounds 50 \
    --output-dir /tmp/bench_results

# Benchmark functions matching a regex, using the --select pattern for directory naming
startled stack my-data-processing-stack \
    --select "processor" \
    --select-regex ".*ProcessorFunction$" \
    --memory 512 \
    --output-dir /data/benchmarks
```

#### 3. `report`

Generates HTML reports from previously collected JSON benchmark results.

**Syntax:**
`startled report --input-dir <PATH> --output-dir <PATH> [OPTIONS]`

**Key Options:**
-   `--input-dir <PATH>` (`-d <PATH>`): (Required) Directory containing the JSON benchmark result files. `startled` expects a structure like `<input_dir>/{group_name}/{subgroup_name}/*.json` (e.g., `/tmp/startled_results/my-app/prod/1024mb/*.json`).
-   `--output-dir <PATH>` (`-o <PATH>`): (Required) Directory where the HTML report files will be generated. An `index.html` file and associated assets will be created in this directory.
-   `--screenshot <THEME>`: (Optional) Generates PNG screenshots of the charts. `<THEME>` can be `Light` or `Dark`. This requires the `screenshots` compile-time feature and a properly configured headless Chrome environment.
-   `--readme <MARKDOWN_FILE>`: (Optional) Specifies a markdown file whose content will be rendered as HTML and included on the landing page of the report. This allows for adding custom documentation, explanations, or findings to the benchmark report.
-   `--template-dir <PATH>`: (Optional) Specifies a custom directory containing templates for report generation. This allows for complete customization of the report appearance and behavior. The directory should contain HTML templates (`index.html`, `chart.html`, `_sidebar.html`), CSS (`css/style.css`), and a single JavaScript file (`js/lib.js`) that handles all chart rendering functionality.
-   `--base-url <URL_PATH>`: (Optional) Specifies a base URL path for all generated links in the report. This is useful when hosting the report in a subdirectory of a website (e.g., `--base-url "/reports/"` for a site hosted at `http://example.com/reports/`). When specified, all internal links will be prefixed with this path, ensuring proper navigation even when the report is not hosted at the root of a domain.

**Example:**
```bash
startled report \
    --input-dir /tmp/startled_results/my-application-services \
    --output-dir /var/www/benchmarks/my-application-services \
    --screenshot Dark \
    --readme benchmark-notes.md \
    --template-dir /path/to/custom-templates \
    --base-url "/benchmarks/my-application-services"
```
The main HTML report will be accessible at `/var/www/benchmarks/my-application-services/index.html` and can be hosted at `http://example.com/benchmarks/my-application-services/`.

## How It Works

`startled` follows a structured process for benchmarking and data collection.

### Benchmarking Process Stages

1.  **Configuration Adjustment (Optional)**: If `--memory` or `--env` options are provided, `startled` first retrieves the target Lambda function's existing configuration. It then applies the specified temporary changes, saving the original configuration for later restoration.
2.  **Cold Start Invocations**: The CLI initiates a series of concurrent invocations (matching the `--concurrent` value). These initial invocations are considered cold starts.
3.  **Warm Start Invocations**: Following the cold starts, `startled` executes `--rounds` number of warm start batches. Each batch comprises `--concurrent` invocations to the (now likely initialized) Lambda execution environments.
4.  **Configuration Restoration**: Upon completion of all invocations, or if the process is interrupted, `startled` attempts to restore the Lambda function to its original memory and environment variable settings.

### Metric Collection Details

`startled` gathers metrics from both server-side and client-side perspectives:

-   **Server-Side Metrics**:
    -   These are obtained by requesting the last 4KB of execution logs from AWS Lambda (`LogType::Tail`).
    -   `startled` parses the `platform.report` lines within these logs to extract key metrics:
        -   `initDurationMs`: Initialization time for the function environment (relevant for cold starts).
        -   `durationMs`: Execution time of the function handler.
        -   `billedDurationMs`: The duration for billing purposes.
        -   `memorySizeMB`: The configured memory for the function.
        -   `maxMemoryUsedMB`: The maximum memory utilized during the invocation.
        -   `extensionOverhead`: Derived from spans named `extensionOverhead` within the `spans` array of the `platform.report`. This metric quantifies the performance impact of Lambda Extensions.
        -   `totalColdStartDuration`: Calculated as the sum of `initDurationMs` and `durationMs`.

-   **Client-Side Metrics**:
    -   These metrics represent the total invocation time as observed by the client.
    -   **Direct Measurement**: If no proxy function is specified, the CLI measures the wall-clock time for the AWS SDK's `invoke` API call to complete. This measurement includes network latency between the CLI execution environment and the Lambda API endpoint.
    -   **Proxied Measurement**:
        -   When a `--proxy <PROXY_FUNCTION>` is specified, `startled` invokes this designated proxy Lambda function.
        -   The payload sent to the proxy includes the `target` function's ARN/name and the `payload` intended for the target.
        -   The proxy function is responsible for invoking the `target` function, measuring the duration of that internal invocation, and returning this duration (as `invocation_time_ms`) to `startled`. This approach yields a client-side duration measurement from within the AWS network, minimizing the impact of external network conditions. (See "Proxy Function Contract").

-   **Trace Context Propagation**:
    -   To facilitate end-to-end distributed tracing, `startled` automatically injects standard trace context headers (`traceparent`, `tracestate` for W3C/OpenTelemetry, and `X-Amzn-Trace-Id` for AWS X-Ray) into the JSON payload sent to the Lambda function (or its proxy). These headers are added under a `headers` key within the payload.

### Report Generation Process

1.  **Data Aggregation**: The `report` command reads all `.json` result files from the specified `--input-dir`. It expects a hierarchical directory structure (e.g., `group_name/subgroup_name/*.json`) to organize the reports effectively.
2.  **Statistical Analysis**: For each relevant metric (such as init duration, server duration, client duration, extension overhead, and memory usage), `startled` calculates:
    -   Average (Mean)
    -   Median (P50)
    -   95th Percentile (P95)
    -   99th Percentile (P99)
3.  **Markdown Rendering**: If a markdown file is provided via the `--readme` option, the file is parsed and rendered as HTML to be included on the landing page of the report. This allows for adding custom documentation, explanations of the benchmark setup, or summary of findings.
4.  **Template Loading**: 
    -   By default, `startled` uses embedded HTML templates, CSS, and JavaScript files for report generation.
    -   If a custom template directory is provided via the `--template-dir` option, the CLI loads templates from this directory instead, allowing for complete customization of the report appearance and behavior.
    -   Required files in the custom template directory include HTML templates (`index.html`, `chart.html`, `_sidebar.html`), CSS (`css/style.css`), and a single JavaScript file (`js/lib.js`) that handles both UI functionality and chart generation.
5.  **HTML and Chart Generation**:
    -   Utilizes the Tera templating engine for generating HTML pages.
    -   Embeds interactive charts created with Apache ECharts for data visualization.
    -   Produces a variety of charts, including:
        -   Bar charts comparing AVG/P50/P95/P99 statistics for cold start metrics (init duration, server duration, total cold start duration, and extension overhead).
        -   Bar charts for warm start metrics (server duration, client duration, and extension overhead).
        -   Bar charts for memory usage.
        -   Scatter plots illustrating client duration for each warm invocation over time, useful for identifying trends and outliers.
    -   Generates an `index.html` file as a central navigation point for the report, with a sidebar for navigating between different charts and configurations.
6.  **SEO-Friendly URL Structure**:
    -   The report uses a clean URL structure with directories instead of file extensions for better SEO and readability.
    -   Chart URLs follow the format: `/group_name/subgroup_name/chart-type/` with an index.html inside each directory.
    -   The kebab-case naming convention is used for chart directories (e.g., `cold-start-init/` instead of `cold_start_init.html`).
    -   This structure works well with most web servers and makes the reports more search engine friendly.

### Output File Structure

-   **JSON Results**: Individual benchmark results are stored in a structured path if an output directory is specified.
    -   For `function` command: If `--output-dir` is specified, results are saved to `your_output_dir/{memory_setting}/{function_name}.json` (or `your_output_dir/default/{function_name}.json` if memory is not set). If `--output-dir` is omitted, no results are saved.
    -   For `stack` command: If `--output-dir` is specified, results are saved to `your_output_dir/{select_name_or_pattern}/{memory_setting}/{function_name}.json` (or `your_output_dir/{select_name_or_pattern}/default/{function_name}.json` if memory is not set). If `--output-dir` is omitted, no results are saved.
        -   Example with `--output-dir` and `--select-name`: `/tmp/results/api-tests/512mb/my-api-function.json`
        -   If `--output-dir` is not used with the `stack` command, no `user-services/256mb/user-auth-function.json` would be created.
-   **HTML Reports**: The `report` command generates a structured set of HTML files within its specified `--output-dir`. The input directory for the report command should point to the level containing the `{select_name_or_pattern}` or `{memory_setting}` (for function command) directories.
    -   Example: `/srv/benchmarks/run1/index.html`, with sub-pages such as `/srv/benchmarks/run1/api-tests/512mb/cold_start_init.html`.
    -   Associated CSS and JavaScript files are also copied to this directory.

## The `benchmark/testbed/` Environment

This repository includes a `benchmark/testbed/` directory, which provides a pre-configured environment for use with `startled`.

This testbed contains:
-   Lambda functions for various runtimes (Rust, Node.js, Python).
-   Implementations with different OpenTelemetry configurations (standard Otel, ADOT, Rotel, AWS CloudWatch Application Signals) and baseline `stdout` versions.
-   An AWS SAM template (`template.yaml`) for deploying all test functions and necessary supporting resources, including a mock OTLP receiver and the proxy function.
-   A `Makefile` designed to orchestrate benchmark execution across different runtimes and memory configurations using the `startled` CLI.

Consult `benchmark/testbed/README.md` for comprehensive instructions on deploying and utilizing this testbed.

## Proxy Function Contract

For more accurate client-side duration measurements that minimize the influence of network latency from the CLI host, `startled` supports the use of a proxy Lambda function.

**Mechanism:**
1.  A designated proxy Lambda function is deployed in the same AWS region as the target functions.
2.  When `startled` is run with the `--proxy <PROXY_FUNCTION_NAME>` option, it invokes this proxy function.
3.  The payload sent by `startled` to the proxy function follows this structure:
    ```json
    {
        "target": "arn:aws:lambda:region:account-id:function:your-target-function-arn",
        "payload": { // Original payload intended for the target function
            "your_data_key": "your_data_value",
            // Tracing headers are automatically injected by startled
            "headers": {
                "traceparent": "...",
                "tracestate": "..."
            }
        }
    }
    ```
4.  The proxy function must be implemented to:
    a.  Receive and parse this payload.
    b.  Extract the `target` function ARN/name and its `payload`.
    c.  Record a timestamp before invoking the target (start_time).
    d.  Invoke the `target` function with its designated `payload`.
    e.  Record a timestamp after the target invocation completes (end_time).
    f.  Calculate the duration: `invocation_time_ms = (end_time - start_time)` in milliseconds.
    g.  Return a JSON response to `startled` in the following format:
        ```json
        {
            "invocation_time_ms": 123.45, // The measured duration of the target's invocation
            "response": { // The complete, unaltered response from the target function
                "statusCode": 200,
                "body": "Response from target function."
            }
        }
        ```

The `benchmark/testbed/` includes a Rust-based proxy function (`ProxyFunction` in its `template.yaml`) that adheres to this contract and can serve as a reference.

**Rationale for using a proxy:**
Executing the timing logic within a proxy Lambda located in the same AWS network as the target function provides a more representative measurement of invocation latency, as opposed to measurements taken from a local machine which would include variable internet latency to AWS API endpoints.

## Development and Code Structure

For those interested in the internals of `startled` or contributing:

-   **Main Entry Point**: `benchmark/src/main.rs` (handles command-line argument parsing using `clap`).
-   **Core Benchmarking Logic**: `benchmark/src/benchmark.rs`.
-   **AWS Lambda Interactions**: `benchmark/src/lambda.rs` (function invocation, configuration management, log parsing).
-   **Report Generation**: `benchmark/src/report.rs` (HTML templating, chart creation).
-   **Statistical Calculations**: `benchmark/src/stats.rs`.
-   **Data Structures**: `benchmark/src/types.rs` (defines metrics, configurations, report structures).
-   **HTML Templates & Assets**: `benchmark/src/templates/` (Tera templates, CSS, JavaScript for ECharts).

Build the project using standard Cargo commands from the `benchmark/` directory:
```bash
cd benchmark/
cargo build
# To run tests:
cargo test
```
