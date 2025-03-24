# Cost Projections for Telemetry Approaches

This script compares the cost between different approaches for handling telemetry data:
1. CloudWatch Logs vs OTel Collector
2. Kinesis Data Streams vs OTel Collector

It generates tables and heatmaps showing cost comparisons across different payload sizes
and execution overhead factors.

## Requirements

This script requires Python 3.13+ and the following dependencies:
- matplotlib
- numpy
- pandas
- rich
- seaborn
- click

The dependencies are automatically handled by `uv`.

## Usage

To run the script and generate visualizations:
```
uv run cost-projections.py --dir png
```

To output in markdown format:
```
uv run cost-projections.py --dir png --markdown > cost.md
```

## Output

The script produces:
- Interactive tables showing cost comparisons
- Heatmap visualizations saved to the specified directory
- Example calculations for different payload sizes and overhead factors

## Options

- `--dir`: Directory where output images will be saved (required)
- `--cw-cost`: CloudWatch Logs ingestion cost per GB (default: 0.50)
- `--lambda-cost`: Lambda cost per GB-second (default: 0.0000166667)
- `--kinesis-cost`: Kinesis Data Streams ingestion cost per GB (default: 0.08)
- `--lambda-memory`: Lambda memory in GB (default: 1.0)
- `--base-exec-time`: Base execution time in ms (default: 100)
- `--markdown`: Output in markdown format instead of rich text 