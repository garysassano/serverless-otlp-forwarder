use crate::stats::calculate_stats;
use colored::*;
use comfy_table::{
    presets::*, Attribute, Cell, CellAlignment, ContentArrangement, Table, TableComponent,
};

pub fn print_benchmark_results(function_name: &str, results: &crate::benchmark::BenchmarkResults) {
    const TABLE_WIDTH: usize = 80;
    if !results.cold_starts.is_empty()
        && results
            .cold_starts
            .iter()
            .any(|m| m.init_duration.is_some())
    {
        println!(
            "{}",
            format!(
                "Function: {} | Cold Start Metrics ({} invocations) | Memory Size: {} MB",
                function_name,
                results.cold_starts.len(),
                results.cold_starts[0].memory_size
            )
            .bright_blue()
            .bold()
        );
        println!("{}", "─".repeat(TABLE_WIDTH).dimmed());
        let mut table = Table::new();
        table
            .load_preset(NOTHING)
            .set_style(TableComponent::MiddleHeaderIntersections, '┴')
            .set_style(TableComponent::BottomBorder, '─')
            .set_style(TableComponent::BottomBorderIntersections, '─')
            .set_style(TableComponent::HeaderLines, '─')
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_width(TABLE_WIDTH as u16)
            .set_header(vec![
                Cell::new("Metric").add_attribute(Attribute::Bold),
                Cell::new("Mean").add_attribute(Attribute::Bold),
                Cell::new("P50").add_attribute(Attribute::Bold),
                Cell::new("P95").add_attribute(Attribute::Bold),
                Cell::new("P99").add_attribute(Attribute::Bold),
            ]);
        let init_durations: Vec<f64> = results
            .cold_starts
            .iter()
            .filter_map(|m| m.init_duration)
            .collect();
        if !init_durations.is_empty() {
            let stats = calculate_stats(&init_durations);
            table.add_row(vec![
                Cell::new("Init Duration"),
                Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
            ]);
        }
        let durations: Vec<f64> = results.cold_starts.iter().map(|m| m.duration).collect();
        let stats = calculate_stats(&durations);
        table.add_row(vec![
            Cell::new("Server Duration"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let extension_overheads: Vec<f64> = results
            .cold_starts
            .iter()
            .map(|m| m.extension_overhead)
            .collect();
        let stats = calculate_stats(&extension_overheads);
        table.add_row(vec![
            Cell::new("Extension Overhead"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let total_cold_start_durations: Vec<f64> = results
            .cold_starts
            .iter()
            .filter_map(|m| m.total_cold_start_duration)
            .collect();
        if !total_cold_start_durations.is_empty() {
            let stats = calculate_stats(&total_cold_start_durations);
            table.add_row(vec![
                Cell::new("Total Cold Start Duration"),
                Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
                Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
            ]);
        }
        let billed_durations: Vec<f64> = results
            .cold_starts
            .iter()
            .map(|m| m.billed_duration as f64)
            .collect();
        let stats = calculate_stats(&billed_durations);
        table.add_row(vec![
            Cell::new("Billed Duration"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let memory_used: Vec<f64> = results
            .cold_starts
            .iter()
            .map(|m| m.max_memory_used as f64)
            .collect();
        let stats = calculate_stats(&memory_used);
        table.add_row(vec![
            Cell::new("Memory Used"),
            Cell::new(format!("{:.2} MB", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        println!("{}\n", table);
    }

    if !results.warm_starts.is_empty() {
        println!(
            "{}",
            format!(
                "Function: {} | Warm Start Metrics ({} invocations) | Memory Size: {} MB",
                function_name,
                results.warm_starts.len(),
                results.warm_starts[0].memory_size
            )
            .bright_yellow()
            .bold()
        );
        println!("{}", "─".repeat(TABLE_WIDTH).dimmed());
        let mut table = Table::new();
        table
            .load_preset(NOTHING)
            .set_style(TableComponent::MiddleHeaderIntersections, '┴')
            .set_style(TableComponent::BottomBorder, '─')
            .set_style(TableComponent::BottomBorderIntersections, '─')
            .set_style(TableComponent::HeaderLines, '─')
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_width(TABLE_WIDTH as u16)
            .set_header(vec![
                Cell::new("Metric").add_attribute(Attribute::Bold),
                Cell::new("Mean").add_attribute(Attribute::Bold),
                Cell::new("P50").add_attribute(Attribute::Bold),
                Cell::new("P95").add_attribute(Attribute::Bold),
                Cell::new("P99").add_attribute(Attribute::Bold),
            ]);
        let durations: Vec<f64> = results.warm_starts.iter().map(|m| m.duration).collect();
        let stats = calculate_stats(&durations);
        table.add_row(vec![
            Cell::new("Server Duration"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let extension_overheads: Vec<f64> = results
            .warm_starts
            .iter()
            .map(|m| m.extension_overhead)
            .collect();
        let stats = calculate_stats(&extension_overheads);
        table.add_row(vec![
            Cell::new("Extension Overhead"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let billed_durations: Vec<f64> = results
            .warm_starts
            .iter()
            .map(|m| m.billed_duration as f64)
            .collect();
        let stats = calculate_stats(&billed_durations);
        table.add_row(vec![
            Cell::new("Billed Duration"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        let memory_used: Vec<f64> = results
            .warm_starts
            .iter()
            .map(|m| m.max_memory_used as f64)
            .collect();
        let stats = calculate_stats(&memory_used);
        table.add_row(vec![
            Cell::new("Memory Used"),
            Cell::new(format!("{:.2} MB", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} MB", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        println!("{}\n", table);
    }

    if !results.client_measurements.is_empty() {
        println!(
            "{}",
            format!(
                "Function: {} | Client Metrics ({} invocations) | Memory Size: {} MB",
                function_name,
                results.client_measurements.len(),
                results.client_measurements[0].memory_size
            )
            .bright_cyan()
            .bold()
        );
        println!("{}", "─".repeat(TABLE_WIDTH).dimmed());
        let mut table = Table::new();
        table
            .load_preset(NOTHING)
            .set_style(TableComponent::MiddleHeaderIntersections, '┴')
            .set_style(TableComponent::BottomBorder, '─')
            .set_style(TableComponent::BottomBorderIntersections, '─')
            .set_style(TableComponent::HeaderLines, '─')
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_width(TABLE_WIDTH as u16)
            .set_header(vec![
                Cell::new("Metric").add_attribute(Attribute::Bold),
                Cell::new("Mean").add_attribute(Attribute::Bold),
                Cell::new("P50").add_attribute(Attribute::Bold),
                Cell::new("P95").add_attribute(Attribute::Bold),
                Cell::new("P99").add_attribute(Attribute::Bold),
            ]);
        let client_durations: Vec<f64> = results
            .client_measurements
            .iter()
            .map(|m| m.client_duration)
            .collect();
        let stats = calculate_stats(&client_durations);
        table.add_row(vec![
            Cell::new("Client Duration"),
            Cell::new(format!("{:.2} ms", stats.mean)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p50)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p95)).set_alignment(CellAlignment::Right),
            Cell::new(format!("{:.2} ms", stats.p99)).set_alignment(CellAlignment::Right),
        ]);
        println!("{}\n", table);
    }
}
