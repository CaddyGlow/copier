use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};
use clap::ValueEnum;
use serde::Deserialize;
use strum::{Display, EnumString};

use crate::cli::{Cli, Commands, CopyArgs, PasteArgs, UpdateArgs};
use crate::error::{QuickctxError, Result};

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
    Heredoc,
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
    Copy(CopyConfig),
    Paste(PasteConfig),
    Update(UpdateConfig),
}

#[derive(Debug, Clone)]
pub struct CopyConfig {
    pub inputs: Vec<String>,
    pub output: Option<Utf8PathBuf>,
    pub format: OutputFormat,
    pub fence: FencePreference,
    pub respect_gitignore: bool,
    pub ignore_files: Vec<Utf8PathBuf>,
    pub excludes: Vec<String>,
}

impl CopyConfig {
    pub fn require_inputs(&self) -> Result<()> {
        if self.inputs.is_empty() {
            return Err(QuickctxError::InvalidArgument(
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
pub struct PasteConfig {
    pub source: InputSource,
    pub output_dir: Utf8PathBuf,
    pub conflict: ConflictStrategy,
}

#[derive(Debug, Clone)]
pub struct UpdateConfig {
    pub check_only: bool,
    pub yes: bool,
}

// ============================================================================
// Configuration Builders
// ============================================================================

struct CopyConfigBuilder {
    inputs: Vec<String>,
    output: Option<Utf8PathBuf>,
    format: OutputFormat,
    fence: FencePreference,
    respect_gitignore: bool,
    ignore_files: Vec<Utf8PathBuf>,
    excludes: Vec<String>,
}

impl CopyConfigBuilder {
    fn new() -> Self {
        Self {
            inputs: Vec::new(),
            output: None,
            format: OutputFormat::default(),
            fence: FencePreference::default(),
            respect_gitignore: true,
            ignore_files: Vec::new(),
            excludes: Vec::new(),
        }
    }

    fn with_file_config(mut self, file: &CopySection) -> Self {
        // Vecs: file values go first
        self.inputs = file.paths.clone();
        self.ignore_files = file.ignore_files.clone();
        self.excludes = file.exclude.clone();

        // Options: use file value if not already set
        if self.output.is_none() {
            self.output = file.output.clone();
        }
        if let Some(format) = file.format {
            self.format = format;
        }
        if let Some(fence) = file.fence {
            self.fence = fence;
        }
        if let Some(respect) = file.respect_gitignore {
            self.respect_gitignore = respect;
        }

        self
    }

    fn with_cli_args(mut self, args: &CopyArgs) -> Result<Self> {
        // Vecs: CLI values extend file values
        self.inputs
            .extend(args.paths.iter().map(|p| p.to_string_lossy().to_string()));
        self.excludes.extend(args.exclude.iter().cloned());

        for path in &args.ignore_file {
            self.ignore_files.push(to_utf8_path(path.clone())?);
        }

        // Options: CLI overrides file
        if let Some(path) = &args.output {
            self.output = Some(to_utf8_path(path.clone())?);
        }
        if let Some(format) = args.format {
            self.format = format;
        }
        if let Some(fence) = args.fence {
            self.fence = fence;
        }

        // Special: no_gitignore flag overrides everything
        if args.no_gitignore {
            self.respect_gitignore = false;
        }

        Ok(self)
    }

    fn build(self) -> CopyConfig {
        CopyConfig {
            inputs: self.inputs,
            output: self.output,
            format: self.format,
            fence: self.fence,
            respect_gitignore: self.respect_gitignore,
            ignore_files: self.ignore_files,
            excludes: self.excludes,
        }
    }
}

struct PasteConfigBuilder {
    output_dir: Utf8PathBuf,
    conflict: ConflictStrategy,
    source: Option<InputSource>,
}

impl PasteConfigBuilder {
    fn new(cwd: Utf8PathBuf) -> Self {
        Self {
            output_dir: cwd,
            conflict: ConflictStrategy::default(),
            source: None,
        }
    }

    fn with_file_config(mut self, file: &PasteSection) -> Self {
        if let Some(dir) = &file.output_dir {
            self.output_dir = dir.clone();
        }
        if let Some(conflict) = file.conflict {
            self.conflict = conflict;
        }
        self
    }

    fn with_cli_args(mut self, args: &PasteArgs) -> Result<Self> {
        if let Some(dir) = &args.output_dir {
            self.output_dir = to_utf8_path(dir.clone())?;
        }
        if let Some(conflict) = args.conflict {
            self.conflict = conflict;
        }

        self.source = Some(match &args.input {
            Some(path) => InputSource::File(to_utf8_path(path.clone())?),
            None => InputSource::Stdin,
        });

        Ok(self)
    }

    fn build(self) -> PasteConfig {
        PasteConfig {
            source: self.source.unwrap_or(InputSource::Stdin),
            output_dir: self.output_dir,
            conflict: self.conflict,
        }
    }
}

pub fn load(cli: &Cli) -> Result<RuntimeConfig> {
    let cwd = std::env::current_dir()?;
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
        Some(Commands::Copy(args)) => {
            let cfg = build_copy_config(Some(args), &cli.copy, &file_config)?;
            ModeConfig::Copy(cfg)
        }
        Some(Commands::Paste(args)) => {
            let cfg = build_paste_config(args, &file_config, &context)?;
            ModeConfig::Paste(cfg)
        }
        Some(Commands::Update(args)) => {
            let cfg = build_update_config(args);
            ModeConfig::Update(cfg)
        }
        None => {
            let cfg = build_copy_config(None, &cli.copy, &file_config)?;
            ModeConfig::Copy(cfg)
        }
    };

    Ok(RuntimeConfig { context, mode })
}

fn resolve_config_path(cli: &Cli, cwd: &Utf8Path) -> Option<Utf8PathBuf> {
    if let Some(path) = &cli.config {
        return to_utf8_path(path.clone()).ok();
    }

    let default = cwd.join("quickctx.toml");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn build_copy_config(
    override_args: Option<&CopyArgs>,
    default_args: &CopyArgs,
    file_config: &FileConfig,
) -> Result<CopyConfig> {
    let args = override_args.unwrap_or(default_args);

    let config = CopyConfigBuilder::new()
        .with_file_config(&file_config.copy)
        .with_cli_args(args)?
        .build();

    Ok(config)
}

fn build_paste_config(
    args: &PasteArgs,
    file_config: &FileConfig,
    context: &AppContext,
) -> Result<PasteConfig> {
    let config = PasteConfigBuilder::new(context.cwd.clone())
        .with_file_config(&file_config.paste)
        .with_cli_args(args)?
        .build();

    Ok(config)
}

fn build_update_config(args: &UpdateArgs) -> UpdateConfig {
    UpdateConfig {
        check_only: args.check_only,
        yes: args.yes,
    }
}

fn parse_file_config(path: &Utf8Path) -> Result<FileConfig> {
    let raw = fs::read_to_string(path)
        .map_err(|e| QuickctxError::Config(format!("failed to read {}: {}", path, e)))?;
    let de = toml::de::Deserializer::parse(&raw)
        .map_err(|err| QuickctxError::ConfigParse(err.to_string()))?;
    let file_config: FileConfig = serde_path_to_error::deserialize(de)
        .map_err(|err| QuickctxError::ConfigParse(err.to_string()))?;
    Ok(file_config)
}

fn to_utf8_path(path: PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path)
        .map_err(|p| QuickctxError::InvalidUtfPath(p.to_string_lossy().into_owned()))
}

/// Load analyze configuration from a config file
pub fn load_analyze_config(config_path: Option<&std::path::Path>) -> Result<AnalyzeSection> {
    if let Some(path) = config_path
        && path.exists()
    {
        let file_config = parse_file_config(
            &Utf8PathBuf::from_path_buf(path.to_path_buf())
                .map_err(|p| QuickctxError::InvalidUtfPath(p.to_string_lossy().into_owned()))?,
        )?;
        return Ok(file_config.analyze);
    }

    // Try default location
    let default_path = Utf8PathBuf::from("quickctx.toml");
    if default_path.exists() {
        let file_config = parse_file_config(&default_path)?;
        return Ok(file_config.analyze);
    }

    Ok(AnalyzeSection::default())
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    copy: CopySection,
    #[serde(default)]
    paste: PasteSection,
    #[serde(default)]
    general: GeneralSection,
    #[serde(default)]
    analyze: AnalyzeSection,
}

#[derive(Debug, Default, Deserialize)]
struct CopySection {
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
struct PasteSection {
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
    #[serde(default)]
    pub enable_cache: Option<bool>,
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
}
