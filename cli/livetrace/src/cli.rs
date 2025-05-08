use crate::console_display::Theme;
use clap::{builder::TypedValueParser, error::ErrorKind, ArgGroup, Parser, ValueEnum};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

/// Defines coloring strategies for the console output
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
pub enum ColoringMode {
    /// Color by service name (default)
    #[default]
    Service,
    /// Color by span ID
    Span,
}


/// livetrace: Tail CloudWatch Logs for OTLP/stdout traces and forward them.
#[derive(Parser, Debug, Clone)] // Added Clone
#[command(author = "Dev7A", version, about, long_about = None)]
#[clap(group( // Add group to make poll/timeout mutually exclusive
    ArgGroup::new("mode")
        .required(false) // One or neither can be specified
        .args(["poll_interval", "session_timeout"]),
))]
pub struct CliArgs {
    /// Log group name pattern(s) for discovery (case-sensitive substring search). Can be specified multiple times.
    #[arg(short = 'g', long = "log-group-pattern", num_args(1..))]
    pub log_group_pattern: Option<Vec<String>>,

    /// CloudFormation stack name for log group discovery.
    #[arg(short = 's', long = "stack-name")]
    pub stack_name: Option<String>,

    /// The OTLP HTTP endpoint URL to send traces to (e.g., http://localhost:4318/v1/traces).
    #[arg(short = 'e', long)]
    pub otlp_endpoint: Option<String>,

    /// Add custom HTTP headers to the outgoing OTLP request (e.g., "Authorization=Bearer token"). Can be specified multiple times.
    #[arg(short = 'H', long = "otlp-header")]
    pub otlp_headers: Vec<String>,

    /// AWS Region to use. Defaults to environment/profile configuration.
    #[arg(short = 'r', long = "aws-region")]
    pub aws_region: Option<String>,

    /// AWS Profile to use. Defaults to environment/profile configuration.
    #[arg(short = 'p', long = "aws-profile")]
    pub aws_profile: Option<String>,

    /// Increase logging verbosity (-v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Only forward telemetry, do not display it in the console.
    #[arg(long)]
    pub forward_only: bool,

    /// Comma-separated list of glob patterns for attribute filtering (e.g., "http.*,db.*,aws.lambda.*").
    /// Applies to both span attributes and event attributes.
    #[arg(long = "attrs")]
    pub attrs: Option<String>,

    /// Optional polling interval in seconds. If set, uses FilterLogEvents API instead of StartLiveTail.
    #[arg(long, group = "mode")] // Add to group
    pub poll_interval: Option<u64>,

    /// Session duration in minutes after which livetrace will automatically exit (LiveTail mode only).
    #[arg(long, default_value_t = 30, group = "mode")] // Re-add, add to group
    pub session_timeout: u64,

    /// Event attribute name to use for determining event severity level.
    #[arg(long, default_value = "event.severity")]
    pub event_severity_attribute: String,

    /// Load configuration from a specific profile in .livetrace.toml.
    #[arg(long)]
    pub config_profile: Option<String>,

    /// Save the current effective command-line arguments to the specified profile in .livetrace.toml.
    #[arg(long, value_name = "PROFILE_NAME")]
    pub save_profile: Option<String>,

    /// Color theme for console output.
    ///
    /// Available themes:
    ///  * default - OpenTelemetry-inspired blue-purple palette
    ///  * tableau - Tableau 12 color palette with distinct hues
    ///  * colorbrewer - ColorBrewer Set3 palette (pastel colors)
    ///  * material - Material Design palette with bright, modern colors
    ///  * solarized - Solarized color scheme with muted tones
    ///  * monochrome - Grayscale palette for minimal distraction
    #[arg(
        long,
        default_value = "default",
        value_parser = ThemeValueParser,
        value_name = "THEME",
        help_heading = "Display Options",
    )]
    pub theme: String,

    /// List available color themes and exit.
    #[arg(
        long = "list-themes",
        help_heading = "Display Options",
        conflicts_with = "theme"
    )]
    pub list_themes: bool,

    /// Color output by service name or span ID
    #[arg(
        long = "color-by",
        value_enum,
        default_value_t = ColoringMode::Service,
        help_heading = "Display Options",
    )]
    pub color_by: ColoringMode,

    /// Only display events, hiding span start information in the timeline log view.
    #[arg(long, help_heading = "Display Options")]
    pub events_only: bool,

    /// Maximum time in seconds to wait for spans belonging to a trace before displaying/forwarding it.
    #[arg(long, default_value_t = 5, help_heading = "Processing Options")]
    pub trace_timeout: u64,
}

// Create a custom value parser for themes
#[derive(Clone)]
struct ThemeValueParser;

impl TypedValueParser for ThemeValueParser {
    type Value = String;

    /// This method is called by Clap when validating the theme argument.
    /// It receives the command, argument definition, and user-provided value.
    /// We only use the value parameter to validate the theme name.
    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        // Array of valid themes for display
        let valid_themes = [
            "default - OpenTelemetry-inspired blue-purple palette",
            "tableau - Tableau 12 color palette with distinct hues",
            "colorbrewer - ColorBrewer Set3 palette (pastel colors)",
            "material - Material Design palette with bright, modern colors",
            "solarized - Solarized color scheme with muted tones",
            "monochrome - Grayscale palette for minimal distraction",
        ];

        // Convert OsStr to a regular string for validation
        let theme_str = value.to_string_lossy().to_string();

        // Handle empty theme value (when user just types --theme without a value)
        if theme_str.is_empty() {
            // Show the themes and exit
            print_available_themes();
            // This line should never be reached due to exit()
            return Ok("default".to_string());
        }

        // Check if the theme is valid
        if !Theme::is_valid_theme(&theme_str) {
            // Create a helpful error message with valid themes
            let themes_list = valid_themes.join("\n  * ");

            let err = format!(
                "Invalid theme '{}'. Available themes:\n  * {}",
                theme_str, themes_list
            );

            return Err(clap::Error::raw(ErrorKind::InvalidValue, err));
        }

        // Valid theme, return it
        Ok(theme_str)
    }
}

// Helper function to print available themes
fn print_available_themes() {
    println!("\nAvailable themes:");
    println!("  * default - OpenTelemetry-inspired blue-purple palette");
    println!("  * tableau - Tableau 12 color palette with distinct hues");
    println!("  * colorbrewer - ColorBrewer Set3 palette (pastel colors)");
    println!("  * material - Material Design palette with bright, modern colors");
    println!("  * solarized - Solarized color scheme with muted tones");
    println!("  * monochrome - Grayscale palette for minimal distraction");
    println!("\nUsage: livetrace --theme <THEME>");
    std::process::exit(0);
}

/// Parses attribute glob patterns from a string pattern.
/// This is a more general function that doesn't depend on CliArgs or EffectiveConfig directly.
pub fn parse_attr_globs(patterns_opt: &Option<String>) -> Option<GlobSet> {
    match patterns_opt.as_deref() {
        Some(patterns_str) if !patterns_str.is_empty() => {
            let mut builder = GlobSetBuilder::new();
            for pattern in patterns_str.split(',') {
                let trimmed_pattern = pattern.trim();
                if !trimmed_pattern.is_empty() {
                    match Glob::new(trimmed_pattern) {
                        Ok(glob) => {
                            builder.add(glob);
                        }
                        Err(e) => {
                            tracing::warn!(pattern = trimmed_pattern, error = %e, "Invalid glob pattern for attribute filtering, skipping.");
                        }
                    }
                }
            }
            match builder.build() {
                Ok(glob_set) => Some(glob_set),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to build glob set for attributes");
                    None // Treat as no filter if build fails
                }
            }
        }
        _ => None, // No patterns provided
    }
}
