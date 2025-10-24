use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

use crate::config::{ConflictStrategy, FencePreference, OutputFormat};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to a configuration file (defaults to quickctx.toml if present)
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Increase log verbosity (repeatable)
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Aggregation arguments (available by default)
    #[command(flatten)]
    pub aggregate: AggregateArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Extract files from a markdown document
    Extract(ExtractArgs),

    /// Explicit aggregate mode (equivalent to default invocation)
    Aggregate(AggregateArgs),
}

#[derive(Args, Debug, Default, Clone)]
pub struct AggregateArgs {
    /// Files, directories, or glob patterns to aggregate
    #[arg(value_name = "PATH", required = false)]
    pub paths: Vec<PathBuf>,

    /// Write aggregated markdown to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Output format style
    #[arg(short = 'f', long = "format", value_enum)]
    pub format: Option<OutputFormat>,

    /// Preferred fence style
    #[arg(long = "fence", value_enum)]
    pub fence: Option<FencePreference>,

    /// Do not respect .gitignore entries
    #[arg(long = "no-gitignore", action = ArgAction::SetTrue)]
    pub no_gitignore: bool,

    /// Additional ignore file(s) to apply
    #[arg(long = "ignore-file", value_name = "FILE")]
    pub ignore_file: Vec<PathBuf>,

    /// Exclude glob pattern(s)
    #[arg(long = "exclude", value_name = "GLOB")]
    pub exclude: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub struct ExtractArgs {
    /// Markdown input file (omit to read from stdin)
    #[arg(value_name = "INPUT", required = false)]
    pub input: Option<PathBuf>,

    /// Output directory root (defaults to current directory)
    #[arg(short, long = "output", value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    /// Conflict handling strategy
    #[arg(long = "conflict", value_enum)]
    pub conflict: Option<ConflictStrategy>,
}
