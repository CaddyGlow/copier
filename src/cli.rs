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

    /// Copy arguments (available by default)
    #[command(flatten)]
    pub copy: CopyArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Paste files from a markdown document
    Paste(PasteArgs),

    /// Explicit copy mode (equivalent to default invocation)
    Copy(CopyArgs),
}

#[derive(Args, Debug, Default, Clone)]
pub struct CopyArgs {
    /// Files, directories, or glob patterns to copy
    #[arg(value_name = "PATH", required = false)]
    pub paths: Vec<PathBuf>,

    /// Write copied markdown to a file instead of stdout
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
pub struct PasteArgs {
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
