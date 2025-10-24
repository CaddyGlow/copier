use clap::Parser;
use copier::analyze::{
    LspClient, OutputFormat, ProjectType, RelativePath, SymbolIndex, SymbolInfo, TypeExtractor,
    TypeResolver, detect_project_root, extract_project_name, extract_symbols, get_formatter,
    get_lsp_server_with_config, has_lsp_support, LspServerConfig,
};
use copier::config::{AnalyzeSection, load_analyze_config};
use copier::error::Result;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

/// Project context containing project-level information
struct ProjectContext {
    root_path: PathBuf,
    project_type: ProjectType,
    project_name: String,
    lsp_config: LspServerConfig,
}

/// Shared processing context
#[allow(dead_code)]
struct ProcessingContext<'a> {
    config: &'a AnalyzeSection,
    progress: &'a copier::analyze::progress::ProgressDisplay,
    args: &'a Args,
}

/// Recursively populate type dependencies for symbols
fn populate_type_dependencies(
    symbols: &mut [SymbolInfo],
    type_extractor: &TypeExtractor,
    type_resolver: &TypeResolver,
    uri: &lsp_types::Url,
    client: &mut LspClient,
) {
    for symbol in symbols.iter_mut() {
        // Extract types for this symbol
        let type_refs = type_extractor.extract_types(symbol, uri);

        if !type_refs.is_empty() {
            // Resolve the types
            let resolved = type_resolver.resolve_types(&type_refs, Some(client));
            symbol.type_dependencies = Some(resolved);
        }

        // Recursively process children
        if !symbol.children.is_empty() {
            populate_type_dependencies(
                &mut symbol.children,
                type_extractor,
                type_resolver,
                uri,
                client,
            );
        }
    }
}

/// Collect external type definitions referenced by symbols
fn collect_external_types(
    symbols: &[SymbolInfo],
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    use copier::analyze::type_resolver::TypeResolution;
    use std::collections::{HashMap, HashSet};

    let mut external_files: HashMap<String, HashSet<String>> = HashMap::new();

    fn collect_from_symbol(
        symbol: &SymbolInfo,
        external_files: &mut HashMap<String, HashSet<String>>,
    ) {
        // Check type dependencies
        if let Some(type_deps) = &symbol.type_dependencies {
            for resolved_type in type_deps {
                if let TypeResolution::External {
                    file_path: Some(path),
                    ..
                } = &resolved_type.resolution
                {
                    external_files
                        .entry(path.to_string())
                        .or_default()
                        .insert(resolved_type.type_name.clone());
                }
            }
        }

        // Recursively process children
        for child in &symbol.children {
            collect_from_symbol(child, external_files);
        }
    }

    for symbol in symbols {
        collect_from_symbol(symbol, &mut external_files);
    }

    external_files
}

/// Fetch external symbol definitions from their source files
fn fetch_external_symbols(
    external_files: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    client: &mut LspClient,
    progress: Option<&copier::analyze::progress::ProgressDisplay>,
) -> Result<Vec<SymbolInfo>> {
    let mut external_symbols = Vec::new();
    let pb = progress.map(|p| {
        let bar = p.progress_bar(external_files.len() as u64, "[4/4]");
        bar.set_message("Fetching external types");
        bar
    });

    for (file_path, symbol_names) in external_files {
        if let Some(ref bar) = pb {
            bar.set_message(format!("Fetching external types\n{}", file_path));
        }
        let path = PathBuf::from(file_path);

        // Read the external file
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read external file {}: {}", file_path, e);
                continue;
            }
        };

        // Convert to URI
        let file_uri = match lsp_types::Url::from_file_path(&path) {
            Ok(uri) => uri,
            Err(_) => {
                tracing::warn!("Failed to convert path to URI: {}", file_path);
                continue;
            }
        };

        // Open document in LSP
        if let Err(e) = client.did_open(&path, &content) {
            tracing::warn!("Failed to open external file in LSP: {}", e);
            continue;
        }

        // Extract symbols
        match extract_symbols(client, &file_uri) {
            Ok(symbols) => {
                // Filter to only the symbols we need
                for symbol in symbols {
                    if symbol_names.contains(&symbol.name) {
                        external_symbols.push(symbol);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to extract symbols from {}: {}", file_path, e);
            }
        }

        if let Some(ref bar) = pb {
            bar.inc(1);
        }
    }

    if let Some(bar) = pb {
        bar.finish_and_clear();
        eprintln!("[4/4] ✓ Fetching external types");
    }

    Ok(external_symbols)
}

/// Group files by project (root_path, project_type)
fn group_files_by_project(
    files: &[PathBuf],
    args: &Args,
) -> Result<HashMap<(PathBuf, ProjectType), Vec<PathBuf>>> {
    let mut file_groups: HashMap<(PathBuf, ProjectType), Vec<PathBuf>> = HashMap::new();

    for file in files {
        let (root_path, project_type) = if let Some(ref root) = args.project_root {
            let (detected_root, detected_type) = detect_project_root(root)?;
            if detected_root == *root {
                (root.clone(), detected_type)
            } else {
                (root.clone(), ProjectType::Unknown)
            }
        } else {
            detect_project_root(file)?
        };

        tracing::debug!(
            "File {} -> root: {}, type: {:?}",
            file.display(),
            root_path.display(),
            project_type
        );

        file_groups
            .entry((root_path, project_type))
            .or_default()
            .push(file.clone());
    }

    Ok(file_groups)
}

/// Helper to manage LSP client lifecycle with progress
fn with_lsp_client<F, R>(
    project: &ProjectContext,
    config: &AnalyzeSection,
    progress: &copier::analyze::progress::ProgressDisplay,
    timeout: u64,
    f: F,
) -> Result<R>
where
    F: FnOnce(&mut LspClient) -> Result<R>,
{
    let spinner = progress.spinner(format!("Starting LSP server ({})", project.lsp_config.command));
    let mut client = LspClient::new_with_paths(
        &project.lsp_config.command,
        &project.lsp_config.args,
        &project.root_path,
        project.project_type,
        &config.bin_paths,
    )?;

    spinner.set_message("Initializing LSP server...");
    tracing::info!("Initializing LSP...");
    client.initialize()?;
    spinner.finish_and_clear();

    let lsp_progress_mgr = progress.lsp_progress_manager();
    client.wait_for_indexing(timeout, Some(&lsp_progress_mgr))?;

    let result = f(&mut client)?;
    client.shutdown()?;
    Ok(result)
}

/// Write output to file or stdout
fn write_output(content: &str, output_path: Option<&std::path::Path>) -> Result<()> {
    if let Some(path) = output_path {
        fs::write(path, content).map_err(copier::error::CopierError::Io)?;
        println!("Analysis written to: {}", path.display());
    } else {
        print!("{}", content);
    }
    Ok(())
}

/// Trait for different processing modes (symbols vs diagnostics)
trait ProcessingMode {
    type FileOutput;
    type ProjectOutput;

    fn process_files(
        &self,
        client: &mut LspClient,
        files: &[PathBuf],
        project: &ProjectContext,
        ctx: &ProcessingContext,
    ) -> Result<Self::ProjectOutput>;

    fn format_output(
        &self,
        outputs: Vec<Self::ProjectOutput>,
        format: OutputFormat,
    ) -> String;
}

/// Symbol extraction mode
struct SymbolMode;

impl ProcessingMode for SymbolMode {
    type FileOutput = (String, Vec<SymbolInfo>);
    type ProjectOutput = (String, ProjectType, Vec<Self::FileOutput>);

    fn process_files(
        &self,
        client: &mut LspClient,
        files: &[PathBuf],
        project: &ProjectContext,
        ctx: &ProcessingContext,
    ) -> Result<Self::ProjectOutput> {
        // First pass: collect all symbols from all files
        let mut all_file_symbols = Vec::new();
        let pb = ctx.progress.progress_bar(files.len() as u64, "[2/4]");
        pb.set_message("Extracting symbols");

        for input in files {
            pb.set_message(format!("Extracting symbols\n{}", input.display()));

            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;
            let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path",
                ))
            })?;

            tracing::info!("Opening document: {}", input.display());
            client.did_open(&input_path, &content)?;

            tracing::info!("Extracting symbols...");
            let symbols = extract_symbols(client, &file_uri)?;

            tracing::info!("Found {} symbols in {}", symbols.len(), input.display());

            all_file_symbols.push((input_path, symbols));
            pb.inc(1);
        }
        pb.finish_and_clear();
        eprintln!("[2/4] ✓ Extracting symbols");

        // Build symbol index from all files
        let symbol_index = SymbolIndex::build_from_symbols(&all_file_symbols);
        tracing::info!("Built symbol index with {} types", symbol_index.len());

        // Second pass: populate type dependencies
        let type_extractor = TypeExtractor::new(project.project_type);
        let type_resolver = TypeResolver::new(&symbol_index, true);
        let mut project_files = Vec::new();
        let pb2 = ctx.progress.progress_bar(all_file_symbols.len() as u64, "[3/4]");
        pb2.set_message("Resolving types");

        for (input_path, symbols) in &all_file_symbols {
            pb2.set_message(format!("Resolving types\n{}", input_path.display()));
            let file_uri = lsp_types::Url::from_file_path(input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path for URI conversion",
                ))
            })?;

            let mut mutable_symbols = symbols.clone();
            populate_type_dependencies(
                &mut mutable_symbols,
                &type_extractor,
                &type_resolver,
                &file_uri,
                client,
            );

            let total_types: usize = mutable_symbols
                .iter()
                .filter_map(|s| s.type_dependencies.as_ref().map(|t| t.len()))
                .sum();
            if total_types > 0 {
                tracing::info!(
                    "Associated {} type references with symbols in {}",
                    total_types,
                    input_path.display()
                );
            }

            // Calculate relative path
            let relative_path = input_path
                .strip_prefix(&project.root_path)
                .unwrap_or(input_path)
                .display()
                .to_string();

            project_files.push((relative_path, mutable_symbols));
            pb2.inc(1);
        }
        pb2.finish_and_clear();
        eprintln!("[3/4] ✓ Resolving types");

        // Collect and fetch external type definitions
        let all_symbols: Vec<&SymbolInfo> = project_files
            .iter()
            .flat_map(|(_, syms)| syms.iter())
            .collect();

        let external_files =
            collect_external_types(&all_symbols.iter().map(|&s| s.clone()).collect::<Vec<_>>());
        if !external_files.is_empty() {
            tracing::info!(
                "Found {} external files with type definitions for project {}",
                external_files.len(),
                project.project_name
            );
            match fetch_external_symbols(&external_files, client, Some(ctx.progress)) {
                Ok(external_symbols) => {
                    if !external_symbols.is_empty() {
                        tracing::info!(
                            "Fetched {} external type definitions",
                            external_symbols.len()
                        );
                        let relative_path =
                            format!("_external_dependencies_{}", project.project_name);
                        project_files.push((relative_path, external_symbols));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch external symbols: {}", e);
                }
            }
        }

        Ok((
            project.project_name.clone(),
            project.project_type,
            project_files,
        ))
    }

    fn format_output(
        &self,
        outputs: Vec<Self::ProjectOutput>,
        format: OutputFormat,
    ) -> String {
        let formatter = get_formatter(format);
        formatter.format_by_projects(&outputs)
    }
}

/// Diagnostics collection mode
struct DiagnosticsMode {
    timeout_ms: u64,
}

impl ProcessingMode for DiagnosticsMode {
    type FileOutput = copier::analyze::FileDiagnostics;
    type ProjectOutput = copier::analyze::ProjectDiagnostics;

    fn process_files(
        &self,
        client: &mut LspClient,
        files: &[PathBuf],
        project: &ProjectContext,
        ctx: &ProcessingContext,
    ) -> Result<Self::ProjectOutput> {
        // Open all documents with progress
        let pb = ctx.progress.progress_bar(files.len() as u64, "[2/3]");
        pb.set_message("Opening files");

        for input in files {
            pb.set_message(format!("Opening files\n{}", input.display()));
            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;
            let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            tracing::info!("Opening document: {}", input.display());
            client.did_open(&input_path, &content)?;
            pb.inc(1);
        }
        pb.finish_and_clear();
        eprintln!("[2/3] ✓ Opening files");

        // Collect diagnostics with progress
        let diagnostics_map =
            client.collect_diagnostics(self.timeout_ms, Some(files.len()), Some(ctx.progress))?;

        // Build file diagnostics
        let mut file_diagnostics = Vec::new();

        for input in files {
            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

            let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path",
                ))
            })?;

            let diagnostics = diagnostics_map.get(&file_uri).cloned().unwrap_or_default();

            tracing::info!(
                "Found {} diagnostic(s) in {}",
                diagnostics.len(),
                input.display()
            );

            let relative_path = input_path
                .strip_prefix(&project.root_path)
                .unwrap_or(&input_path)
                .display()
                .to_string();

            file_diagnostics.push(copier::analyze::FileDiagnostics {
                file_path: RelativePath::from_string(relative_path),
                diagnostics,
            });
        }

        Ok(copier::analyze::ProjectDiagnostics {
            project_name: project.project_name.clone(),
            project_type: project.project_type,
            files: file_diagnostics,
        })
    }

    fn format_output(
        &self,
        outputs: Vec<Self::ProjectOutput>,
        format: OutputFormat,
    ) -> String {
        let formatter = get_formatter(format);
        formatter.format_diagnostics(&outputs)
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "copier-analyze",
    author,
    version,
    about = "Analyze source code using LSP to extract symbols, documentation, and types"
)]
struct Args {
    /// Source file(s) to analyze
    #[arg(value_name = "FILE", required = true)]
    inputs: Vec<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "markdown")]
    format: CliOutputFormat,

    /// Output file (defaults to stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to configuration file (defaults to copier.toml if present)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Override project root directory
    #[arg(long)]
    project_root: Option<PathBuf>,

    /// Override LSP server command
    #[arg(long)]
    lsp_server: Option<String>,

    /// Increase log verbosity (repeatable)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Show diagnostics (errors/warnings) instead of symbols
    #[arg(long)]
    diagnostics: bool,

    /// Timeout in seconds to wait for diagnostics (default: 30)
    #[arg(long, default_value = "30")]
    diagnostics_timeout: u64,

    /// Don't respect .gitignore files when walking directories
    #[arg(long)]
    no_gitignore: bool,

    /// Include hidden files and directories (starting with .)
    #[arg(long)]
    hidden: bool,

    /// Timeout in seconds to wait for LSP server readiness (default: 30)
    #[arg(long, default_value = "30")]
    lsp_timeout: u64,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliOutputFormat {
    Markdown,
    Json,
    Csv,
    Compact,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Markdown => OutputFormat::Markdown,
            CliOutputFormat::Json => OutputFormat::Json,
            CliOutputFormat::Csv => OutputFormat::Csv,
            CliOutputFormat::Compact => OutputFormat::Compact,
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .with_target(false)
        .init();

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<()> {
    // Create progress display based on verbosity
    let progress = copier::analyze::progress::ProgressDisplay::new(args.verbose);

    // Validate all input paths exist
    for input in &args.inputs {
        if !input.exists() {
            return Err(copier::error::CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Input path not found: {}", input.display()),
            )));
        }
    }

    // Expand inputs: files stay as-is, directories are walked recursively
    let respect_gitignore = !args.no_gitignore;
    let expanded_files = expand_inputs(&args.inputs, respect_gitignore, args.hidden, Some(&progress))?;

    if expanded_files.is_empty() {
        return Err(copier::error::CopierError::InvalidArgument(
            "No valid source files found with LSP support".to_string(),
        ));
    }

    tracing::info!("Processing {} file(s)", expanded_files.len());

    // Create new Args with expanded files
    let mut expanded_args = args.clone();
    expanded_args.inputs = expanded_files;

    // Route to appropriate mode using unified processor
    if expanded_args.diagnostics {
        let mode = DiagnosticsMode {
            timeout_ms: expanded_args.diagnostics_timeout * 1000,
        };
        process_with_mode(&expanded_args, mode, &progress)
    } else {
        process_with_mode(&expanded_args, SymbolMode, &progress)
    }
}

/// Expand inputs: files are kept as-is, directories are walked recursively
fn expand_inputs(
    inputs: &[PathBuf],
    respect_gitignore: bool,
    include_hidden: bool,
    progress: Option<&copier::analyze::progress::ProgressDisplay>,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for input in inputs {
        if input.is_file() {
            // Keep file as-is (will be validated later for LSP support)
            files.push(input.clone());
        } else if input.is_dir() {
            // Walk directory recursively
            walk_directory(input, respect_gitignore, include_hidden, &mut files, progress)?;
        }
    }

    Ok(files)
}

/// Walk a directory recursively and collect source files with LSP support
fn walk_directory(
    dir: &PathBuf,
    respect_gitignore: bool,
    include_hidden: bool,
    files: &mut Vec<PathBuf>,
    progress: Option<&copier::analyze::progress::ProgressDisplay>,
) -> Result<()> {
    let spinner = progress.map(|p| p.spinner(format!("[1/4] Finding source files")));

    let mut builder = WalkBuilder::new(dir);
    builder.follow_links(false);
    builder.sort_by_file_name(|a, b| a.cmp(b));

    // Handle gitignore
    if respect_gitignore {
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);
        builder.require_git(false);
    } else {
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);
    }

    // Handle hidden files
    if include_hidden {
        builder.standard_filters(false); // Don't skip hidden files
    } else {
        builder.standard_filters(true); // Skip hidden files
    }

    let walker = builder.build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!("Failed to read entry: {}", err);
                continue;
            }
        };

        // Only process regular files
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.into_path();

        // Only include files with known LSP support
        if has_lsp_support(&path) {
            files.push(path);
        } else {
            tracing::debug!("Skipping file without LSP support: {}", path.display());
        }
    }

    if let Some(pb) = spinner {
        pb.finish_and_clear();
        eprintln!("[1/4] ✓ Finding source files");
    }

    Ok(())
}

/// Unified file processing with any mode
fn process_with_mode<M: ProcessingMode>(
    args: &Args,
    mode: M,
    progress: &copier::analyze::progress::ProgressDisplay,
) -> Result<()> {
    let config = load_analyze_config(args.config.as_deref())?;
    let file_groups = group_files_by_project(&args.inputs, args)?;

    tracing::info!("Files grouped into {} project(s)", file_groups.len());

    let mut all_outputs = Vec::new();

    for ((root_path, project_type), files) in file_groups {
        let project_name = extract_project_name(&root_path, project_type);

        tracing::info!(
            "Processing {} files in project: {} ({:?}) at {}",
            files.len(),
            project_name,
            project_type,
            root_path.display()
        );

        // Get LSP server configuration
        let lsp_config = if let Some(ref cmd) = args.lsp_server {
            LspServerConfig::from_command_string(cmd)
        } else {
            get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
        };

        tracing::info!("Using LSP server: {}", lsp_config.command);

        let project_ctx = ProjectContext {
            root_path,
            project_type,
            project_name,
            lsp_config,
        };

        let timeout_secs = config.lsp_readiness_timeout_secs.unwrap_or(args.lsp_timeout);

        let output = with_lsp_client(&project_ctx, &config, progress, timeout_secs, |client| {
            let ctx = ProcessingContext {
                config: &config,
                progress,
                args,
            };
            mode.process_files(client, &files, &project_ctx, &ctx)
        })?;

        all_outputs.push(output);
        tracing::info!("Completed processing for project: {}", project_ctx.project_name);
    }

    // Format and write output
    let formatted = mode.format_output(all_outputs, args.format.into());
    write_output(&formatted, args.output.as_deref())?;

    tracing::info!("Successfully processed {} files", args.inputs.len());
    Ok(())
}

