use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use clap::ValueEnum;
use serde::Deserialize;
use strum::{Display, EnumString};

use crate::cli::{AggregateArgs, Cli, Commands, ExtractArgs};
use crate::error::{CopierError, Result};

#[derive(
    Debug, Clone, Copy, ValueEnum, Deserialize, Display, EnumString, PartialEq, Eq, Default,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum OutputFormat {
    #[default]
    Simple,
    Comment,
    Heading,
}

#[derive(
    Debug, Clone, Copy, ValueEnum, Deserialize, Display, EnumString, PartialEq, Eq, Default,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum FencePreference {
    #[default]
    Auto,
    Backtick,
    Tilde,
}

#[derive(
    Debug, Clone, Copy, ValueEnum, Deserialize, Display, EnumString, PartialEq, Eq, Default,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ConflictStrategy {
    #[default]
    Prompt,
    Skip,
    Overwrite,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub context: AppContext,
    pub mode: ModeConfig,
}

#[derive(Debug, Clone)]
pub struct AppContext {
    pub cwd: Utf8PathBuf,
    pub verbosity: u8,
}

#[derive(Debug, Clone)]
pub enum ModeConfig {
    Aggregate(AggregateConfig),
    Extract(ExtractConfig),
}

#[derive(Debug, Clone)]
pub struct AggregateConfig {
    pub inputs: Vec<String>,
    pub output: Option<Utf8PathBuf>,
    pub format: OutputFormat,
    pub fence: FencePreference,
    pub respect_gitignore: bool,
    pub ignore_files: Vec<Utf8PathBuf>,
    pub excludes: Vec<String>,
}

impl AggregateConfig {
    pub fn require_inputs(&self) -> Result<()> {
        if self.inputs.is_empty() {
            return Err(CopierError::InvalidArgument(
                "no input paths were provided".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum InputSource {
    Stdin,
    File(Utf8PathBuf),
}

#[derive(Debug, Clone)]
pub struct ExtractConfig {
    pub source: InputSource,
    pub output_dir: Utf8PathBuf,
    pub conflict: ConflictStrategy,
}

pub fn load(cli: &Cli) -> Result<RuntimeConfig> {
    let cwd = std::env::current_dir().context("failed to read current directory")?;
    let cwd = to_utf8_path(cwd)?;

    let config_path = resolve_config_path(cli, &cwd);
    let file_config = if let Some(path) = &config_path {
        parse_file_config(path)?
    } else {
        FileConfig::default()
    };

    let verbosity = cli.verbose + file_config.general.verbose.unwrap_or(0);

    let context = AppContext { cwd, verbosity };

    let mode = match &cli.command {
        Some(Commands::Aggregate(args)) => {
            let cfg = build_aggregate_config(Some(args), &cli.aggregate, &file_config)?;
            ModeConfig::Aggregate(cfg)
        }
        Some(Commands::Extract(args)) => {
            let cfg = build_extract_config(args, &file_config, &context)?;
            ModeConfig::Extract(cfg)
        }
        None => {
            let cfg = build_aggregate_config(None, &cli.aggregate, &file_config)?;
            ModeConfig::Aggregate(cfg)
        }
    };

    Ok(RuntimeConfig { context, mode })
}

fn resolve_config_path(cli: &Cli, cwd: &Utf8Path) -> Option<Utf8PathBuf> {
    if let Some(path) = &cli.config {
        return to_utf8_path(path.clone()).ok();
    }

    let default = cwd.join("copier.toml");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn build_aggregate_config(
    override_args: Option<&AggregateArgs>,
    default_args: &AggregateArgs,
    file_config: &FileConfig,
) -> Result<AggregateConfig> {
    let args = override_args.unwrap_or(default_args);
    let mut inputs: Vec<String> = file_config.aggregate.paths.clone();
    inputs.extend(args.paths.iter().map(|p| p.to_string_lossy().to_string()));

    let respect_gitignore = if args.no_gitignore {
        false
    } else {
        file_config.aggregate.respect_gitignore.unwrap_or(true)
    };

    let mut ignore_files = Vec::new();
    ignore_files.extend(file_config.aggregate.ignore_files.iter().cloned());
    for path in &args.ignore_file {
        ignore_files.push(to_utf8_path(path.clone())?);
    }

    let mut excludes = file_config.aggregate.exclude.clone();
    excludes.extend(args.exclude.iter().cloned());

    let output = if let Some(path) = &args.output {
        Some(to_utf8_path(path.clone())?)
    } else {
        file_config.aggregate.output.clone()
    };

    let format = args
        .format
        .or(file_config.aggregate.format)
        .unwrap_or_default();

    let fence = args
        .fence
        .or(file_config.aggregate.fence)
        .unwrap_or_default();

    let config = AggregateConfig {
        inputs,
        output,
        format,
        fence,
        respect_gitignore,
        ignore_files,
        excludes,
    };

    Ok(config)
}

fn build_extract_config(
    args: &ExtractArgs,
    file_config: &FileConfig,
    context: &AppContext,
) -> Result<ExtractConfig> {
    let output_dir = if let Some(dir) = &args.output_dir {
        to_utf8_path(dir.clone())?
    } else if let Some(dir) = &file_config.extractor.output_dir {
        dir.clone()
    } else {
        context.cwd.clone()
    };

    let conflict = args
        .conflict
        .or(file_config.extractor.conflict)
        .unwrap_or_default();

    let source = match &args.input {
        Some(path) => InputSource::File(to_utf8_path(path.clone())?),
        None => InputSource::Stdin,
    };

    Ok(ExtractConfig {
        source,
        output_dir,
        conflict,
    })
}

fn parse_file_config(path: &Utf8Path) -> Result<FileConfig> {
    let raw = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;
    let de = toml::de::Deserializer::parse(&raw)
        .map_err(|err| CopierError::ConfigParse(err.to_string()))?;
    let file_config: FileConfig = serde_path_to_error::deserialize(de)
        .map_err(|err| CopierError::ConfigParse(err.to_string()))?;
    Ok(file_config)
}

fn to_utf8_path(path: PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path)
        .map_err(|p| CopierError::InvalidUtfPath(p.to_string_lossy().into_owned()))
}

/// Load analyze configuration from a config file
pub fn load_analyze_config(config_path: Option<&std::path::Path>) -> Result<AnalyzeSection> {
    if let Some(path) = config_path {
        if path.exists() {
            let file_config = parse_file_config(&Utf8PathBuf::from_path_buf(path.to_path_buf())
                .map_err(|p| CopierError::InvalidUtfPath(p.to_string_lossy().into_owned()))?)?;
            return Ok(file_config.analyze);
        }
    }

    // Try default location
    let default_path = Utf8PathBuf::from("copier.toml");
    if default_path.exists() {
        let file_config = parse_file_config(&default_path)?;
        return Ok(file_config.analyze);
    }

    Ok(AnalyzeSection::default())
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    aggregate: AggregateSection,
    #[serde(default)]
    extractor: ExtractSection,
    #[serde(default)]
    general: GeneralSection,
    #[serde(default)]
    analyze: AnalyzeSection,
}

#[derive(Debug, Default, Deserialize)]
struct AggregateSection {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    output: Option<Utf8PathBuf>,
    #[serde(default)]
    format: Option<OutputFormat>,
    #[serde(default)]
    fence: Option<FencePreference>,
    #[serde(default)]
    respect_gitignore: Option<bool>,
    #[serde(default)]
    ignore_files: Vec<Utf8PathBuf>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExtractSection {
    #[serde(default)]
    output_dir: Option<Utf8PathBuf>,
    #[serde(default)]
    conflict: Option<ConflictStrategy>,
}

#[derive(Debug, Default, Deserialize)]
struct GeneralSection {
    #[serde(default)]
    verbose: Option<u8>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct AnalyzeSection {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub lsp_servers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub bin_paths: Vec<String>,
    #[serde(default)]
    pub lsp_readiness_timeout_secs: Option<u64>,
}
