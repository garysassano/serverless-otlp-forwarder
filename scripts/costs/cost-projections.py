#!/usr/bin/env python3
# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "matplotlib",
#     "numpy",
#     "pandas",
#     "rich",
#     "seaborn",
#     "click",
# ]
# ///
"""
Cost Projections: Comparing Different Telemetry Approaches

This script compares the cost between different approaches for handling telemetry data:
1. CloudWatch Logs vs OTel Collector
2. Kinesis Data Streams vs OTel Collector

It generates tables and heatmaps showing cost comparisons across different payload sizes
and execution overhead factors.
"""

import os
import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
import seaborn as sns
import click
from rich.console import Console
from rich.markdown import Markdown
from rich.table import Table
from rich.panel import Panel
from rich import box

# AWS Pricing constants (in USD)
# https://aws.amazon.com/cloudwatch/pricing/
# https://aws.amazon.com/lambda/pricing/
# https://aws.amazon.com/kinesis/data-streams/pricing/
CW_LOGS_INGESTION_COST_PER_GB = 0.50  # $0.50 per GB
LAMBDA_COST_PER_GB_SECOND = 0.0000166667  # $0.0000166667 per GB-second
KINESIS_INGESTION_COST_PER_GB = 0.08  # $0.08 per GB for on-demand Kinesis

# Assume average Lambda memory size (in GB)
LAMBDA_MEMORY_GB = 1.0

# Base execution time for both approaches (in ms)
BASE_EXECUTION_MS = 100

# Payload sizes in KB
PAYLOAD_SIZES_KB = [1, 4, 8, 16]

# OTel Collector overhead factors (execution time multipliers)
# e.g., 2x means OTel collector takes twice as long as base execution
OTEL_OVERHEAD_FACTORS = [1, 1.5, 2, 3, 4, 5, 6, 7, 8, 9, 10]

def calculate_cost_ratio_cloudwatch_vs_otel(payload_sizes, overhead_factors):
    """
    Calculate cost ratio between CloudWatch Logs and OTel Collector approaches.
    
    Returns:
        - Matrix of cost ratios (CW / OTel) for different payload sizes and overhead factors
    """
    # Initialize result matrix (rows = payload sizes, columns = overhead factors)
    cost_ratios = np.zeros((len(payload_sizes), len(overhead_factors)))
    
    for i, payload_kb in enumerate(payload_sizes):
        # Convert KB to GB for calculations
        payload_gb = payload_kb / 1024 / 1024
        
        # CloudWatch approach cost:
        # 1. Lambda execution at base duration
        # 2. CloudWatch Logs ingestion cost
        cw_exec_time_sec = BASE_EXECUTION_MS / 1000
        cw_lambda_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * cw_exec_time_sec
        cw_ingestion_cost = CW_LOGS_INGESTION_COST_PER_GB * payload_gb
        cw_total_cost = cw_lambda_cost + cw_ingestion_cost
        
        for j, factor in enumerate(overhead_factors):
            # OTel Collector approach cost:
            # Only Lambda execution with overhead, no ingestion cost
            otel_exec_time_sec = (BASE_EXECUTION_MS * factor) / 1000
            otel_lambda_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * otel_exec_time_sec
            
            # Calculate cost ratio: CloudWatch / OTel
            # >1 means CloudWatch is more expensive
            # <1 means OTel is more expensive
            cost_ratios[i, j] = cw_total_cost / otel_lambda_cost
    
    return cost_ratios

def calculate_cost_ratio_kinesis_vs_otel(payload_sizes, overhead_factors):
    """
    Calculate cost ratio between Kinesis Data Streams and OTel Collector approaches.
    
    Parameters:
        - payload_sizes: List of payload sizes in KB
        - overhead_factors: List of OTel execution overhead factors
    
    Returns:
        - Matrix of cost ratios (Kinesis / OTel) for different payload sizes and overhead factors
    """
    # Initialize result matrix (rows = payload sizes, columns = overhead factors)
    cost_ratios = np.zeros((len(payload_sizes), len(overhead_factors)))
    
    for i, payload_kb in enumerate(payload_sizes):
        # Convert KB to GB for calculations
        payload_gb = payload_kb / 1024 / 1024
        
        # Kinesis approach cost:
        # 1. Lambda execution at base duration
        # 2. Kinesis ingestion cost
        # Note: We do not include Kinesis stream cost since at scale it becomes negligible
        kinesis_exec_time_sec = BASE_EXECUTION_MS / 1000
        kinesis_lambda_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * kinesis_exec_time_sec
        kinesis_ingestion_cost = KINESIS_INGESTION_COST_PER_GB * payload_gb
        kinesis_total_cost = kinesis_lambda_cost + kinesis_ingestion_cost
        
        for j, factor in enumerate(overhead_factors):
            # OTel Collector approach cost:
            # Only Lambda execution with overhead, no ingestion cost
            otel_exec_time_sec = (BASE_EXECUTION_MS * factor) / 1000
            otel_lambda_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * otel_exec_time_sec
            
            # Calculate cost ratio: Kinesis / OTel
            # >1 means Kinesis is more expensive
            # <1 means OTel is more expensive
            cost_ratios[i, j] = kinesis_total_cost / otel_lambda_cost
    
    return cost_ratios

def format_ratio(ratio):
    """Format cost ratio for display."""
    if ratio < 1:
        return f"{ratio:.2f}x (CW cheaper)"
    else:
        return f"{ratio:.2f}x (OTel Collector cheaper)"

def create_heatmap(cost_ratios, payload_sizes, overhead_factors, title, output_dir, filename, first_approach="CW", second_approach="OTel Collector", additional_note=None):
    """Create a heatmap visualization of the cost ratio comparison."""
    plt.figure(figsize=(12, 8))
    
    # Prepare the data for visualization
    # Convert ratios to logarithmic scale centered at 1 for better visualization
    # negative values mean first approach is cheaper, positive mean second approach is cheaper
    log_ratios = np.log2(cost_ratios)
    
    # Create labels for the heatmap
    index = [f"{size}KB" for size in payload_sizes]
    columns = [f"{factor}x" for factor in overhead_factors]
    
    # Create a DataFrame for the heatmap
    df = pd.DataFrame(log_ratios, index=index, columns=columns)
    
    # Create a diverging colormap - blue for first_approach cheaper, red for second_approach cheaper
    cmap = sns.diverging_palette(220, 10, as_cmap=True)  # Blue to red
    
    # Create the heatmap with increased figure margins
    plt.subplots_adjust(bottom=0.2)  # More space at the bottom
    
    # Simply convert the raw ratios to percentages (ratio * 100)
    percentages = cost_ratios * 100
    
    # Format the annotations to show just the percentage
    annot = np.vectorize(lambda x: f"{x:.0f}%")(percentages)
    
    # Find the min and max log ratios to set appropriate vmin and vmax values
    # while still keeping 0 (which is log2(1) = log2(100%)) as the center
    max_log_ratio = np.max(np.abs(log_ratios))
    
    # Ensure max_log_ratio is at least 1 (which corresponds to 200% or 50%)
    max_log_ratio = max(max_log_ratio, 1.5)
    
    # Adjust scale based on the approach being compared
    if first_approach == "Kinesis":
        # For Kinesis, use a tighter scale since values are in a narrower range
        max_log_ratio = min(max_log_ratio, 2.0)  # Cap at 2.0 (about 400%)
    else:
        # For CloudWatch, allow a wider scale
        max_log_ratio = min(max_log_ratio, 3.0)  # Cap at 3.0 (about 800%)
    
    # Round to the nearest 0.5
    max_log_ratio = np.ceil(max_log_ratio * 2) / 2
    
    # Create the heatmap with customized colorbar and appropriate vmin/vmax
    ax = sns.heatmap(df, annot=annot, fmt="", cmap=cmap, center=0,
                    vmin=-max_log_ratio, vmax=max_log_ratio,
                    linewidths=.5, cbar_kws={"shrink": .8, "label": ""})
    
    # Customize the colorbar to show percentages instead of log2 values
    colorbar = ax.collections[0].colorbar
    
    # Determine step size based on max_log_ratio
    step = 0.5 if max_log_ratio <= 2 else 1.0
    
    # Create tick positions in log2 space
    log_ticks = np.arange(-max_log_ratio, max_log_ratio + step, step)
    
    # Ensure 0 is included in the ticks (for 100%)
    if 0 not in log_ticks:
        log_ticks = np.sort(np.append(log_ticks, 0))
    
    colorbar.set_ticks(log_ticks)
    
    # Convert log2 values to percentages for the labels
    # 2^x = ratio, so we convert to percentage: ratio * 100
    percentage_labels = [f"{int(round(2**x * 100))}%" for x in log_ticks]
    
    # Replace the middle label (100%) with "Equal cost"
    middle_idx = np.where(np.isclose(log_ticks, 0))[0][0]
    percentage_labels[middle_idx] = "100% (Equal cost)"
    
    colorbar.set_ticklabels(percentage_labels)
    colorbar.ax.set_title("Cost Ratio", fontsize=10, pad=10)
    
    # Instead of text annotations in each cell, add a color legend that matches the heatmap colors
    from matplotlib.patches import Patch
    from matplotlib.colors import Normalize
    
    # Get the actual colors from the colormap for the legend - use the same scale as the heatmap
    norm = Normalize(vmin=-max_log_ratio, vmax=max_log_ratio)
    
    # Use 2/3 of the max_log_ratio for a good representative color
    legend_offset = max_log_ratio * 0.67
    first_color = cmap(norm(-legend_offset))   # A blue color for first_approach cheaper
    second_color = cmap(norm(legend_offset))   # A red color for second_approach cheaper
    
    legend_elements = [
        Patch(facecolor=first_color, edgecolor='black', label=f'{first_approach} cheaper'),
        Patch(facecolor=second_color, edgecolor='black', label=f'{second_approach} cheaper')
    ]
    plt.legend(handles=legend_elements, loc='upper center', bbox_to_anchor=(0.5, -0.12), ncol=2)
    
    plt.title(title, fontsize=16)
    plt.xlabel("OTel Collector Execution Overhead Factor", fontsize=12)
    plt.ylabel("Payload Size", fontsize=12)
    
    # Add additional note if provided with improved formatting
    if additional_note:
        plt.subplots_adjust(bottom=0.25)  # Even more space for note at the bottom
        plt.figtext(0.5, 0.02, additional_note, wrap=False, horizontalalignment='center', fontsize=9)
    
    # plt.tight_layout()  # Commented out to avoid conflicts with manual adjustments
    output_path = os.path.join(output_dir, filename)
    plt.savefig(output_path)
    
    # Use a single print statement
    print(f"\nHeatmap visualization saved as '{output_path}'")
    
    return output_path

def display_comparison_table(cost_ratios, payload_sizes, overhead_factors, first_approach, second_approach="OTel Collector", use_markdown=False, console=None):
    """Display formatted comparison table in either Rich format or Markdown."""
    # A ratio > 1 means second_approach is cheaper, < 1 means first_approach is cheaper
    
    # Create DataFrame for display
    index = [f"{size}KB" for size in payload_sizes]
    columns = [f"{factor}x" for factor in overhead_factors]
    
    # Create a DataFrame with full data for the raw numbers table
    raw_df = pd.DataFrame(cost_ratios, index=index, columns=columns)
    
    if use_markdown:
        # Create Markdown output using triple-quoted string
        markdown = f"""## Raw Cost Ratios ({first_approach} / {second_approach})
* Values > 100%: {first_approach} is more expensive ({second_approach} cheaper)
* Values < 100%: {first_approach} is cheaper ({second_approach} more expensive)

"""
        
        # Build markdown table
        md_table = "| Payload Size |" + "|".join([f" {col} " for col in columns]) + "|\n"
        md_table += "|" + "-|"*(len(columns)+1) + "\n"
        
        for i, row_idx in enumerate(index):
            row_values = [f" {row_idx} "] + [f" {val:.0f}% " for val in raw_df.iloc[i] * 100]
            md_table += "|" + "|".join(row_values) + "|\n"
        
        # Combine markdown and table
        markdown += md_table
        markdown += f"""
## Cost Comparison: {first_approach} vs {second_approach}
* **Rows**: Payload sizes in KB
* **Columns**: OTel Collector execution overhead factor
* **Values**: How many times cheaper one approach is versus the other
"""
        
        print(markdown)
    else:
        # Use Rich for console output
        console.print(f"\n[bold]Raw Cost Ratios ({first_approach} / {second_approach})[/bold]")
        console.print(f"• Values > 100%: {first_approach} is more expensive ({second_approach} cheaper)")
        console.print(f"• Values < 100%: {first_approach} is cheaper ({second_approach} more expensive)")
        
        # Create Rich table with better formatting
        table = Table(
            title=f"Cost Ratio: {first_approach} vs {second_approach}", 
            box=box.SIMPLE_HEAD,
            show_header=True, 
            header_style="bold white on blue",
            min_width=70,
            title_style="bold cyan",
            title_justify="center",
            padding=(0, 1)
        )
        
        # Add columns
        table.add_column("Payload\nSize", style="bold", no_wrap=True)
        for col in columns:
            table.add_column(col, justify="right", no_wrap=True)
        
        # Add rows with color formatting
        for i, row_idx in enumerate(index):
            row_values = []
            for val in raw_df.iloc[i] * 100:
                # Format with color based on value
                if val > 100:
                    # First approach more expensive (second cheaper)
                    color = "green"
                else:
                    # First approach cheaper
                    color = "red"
                row_values.append(f"[{color}]{val:.0f}%[/{color}]")
            table.add_row(row_idx, *row_values)
        
        console.print("\n")
        console.print(table)
        
        # Add explanation
        legend = Table.grid(padding=1)
        legend.add_column(style="bold")
        legend.add_column()
        
        legend.add_row("[bold]Rows:[/bold]", "Payload sizes in KB")
        legend.add_row("[bold]Columns:[/bold]", "OTel Collector execution overhead factor")
        legend.add_row("[bold]Values:[/bold]", "Cost ratio as percentage")
        legend.add_row("[green]Green:[/green]", f"{second_approach} is cheaper")
        legend.add_row("[red]Red:[/red]", f"{first_approach} is cheaper")
        
        console.print(Panel(
            legend,
            title="Legend",
            title_align="left",
            border_style="dim",
            padding=(1, 2)
        ))

def show_example_calculation(payload_kb, factor, first_approach, use_markdown=False, console=None):
    """Show detailed example calculation for specific payload size and overhead factor."""
    payload_gb = payload_kb / 1024 / 1024
    
    # Base Lambda execution time
    base_exec_time_sec = BASE_EXECUTION_MS / 1000
    base_lambda_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * base_exec_time_sec
    
    # OTel approach
    otel_exec_time_sec = (BASE_EXECUTION_MS * factor) / 1000
    otel_total_cost = LAMBDA_COST_PER_GB_SECOND * LAMBDA_MEMORY_GB * otel_exec_time_sec
    
    # Calculate the ratio and determine the result message
    if first_approach == "CloudWatch":
        # CloudWatch approach
        cw_ingestion_cost = CW_LOGS_INGESTION_COST_PER_GB * payload_gb
        first_total_cost = base_lambda_cost + cw_ingestion_cost
        
        if use_markdown:
            approach_details = f"""**CloudWatch Logs approach:**
* Lambda execution: `${base_lambda_cost:.8f}` ({BASE_EXECUTION_MS}ms at {LAMBDA_MEMORY_GB}GB)
* Log ingestion: `${cw_ingestion_cost:.8f}` ({payload_kb}KB at ${CW_LOGS_INGESTION_COST_PER_GB}/GB)
* **Total: `${first_total_cost:.8f}`**"""
        else:
            approach_details = {
                "title": "CloudWatch Logs approach:",
                "items": [
                    f"Lambda execution: ${base_lambda_cost:.8f} ({BASE_EXECUTION_MS}ms at {LAMBDA_MEMORY_GB}GB)",
                    f"Log ingestion: ${cw_ingestion_cost:.8f} ({payload_kb}KB at ${CW_LOGS_INGESTION_COST_PER_GB}/GB)",
                    f"Total: ${first_total_cost:.8f}"
                ]
            }
            
    elif first_approach == "Kinesis":
        # Kinesis approach - without stream cost
        kinesis_ingestion_cost = KINESIS_INGESTION_COST_PER_GB * payload_gb
        first_total_cost = base_lambda_cost + kinesis_ingestion_cost
        
        if use_markdown:
            approach_details = f"""**Kinesis Data Streams approach:**
* Lambda execution: `${base_lambda_cost:.8f}` ({BASE_EXECUTION_MS}ms at {LAMBDA_MEMORY_GB}GB)
* Data ingestion: `${kinesis_ingestion_cost:.8f}` ({payload_kb}KB at ${KINESIS_INGESTION_COST_PER_GB}/GB)
* Stream cost: Not included (negligible at scale)
* **Total: `${first_total_cost:.8f}`**"""
        else:
            approach_details = {
                "title": "Kinesis Data Streams approach:",
                "items": [
                    f"Lambda execution: ${base_lambda_cost:.8f} ({BASE_EXECUTION_MS}ms at {LAMBDA_MEMORY_GB}GB)",
                    f"Data ingestion: ${kinesis_ingestion_cost:.8f} ({payload_kb}KB at ${KINESIS_INGESTION_COST_PER_GB}/GB)",
                    f"Stream cost: Not included (negligible at scale)",
                    f"Total: ${first_total_cost:.8f}"
                ]
            }
    
    # Calculate the ratio and determine the result message
    ratio = first_total_cost / otel_total_cost
    if ratio >= 1:
        result_msg = f"OTel Collector is {ratio:.2f}x cheaper"
        winner = "OTel Collector"
    else:
        result_msg = f"{first_approach} is {1/ratio:.2f}x cheaper"
        winner = first_approach
    
    if use_markdown:
        # Combine all parts into a single markdown string
        markdown = f"""
### Example: {payload_kb}KB payload with {factor}x OTel Collector execution overhead

{approach_details}

**OTel Collector approach:**
* Lambda execution: `${otel_total_cost:.8f}` ({BASE_EXECUTION_MS * factor}ms at {LAMBDA_MEMORY_GB}GB)
* **Total: `${otel_total_cost:.8f}`**

**Result:**
* **{result_msg}**"""
        
        print(markdown)
    else:
        # Rich console output
        console.print(f"\n[bold cyan]Example: {payload_kb}KB payload with {factor}x OTel Collector execution overhead[/bold cyan]")
        
        # Create a grid layout for side-by-side comparison
        grid = Table.grid(padding=0, expand=True)
        grid.add_column(ratio=1)
        grid.add_column(ratio=1)
        
        # First approach panel with improved styling
        first_panel = Panel(
            "\n".join([f"• {item}" for item in approach_details["items"]]),
            title=approach_details["title"],
            border_style="blue",
            title_align="left",
            padding=(1, 2)
        )
        
        # OTel approach panel with improved styling
        otel_panel = Panel(
            f"• Lambda execution: ${otel_total_cost:.8f} ({BASE_EXECUTION_MS * factor}ms at {LAMBDA_MEMORY_GB}GB)\n"
            f"• Total: ${otel_total_cost:.8f}",
            title="OTel Collector approach:",
            border_style="blue",
            title_align="left",
            padding=(1, 2)
        )
        
        # Add both panels side by side
        grid.add_row(first_panel, otel_panel)
        console.print(grid)
        
        # Determine winner and color
        if ratio >= 1:
            winner_color = "green"
            loser_color = "dim"
            winner = "OTel Collector"  # second approach
            loser = first_approach
            factor = ratio
        else:
            winner_color = "green"
            loser_color = "dim"
            winner = first_approach
            loser = "OTel Collector"  # second approach
            factor = 1/ratio
        
        # Result panel with improved styling
        result_panel = Panel(
            f"[{winner_color}]{winner}[/{winner_color}] is [{winner_color}]{factor:.2f}x[/{winner_color}] cheaper than [{loser_color}]{loser}[/{loser_color}]",
            title="[bold]Cost Comparison Result[/bold]",
            border_style="yellow",
            padding=(1, 2),
            title_align="center"
        )
        
        console.print(result_panel)

@click.command()
@click.option(
    "--dir", 
    required=True, 
    type=click.Path(), 
    help="Directory where to save output images"
)
@click.option(
    "--cw-cost", 
    type=float, 
    help=f"CloudWatch Logs ingestion cost per GB (default: {CW_LOGS_INGESTION_COST_PER_GB})"
)
@click.option(
    "--lambda-cost", 
    type=float, 
    help=f"Lambda cost per GB-second (default: {LAMBDA_COST_PER_GB_SECOND})"
)
@click.option(
    "--kinesis-cost", 
    type=float, 
    help=f"Kinesis Data Streams ingestion cost per GB (default: {KINESIS_INGESTION_COST_PER_GB})"
)
@click.option(
    "--lambda-memory", 
    type=float, 
    help=f"Lambda memory in GB (default: {LAMBDA_MEMORY_GB})"
)
@click.option(
    "--base-exec-time", 
    type=int, 
    help=f"Base execution time in ms (default: {BASE_EXECUTION_MS})"
)
@click.option(
    "--markdown", 
    is_flag=True, 
    help="Output in markdown format instead of rich text"
)
def main(dir, cw_cost, lambda_cost, kinesis_cost, lambda_memory, base_exec_time, markdown):
    """Cost Projections for Telemetry Approaches"""
    # Make sure output directory exists
    if not os.path.exists(dir):
        os.makedirs(dir)
    
    # Override constants if provided in arguments
    global CW_LOGS_INGESTION_COST_PER_GB, LAMBDA_COST_PER_GB_SECOND, KINESIS_INGESTION_COST_PER_GB
    global LAMBDA_MEMORY_GB, BASE_EXECUTION_MS
    
    if cw_cost is not None:
        CW_LOGS_INGESTION_COST_PER_GB = cw_cost
    if lambda_cost is not None:
        LAMBDA_COST_PER_GB_SECOND = lambda_cost
    if kinesis_cost is not None:
        KINESIS_INGESTION_COST_PER_GB = kinesis_cost
    if lambda_memory is not None:
        LAMBDA_MEMORY_GB = lambda_memory
    if base_exec_time is not None:
        BASE_EXECUTION_MS = base_exec_time
    
    # Set up the console if using rich output
    console = None if markdown else Console()
    
    # Introduction and assumptions
    assumptions = {
        "Base Lambda execution time": f"{BASE_EXECUTION_MS}ms",
        "Lambda memory": f"{LAMBDA_MEMORY_GB}GB",
        "Lambda cost": f"${LAMBDA_COST_PER_GB_SECOND} per GB-second",
        "CloudWatch Logs ingestion cost": f"${CW_LOGS_INGESTION_COST_PER_GB} per GB",
        "Kinesis Data Streams ingestion cost (on-demand)": f"${KINESIS_INGESTION_COST_PER_GB} per GB",
        "Kinesis Data Streams stream cost": "Not included (negligible at scale)"
    }
    
    if markdown:
        # Markdown output
        md_intro = """## Cost Comparison: Different Telemetry Approaches vs OpenTelemetry Collector

Provide a cost comparison for Cloudwatch ingestion, or the Kinesis experimental extension,
vs the increased billed duration due to execution overhead when using a sidecar collector.

### Assumptions
"""
        for key, value in assumptions.items():
            md_intro += f"* {key}: {value}\n"
        
        print(md_intro)
    else:
        # Rich console output with improved styling
        console.print()
        title = Panel.fit(
            "[bold white]Comparing cost between CloudWatch Logs, Kinesis, and OpenTelemetry Collector approaches[/bold white]",
            title="[bold cyan]Cost Comparison: Telemetry Approaches vs OpenTelemetry[/bold cyan]",
            border_style="cyan",
            padding=(1, 4),
            title_align="center"
        )
        console.print(title)
        
        # Create a table for assumptions with improved styling
        assumptions_table = Table(
            title="Cost Model Assumptions",
            box=box.SIMPLE_HEAD,
            show_header=True,
            header_style="bold white on blue",
            min_width=80,
            title_style="bold",
            title_justify="center",
            padding=(0, 2)
        )
        
        assumptions_table.add_column("Parameter", style="bold")
        assumptions_table.add_column("Value")
        
        for key, value in assumptions.items():
            assumptions_table.add_row(key, value)
        
        console.print(assumptions_table)
    
    # === SCENARIO 1: CloudWatch Logs vs OTel Collector ===
    if markdown:
        scenario1_md = """## SCENARIO 1: CloudWatch Logs vs OTel Collector

### Approaches being compared
1. **CloudWatch Logs approach**: Write directly to CloudWatch Logs
   * Lambda runs at base duration
   * Incurs CloudWatch Logs ingestion costs
2. **OTel Collector approach**: Use OpenTelemetry Collector
   * Lambda runs with execution overhead (columns)
   * No CloudWatch Logs ingestion costs
"""
        print(scenario1_md)
    else:
        console.print()
        scenario_header = Panel(
            "[white]Comparing direct CloudWatch Logs integration with OpenTelemetry Collector approach[/white]",
            title="[bold]SCENARIO 1: CloudWatch Logs vs OTel Collector[/bold]",
            border_style="magenta",
            padding=(1, 2),
            title_align="center"
        )
        console.print(scenario_header)
        
        approaches_table = Table(
            title="Approaches being compared",
            box=box.SIMPLE_HEAD,
            show_header=True,
            header_style="bold white on blue",
            title_style="bold",
            min_width=80,
            padding=(0, 1)
        )
        
        approaches_table.add_column("Approach", style="bold yellow")
        approaches_table.add_column("Description")
        
        approaches_table.add_row(
            "CloudWatch Logs approach", 
            "Write directly to CloudWatch Logs\n• Lambda runs at base duration\n• Incurs CloudWatch Logs ingestion costs"
        )
        approaches_table.add_row(
            "OTel Collector approach", 
            "Use OpenTelemetry Collector\n• Lambda runs with execution overhead\n• No CloudWatch Logs ingestion costs"
        )
        
        console.print(approaches_table)
    
    # Calculate cost ratios for CloudWatch vs OTel
    cw_vs_otel_ratios = calculate_cost_ratio_cloudwatch_vs_otel(PAYLOAD_SIZES_KB, OTEL_OVERHEAD_FACTORS)
    
    # Display comparison table
    display_comparison_table(
        cw_vs_otel_ratios, 
        PAYLOAD_SIZES_KB, 
        OTEL_OVERHEAD_FACTORS, 
        "CloudWatch", 
        "OTel Collector", 
        use_markdown=markdown, 
        console=console
    )
    
    # Create a heatmap visualization
    cloudwatch_note = ("Note: This comparison excludes the costs of running the OpenTelemetry Collector infrastructure, which\n"
                       "would typically be amortized across many Lambda invocations in a production environment.")
    
    create_heatmap(
        cw_vs_otel_ratios, 
        PAYLOAD_SIZES_KB, 
        OTEL_OVERHEAD_FACTORS, 
        "Cost Comparison: CloudWatch Logs vs OTel Collector", 
        dir,
        "cloudwatch_vs_otel_heatmap.png",
        first_approach="CloudWatch",
        second_approach="OTel Collector",
        additional_note=cloudwatch_note
    )
    
    # Example calculations for CloudWatch vs OTel
    if markdown:
        example_header = "\n## Example Calculations: CloudWatch vs OTel Collector"
        print(example_header)
    else:
        console.print("\n[bold]Example Calculations: CloudWatch vs OTel Collector[/bold]", style="cyan")
    
    show_example_calculation(1, 1.5, "CloudWatch", use_markdown=markdown, console=console)  # Small payload, low overhead
    show_example_calculation(16, 8, "CloudWatch", use_markdown=markdown, console=console)  # Large payload, high overhead
    
    # === SCENARIO 2: Kinesis vs OTel Collector ===
    if markdown:
        scenario2_md = """## SCENARIO 2: Kinesis Data Streams vs OTel Collector

### Approaches being compared
1. **Kinesis Data Streams approach**: Send payload to Kinesis
   * Lambda runs at base duration
   * Incurs Kinesis Data Streams ingestion costs
   * Kinesis stream costs not included (negligible at scale)
2. **OTel Collector approach**: Use OpenTelemetry Collector
   * Lambda runs with execution overhead (columns)
   * No Kinesis Data Streams costs
"""
        print(scenario2_md)
    else:
        console.print()
        scenario_header = Panel(
            "[white]Comparing Kinesis Data Streams integration with OpenTelemetry Collector approach[/white]",
            title="[bold]SCENARIO 2: Kinesis Data Streams vs OTel Collector[/bold]",
            border_style="magenta",
            padding=(1, 2),
            title_align="center"
        )
        console.print(scenario_header)
        
        approaches_table = Table(
            title="Approaches being compared",
            box=box.SIMPLE_HEAD,
            show_header=True,
            header_style="bold white on blue",
            title_style="bold",
            min_width=80,
            padding=(0, 1)
        )
        
        approaches_table.add_column("Approach", style="bold yellow")
        approaches_table.add_column("Description")
        
        approaches_table.add_row(
            "Kinesis Data Streams approach", 
            "Send payload to Kinesis\n• Lambda runs at base duration\n• Incurs Kinesis Data Streams ingestion costs\n• Stream costs not included (negligible at scale)"
        )
        approaches_table.add_row(
            "OTel Collector approach", 
            "Use OpenTelemetry Collector\n• Lambda runs with execution overhead\n• No Kinesis Data Streams costs"
        )
        
        console.print(approaches_table)
    
    # Calculate cost ratios for Kinesis vs OTel
    kinesis_vs_otel_ratios = calculate_cost_ratio_kinesis_vs_otel(PAYLOAD_SIZES_KB, OTEL_OVERHEAD_FACTORS)
    
    # Display comparison table
    display_comparison_table(
        kinesis_vs_otel_ratios, 
        PAYLOAD_SIZES_KB, 
        OTEL_OVERHEAD_FACTORS, 
        "Kinesis", 
        "OTel Collector", 
        use_markdown=markdown, 
        console=console
    )
    
    # Create a heatmap visualization with a note about Kinesis shard costs
    kinesis_note = ("Note: Kinesis has a fixed cost of approximately $28/month per shard, with each shard supporting\n"
                    "1MB/s throughput (about 2.5TB/month). At scale with high data volume, this shard cost\n"
                    "becomes negligible on a per-request basis.")
    
    create_heatmap(
        kinesis_vs_otel_ratios, 
        PAYLOAD_SIZES_KB, 
        OTEL_OVERHEAD_FACTORS, 
        "Cost Comparison: Kinesis Data Streams vs OTel Collector", 
        dir,
        "kinesis_vs_otel_heatmap.png",
        first_approach="Kinesis",
        second_approach="OTel Collector",
        additional_note=kinesis_note
    )
    
    # Example calculations for Kinesis vs OTel
    if markdown:
        example_header = "\n## Example Calculations: Kinesis vs OTel Collector"
        print(example_header)
    else:
        console.print("\n[bold]Example Calculations: Kinesis vs OTel Collector[/bold]", style="cyan")
    
    show_example_calculation(1, 1.5, "Kinesis", use_markdown=markdown, console=console)  # Small payload, low overhead
    show_example_calculation(16, 8, "Kinesis", use_markdown=markdown, console=console)  # Large payload, high overhead

if __name__ == "__main__":
    main() 