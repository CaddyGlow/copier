use clap::Parser;
use copier::analyze::{
    LspClient, OutputFormat, SymbolIndex, SymbolInfo, TypeExtractor, TypeResolver,
    detect_project_root, extract_project_name, extract_symbols, get_formatter,
    get_lsp_server_with_config, has_lsp_support,
};
use copier::config::load_analyze_config;
use copier::error::Result;
use ignore::WalkBuilder;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

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
                        .entry(path.clone())
                        .or_insert_with(HashSet::new)
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
) -> Result<Vec<SymbolInfo>> {
    let mut external_symbols = Vec::new();

    for (file_path, symbol_names) in external_files {
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
    }

    Ok(external_symbols)
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

    /// Timeout in seconds to wait for diagnostics (default: 3)
    #[arg(long, default_value = "3")]
    diagnostics_timeout: u64,

    /// Don't respect .gitignore files when walking directories
    #[arg(long)]
    no_gitignore: bool,

    /// Include hidden files and directories (starting with .)
    #[arg(long)]
    hidden: bool,

    /// Timeout in seconds to wait for LSP server readiness (default: 10)
    #[arg(long, default_value = "10")]
    lsp_timeout: u64,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliOutputFormat {
    Markdown,
    Json,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Markdown => OutputFormat::Markdown,
            CliOutputFormat::Json => OutputFormat::Json,
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
    let expanded_files = expand_inputs(&args.inputs, respect_gitignore, args.hidden)?;

    if expanded_files.is_empty() {
        return Err(copier::error::CopierError::InvalidArgument(
            "No valid source files found with LSP support".to_string(),
        ));
    }

    tracing::info!("Processing {} file(s)", expanded_files.len());

    // Create new Args with expanded files
    let mut expanded_args = args.clone();
    expanded_args.inputs = expanded_files;

    // Route to appropriate mode
    if expanded_args.diagnostics {
        // Diagnostics mode
        if expanded_args.inputs.len() == 1 {
            run_diagnostics_single_file(&expanded_args)
        } else {
            run_diagnostics_multiple_files(&expanded_args)
        }
    } else {
        // Symbol extraction mode (with type dependencies)
        if expanded_args.inputs.len() == 1 {
            run_single_file(&expanded_args)
        } else {
            run_multiple_files(&expanded_args)
        }
    }
}

/// Expand inputs: files are kept as-is, directories are walked recursively
fn expand_inputs(
    inputs: &[PathBuf],
    respect_gitignore: bool,
    include_hidden: bool,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for input in inputs {
        if input.is_file() {
            // Keep file as-is (will be validated later for LSP support)
            files.push(input.clone());
        } else if input.is_dir() {
            // Walk directory recursively
            walk_directory(input, respect_gitignore, include_hidden, &mut files)?;
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
) -> Result<()> {
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
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
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

    Ok(())
}

fn run_single_file(args: &Args) -> Result<()> {
    let input = &args.inputs[0];

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    // Detect project root and type
    let (root_path, project_type) = if let Some(ref root) = args.project_root {
        // User-specified root, try to detect type or use Unknown
        let (detected_root, detected_type) = detect_project_root(root)?;
        if detected_root == *root {
            (root.clone(), detected_type)
        } else {
            (root.clone(), copier::analyze::ProjectType::Unknown)
        }
    } else {
        detect_project_root(input)?
    };

    let project_name = extract_project_name(&root_path, project_type);

    tracing::info!(
        "Project: {} ({:?}) at {}",
        project_name,
        project_type,
        root_path.display()
    );

    // Get LSP server configuration (priority: CLI flag > config file > defaults)
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::from_command_string(cmd)
    } else {
        get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Canonicalize input file path
    let input_path = input
        .canonicalize()
        .map_err(copier::error::CopierError::Io)?;

    // Read file content
    let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

    // Create LSP client with custom bin paths
    let mut client = LspClient::new_with_paths(
        &lsp_config.command,
        &lsp_config.args,
        &root_path,
        project_type,
        &config.bin_paths,
    )?;

    // Initialize LSP
    tracing::info!("Initializing LSP...");
    client.initialize()?;

    // Wait for LSP server to complete indexing
    let timeout_secs = config
        .lsp_readiness_timeout_secs
        .unwrap_or(args.lsp_timeout);
    client.wait_for_indexing(timeout_secs)?;

    // Get file URI
    let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
        copier::error::CopierError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid file path",
        ))
    })?;

    // Open document
    tracing::info!("Opening document...");
    client.did_open(&input_path, &content)?;

    // Extract symbols
    tracing::info!("Extracting symbols...");
    let mut symbols = extract_symbols(&mut client, &file_uri)?;

    tracing::info!("Found {} symbols", symbols.len());

    // Build symbol index for type resolution
    let file_symbols = vec![(input_path.clone(), symbols.clone())];
    let symbol_index = SymbolIndex::build_from_symbols(&file_symbols);

    // Populate type dependencies
    let type_extractor = TypeExtractor::new(project_type);
    let type_resolver = TypeResolver::new(&symbol_index, true);
    populate_type_dependencies(
        &mut symbols,
        &type_extractor,
        &type_resolver,
        &file_uri,
        &mut client,
    );

    let total_types: usize = symbols
        .iter()
        .filter_map(|s| s.type_dependencies.as_ref().map(|t| t.len()))
        .sum();
    if total_types > 0 {
        tracing::info!("Associated {} type references with symbols", total_types);
    }

    // Collect and fetch external type definitions
    let external_files = collect_external_types(&symbols);
    if !external_files.is_empty() {
        tracing::info!(
            "Found {} external files with type definitions",
            external_files.len()
        );
        match fetch_external_symbols(&external_files, &mut client) {
            Ok(mut external_symbols) => {
                if !external_symbols.is_empty() {
                    tracing::info!(
                        "Fetched {} external type definitions",
                        external_symbols.len()
                    );
                    symbols.append(&mut external_symbols);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch external symbols: {}", e);
            }
        }
    }

    // Calculate relative path from project root
    let relative_path = input_path
        .strip_prefix(&root_path)
        .unwrap_or(&input_path)
        .display()
        .to_string();

    // Format output
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format(&symbols, &relative_path);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Analysis written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    // Shutdown LSP
    client.shutdown()?;

    Ok(())
}

fn run_multiple_files(args: &Args) -> Result<()> {
    use std::collections::HashMap;

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    tracing::info!("Analyzing {} files", args.inputs.len());

    // First pass: detect project root and type for each file
    // Group files by (root_path, project_type)
    let mut file_groups: HashMap<(PathBuf, copier::analyze::ProjectType), Vec<PathBuf>> =
        HashMap::new();

    for input in &args.inputs {
        let (root_path, project_type) = if let Some(ref root) = args.project_root {
            // User-specified root, try to detect type or use Unknown
            let (detected_root, detected_type) = detect_project_root(root)?;
            if detected_root == *root {
                (root.clone(), detected_type)
            } else {
                (root.clone(), copier::analyze::ProjectType::Unknown)
            }
        } else {
            detect_project_root(input)?
        };

        tracing::info!(
            "File {} -> root: {}, type: {:?}",
            input.display(),
            root_path.display(),
            project_type
        );

        file_groups
            .entry((root_path, project_type))
            .or_insert_with(Vec::new)
            .push(input.clone());
    }

    tracing::info!("Files grouped into {} project(s)", file_groups.len());

    // Process each group with its own LSP client
    // Track projects: (project_name, project_type, Vec<(file_path, symbols)>)
    let mut projects: Vec<(
        String,
        copier::analyze::ProjectType,
        Vec<(String, Vec<copier::analyze::SymbolInfo>)>,
    )> = Vec::new();

    for ((root_path, project_type), files) in file_groups {
        let project_name = extract_project_name(&root_path, project_type);

        tracing::info!(
            "Processing {} files in project: {} ({:?}) at {}",
            files.len(),
            project_name,
            project_type,
            root_path.display()
        );

        // Get LSP server configuration (priority: CLI flag > config file > defaults)
        let lsp_config = if let Some(ref cmd) = args.lsp_server {
            copier::analyze::LspServerConfig::from_command_string(cmd)
        } else {
            get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
        };

        tracing::info!("Using LSP server: {}", lsp_config.command);

        // Create LSP client for this project group
        let mut client = LspClient::new_with_paths(
            &lsp_config.command,
            &lsp_config.args,
            &root_path,
            project_type,
            &config.bin_paths,
        )?;

        // Initialize LSP
        tracing::info!("Initializing LSP...");
        client.initialize()?;

        // Wait for LSP server to complete indexing
        let timeout_secs = config
            .lsp_readiness_timeout_secs
            .unwrap_or(args.lsp_timeout);
        client.wait_for_indexing(timeout_secs)?;

        // First pass: collect all symbols from all files
        let mut all_file_symbols = Vec::new();

        for input in &files {
            let input_path = input
                .canonicalize()
                .map_err(copier::error::CopierError::Io)?;
            let content =
                fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path",
                ))
            })?;

            tracing::info!("Opening document: {}", input.display());
            client.did_open(&input_path, &content)?;

            tracing::info!("Extracting symbols...");
            let symbols = extract_symbols(&mut client, &file_uri)?;

            tracing::info!("Found {} symbols in {}", symbols.len(), input.display());

            all_file_symbols.push((input_path, symbols));
        }

        // Build symbol index from all files
        let symbol_index = SymbolIndex::build_from_symbols(&all_file_symbols);
        tracing::info!("Built symbol index with {} types", symbol_index.len());

        // Second pass: populate type dependencies
        let type_extractor = TypeExtractor::new(project_type);
        let type_resolver = TypeResolver::new(&symbol_index, true);
        let mut project_files = Vec::new();

        for (input_path, symbols) in &all_file_symbols {
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
                &mut client,
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
                .strip_prefix(&root_path)
                .unwrap_or(input_path)
                .display()
                .to_string();

            project_files.push((relative_path, mutable_symbols));
        }

        // Collect and fetch external type definitions for this project
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
                project_name
            );
            match fetch_external_symbols(&external_files, &mut client) {
                Ok(external_symbols) => {
                    if !external_symbols.is_empty() {
                        tracing::info!(
                            "Fetched {} external type definitions",
                            external_symbols.len()
                        );
                        // Add external symbols as a separate "file" in the project
                        let relative_path = format!("_external_dependencies_{}", project_name);
                        project_files.push((relative_path, external_symbols));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch external symbols: {}", e);
                }
            }
        }

        // Shutdown LSP for this group
        client.shutdown()?;
        tracing::info!("Completed analysis for project: {}", project_name);

        projects.push((project_name, project_type, project_files));
    }

    // Format output for all projects
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_by_projects(&projects);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Analysis written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    tracing::info!("Successfully analyzed {} files", args.inputs.len());

    Ok(())
}

fn run_diagnostics_single_file(args: &Args) -> Result<()> {
    let input = &args.inputs[0];

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    // Detect project root and type
    let (root_path, project_type) = if let Some(ref root) = args.project_root {
        let (detected_root, detected_type) = detect_project_root(root)?;
        if detected_root == *root {
            (root.clone(), detected_type)
        } else {
            (root.clone(), copier::analyze::ProjectType::Unknown)
        }
    } else {
        detect_project_root(input)?
    };

    let project_name = extract_project_name(&root_path, project_type);

    tracing::info!(
        "Project: {} ({:?}) at {}",
        project_name,
        project_type,
        root_path.display()
    );

    // Get LSP server configuration
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::from_command_string(cmd)
    } else {
        get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Canonicalize input file path
    let input_path = input
        .canonicalize()
        .map_err(copier::error::CopierError::Io)?;

    // Read file content
    let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

    // Create LSP client
    let mut client = LspClient::new_with_paths(
        &lsp_config.command,
        &lsp_config.args,
        &root_path,
        project_type,
        &config.bin_paths,
    )?;

    // Initialize LSP
    tracing::info!("Initializing LSP...");
    client.initialize()?;

    // Open document
    tracing::info!("Opening document...");
    client.did_open(&input_path, &content)?;

    // Collect diagnostics
    let timeout_ms = args.diagnostics_timeout * 1000;
    let diagnostics_map = client.collect_diagnostics(timeout_ms)?;

    // Get file URI
    let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
        copier::error::CopierError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid file path",
        ))
    })?;

    // Extract diagnostics for this file
    let diagnostics = diagnostics_map.get(&file_uri).cloned().unwrap_or_default();

    tracing::info!("Found {} diagnostic(s)", diagnostics.len());

    // Calculate relative path
    let relative_path = input_path
        .strip_prefix(&root_path)
        .unwrap_or(&input_path)
        .display()
        .to_string();

    // Build project diagnostics structure
    let file_diags = copier::analyze::FileDiagnostics {
        file_path: relative_path,
        diagnostics,
    };

    let project_diags = copier::analyze::ProjectDiagnostics {
        project_name,
        project_type,
        files: vec![file_diags],
    };

    // Format output
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_diagnostics(&[project_diags]);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Diagnostics written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    // Shutdown LSP
    client.shutdown()?;

    Ok(())
}

fn run_diagnostics_multiple_files(args: &Args) -> Result<()> {
    use std::collections::HashMap;

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    tracing::info!("Analyzing {} files for diagnostics", args.inputs.len());

    // Group files by (root_path, project_type)
    let mut file_groups: HashMap<(PathBuf, copier::analyze::ProjectType), Vec<PathBuf>> =
        HashMap::new();

    for input in &args.inputs {
        let (root_path, project_type) = if let Some(ref root) = args.project_root {
            let (detected_root, detected_type) = detect_project_root(root)?;
            if detected_root == *root {
                (root.clone(), detected_type)
            } else {
                (root.clone(), copier::analyze::ProjectType::Unknown)
            }
        } else {
            detect_project_root(input)?
        };

        tracing::info!(
            "File {} -> root: {}, type: {:?}",
            input.display(),
            root_path.display(),
            project_type
        );

        file_groups
            .entry((root_path, project_type))
            .or_insert_with(Vec::new)
            .push(input.clone());
    }

    tracing::info!("Files grouped into {} project(s)", file_groups.len());

    // Process each group
    let mut projects: Vec<copier::analyze::ProjectDiagnostics> = Vec::new();

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
            copier::analyze::LspServerConfig::from_command_string(cmd)
        } else {
            get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
        };

        tracing::info!("Using LSP server: {}", lsp_config.command);

        // Create LSP client
        let mut client = LspClient::new_with_paths(
            &lsp_config.command,
            &lsp_config.args,
            &root_path,
            project_type,
            &config.bin_paths,
        )?;

        // Initialize LSP
        tracing::info!("Initializing LSP...");
        client.initialize()?;

        // Open all documents
        for input in &files {
            let input_path = input
                .canonicalize()
                .map_err(copier::error::CopierError::Io)?;
            let content =
                fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            tracing::info!("Opening document: {}", input.display());
            client.did_open(&input_path, &content)?;
        }

        // Collect diagnostics
        let timeout_ms = args.diagnostics_timeout * 1000;
        let diagnostics_map = client.collect_diagnostics(timeout_ms)?;

        // Build file diagnostics
        let mut file_diagnostics = Vec::new();

        for input in &files {
            let input_path = input
                .canonicalize()
                .map_err(copier::error::CopierError::Io)?;

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

            // Calculate relative path
            let relative_path = input_path
                .strip_prefix(&root_path)
                .unwrap_or(&input_path)
                .display()
                .to_string();

            file_diagnostics.push(copier::analyze::FileDiagnostics {
                file_path: relative_path,
                diagnostics,
            });
        }

        // Shutdown LSP for this group
        client.shutdown()?;
        tracing::info!(
            "Completed diagnostics collection for project: {}",
            project_name
        );

        projects.push(copier::analyze::ProjectDiagnostics {
            project_name,
            project_type,
            files: file_diagnostics,
        });
    }

    // Format output
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_diagnostics(&projects);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Diagnostics written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    tracing::info!(
        "Successfully collected diagnostics for {} files",
        args.inputs.len()
    );

    Ok(())
}
