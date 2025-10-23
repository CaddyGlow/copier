use clap::Parser;
use copier::analyze::{
    detect_project_root, extract_project_name, extract_symbols, get_formatter,
    get_lsp_server_with_config, has_lsp_support, LspClient, OutputFormat, SymbolIndex,
    TypeExtractor, TypeResolver,
};
use copier::config::load_analyze_config;
use copier::error::Result;
use ignore::WalkBuilder;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

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

    /// Analyze type dependencies and show where types are defined
    #[arg(long)]
    type_deps: bool,

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
            "No valid source files found with LSP support".to_string()
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
    } else if expanded_args.type_deps {
        // Type dependencies mode
        if expanded_args.inputs.len() == 1 {
            run_type_deps_single_file(&expanded_args)
        } else {
            run_type_deps_multiple_files(&expanded_args)
        }
    } else {
        // Symbol extraction mode
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

    tracing::info!("Project: {} ({:?}) at {}", project_name, project_type, root_path.display());

    // Get LSP server configuration (priority: CLI flag > config file > defaults)
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::from_command_string(cmd)
    } else {
        get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Canonicalize input file path
    let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

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
    let symbols = extract_symbols(&mut client, &file_uri)?;

    tracing::info!("Found {} symbols", symbols.len());

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

    tracing::info!(
        "Files grouped into {} project(s)",
        file_groups.len()
    );

    // Process each group with its own LSP client
    // Track projects: (project_name, project_type, Vec<(file_path, symbols)>)
    let mut projects: Vec<(String, copier::analyze::ProjectType, Vec<(String, Vec<copier::analyze::SymbolInfo>)>)> = Vec::new();

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

        // Process each file in this group
        let mut project_files = Vec::new();

        for input in &files {
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
            let symbols = extract_symbols(&mut client, &file_uri)?;

            tracing::info!("Found {} symbols in {}", symbols.len(), input.display());

            // Calculate relative path
            let relative_path = input_path
                .strip_prefix(&root_path)
                .unwrap_or(&input_path)
                .display()
                .to_string();

            project_files.push((relative_path, symbols));
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

    tracing::info!("Project: {} ({:?}) at {}", project_name, project_type, root_path.display());

    // Get LSP server configuration
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::from_command_string(cmd)
    } else {
        get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Canonicalize input file path
    let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

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
            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;
            let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            tracing::info!("Opening document: {}", input.display());
            client.did_open(&input_path, &content)?;
        }

        // Collect diagnostics
        let timeout_ms = args.diagnostics_timeout * 1000;
        let diagnostics_map = client.collect_diagnostics(timeout_ms)?;

        // Build file diagnostics
        let mut file_diagnostics = Vec::new();

        for input in &files {
            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

            let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path",
                ))
            })?;

            let diagnostics = diagnostics_map.get(&file_uri).cloned().unwrap_or_default();

            tracing::info!("Found {} diagnostic(s) in {}", diagnostics.len(), input.display());

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
        tracing::info!("Completed diagnostics collection for project: {}", project_name);

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

    tracing::info!("Successfully collected diagnostics for {} files", args.inputs.len());

    Ok(())
}

fn run_type_deps_single_file(args: &Args) -> Result<()> {
    let input = &args.inputs[0];

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    // Canonicalize path
    let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

    // Detect project root
    let (root_path, project_type) = if let Some(ref override_root) = args.project_root {
        let project_type = copier::analyze::ProjectType::Rust; // Default to Rust when overridden
        (override_root.clone(), project_type)
    } else {
        detect_project_root(&input_path)?
    };

    let project_name = extract_project_name(&root_path, project_type);

    tracing::info!(
        "Analyzing file: {} in project: {} ({:?}) at root: {}",
        input.display(),
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

    tracing::info!("Starting LSP server: {} {:?}", lsp_config.command, lsp_config.args);

    // Start LSP client
    let mut client = LspClient::new_with_paths(
        &lsp_config.command,
        &lsp_config.args,
        &root_path,
        project_type,
        &config.bin_paths,
    )?;

    client.initialize()?;

    // Wait for LSP server to complete indexing
    let timeout_secs = config.lsp_readiness_timeout_secs.unwrap_or(args.lsp_timeout);
    client.wait_for_indexing(timeout_secs)?;

    // Read file content
    let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

    tracing::info!("Opening document...");
    client.did_open(&input_path, &content)?;

    // Get file URI
    let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
        copier::error::CopierError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid file path for URI conversion",
        ))
    })?;

    // Extract symbols
    let symbols = extract_symbols(&mut client, &file_uri)?;
    tracing::info!("Extracted {} symbols", symbols.len());

    // Build symbol index
    let file_symbols = vec![(input_path.clone(), symbols.clone())];
    let symbol_index = SymbolIndex::build_from_symbols(&file_symbols);
    tracing::info!("Built symbol index with {} types", symbol_index.len());

    // Extract type references from symbols
    let type_extractor = TypeExtractor::new(project_type);
    let mut all_type_refs = Vec::new();
    for symbol in &symbols {
        all_type_refs.extend(type_extractor.extract_types(symbol, &file_uri));
    }
    tracing::info!("Extracted {} type references", all_type_refs.len());

    // Resolve type references
    let type_resolver = TypeResolver::new(&symbol_index, true); // use_lsp = true
    let resolved_types = type_resolver.resolve_types(&all_type_refs, Some(&mut client));
    tracing::info!("Resolved {} types", resolved_types.len());

    // Shutdown LSP
    client.shutdown()?;

    // Calculate relative path
    let relative_path = input_path
        .strip_prefix(&root_path)
        .unwrap_or(&input_path)
        .display()
        .to_string();

    // Build project type dependencies structure
    let file_type_deps = copier::analyze::FileTypeDependencies {
        file_path: relative_path,
        types: resolved_types,
    };

    let project_type_deps = copier::analyze::ProjectTypeDependencies {
        project_name,
        project_type,
        files: vec![file_type_deps],
    };

    // Format output
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_type_dependencies(&[project_type_deps]);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Type dependencies written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    Ok(())
}

fn run_type_deps_multiple_files(args: &Args) -> Result<()> {
    use std::collections::HashMap;

    // Load configuration
    let config = load_analyze_config(args.config.as_deref())?;

    tracing::info!("Analyzing {} files for type dependencies", args.inputs.len());

    // Group files by (root_path, project_type)
    let mut file_groups: HashMap<(PathBuf, copier::analyze::ProjectType), Vec<PathBuf>> =
        HashMap::new();

    for input in &args.inputs {
        let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

        let (root_path, project_type) = if let Some(ref override_root) = args.project_root {
            let project_type = copier::analyze::ProjectType::Rust; // Default to Rust when overridden
            (override_root.clone(), project_type)
        } else {
            detect_project_root(&input_path)?
        };

        file_groups
            .entry((root_path, project_type))
            .or_insert_with(Vec::new)
            .push(input_path);
    }

    tracing::info!("Found {} project(s)", file_groups.len());

    let mut projects = Vec::new();

    // Process each project group
    for ((root_path, project_type), files) in file_groups {
        let project_name = extract_project_name(&root_path, project_type);

        tracing::info!(
            "Processing {} files in project: {} ({:?})",
            files.len(),
            project_name,
            project_type
        );

        // Get LSP server configuration
        let lsp_config = if let Some(ref cmd) = args.lsp_server {
            copier::analyze::LspServerConfig::from_command_string(cmd)
        } else {
            get_lsp_server_with_config(project_type, Some(&config.lsp_servers))
        };

        // Start LSP client for this project
        let mut client = LspClient::new_with_paths(
            &lsp_config.command,
            &lsp_config.args,
            &root_path,
            project_type,
            &config.bin_paths,
        )?;

        client.initialize()?;

        // Wait for LSP server to complete indexing
        let timeout_secs = config.lsp_readiness_timeout_secs.unwrap_or(args.lsp_timeout);
        client.wait_for_indexing(timeout_secs)?;

        // Collect symbols from all files in this project
        let mut all_file_symbols = Vec::new();

        for input in &files {
            let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;
            let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

            client.did_open(&input_path, &content)?;

            let file_uri = lsp_types::Url::from_file_path(&input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path for URI conversion",
                ))
            })?;

            let symbols = extract_symbols(&mut client, &file_uri)?;
            tracing::info!("Extracted {} symbols from {}", symbols.len(), input.display());

            all_file_symbols.push((input_path, symbols));
        }

        // Build symbol index from all files
        let symbol_index = SymbolIndex::build_from_symbols(&all_file_symbols);
        tracing::info!("Built symbol index with {} types", symbol_index.len());

        // Extract and resolve type references for each file
        let type_extractor = TypeExtractor::new(project_type);
        let type_resolver = TypeResolver::new(&symbol_index, true); // use_lsp = true

        let mut file_type_dependencies = Vec::new();

        for (input_path, symbols) in &all_file_symbols {
            // Convert path to URI
            let file_uri = lsp_types::Url::from_file_path(input_path).map_err(|_| {
                copier::error::CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path for URI conversion",
                ))
            })?;

            // Extract type references from this file's symbols
            let mut type_refs = Vec::new();
            for symbol in symbols {
                type_refs.extend(type_extractor.extract_types(symbol, &file_uri));
            }

            // Resolve type references
            let resolved_types = type_resolver.resolve_types(&type_refs, Some(&mut client));

            tracing::info!(
                "Extracted and resolved {} types from {}",
                resolved_types.len(),
                input_path.display()
            );

            // Calculate relative path
            let relative_path = input_path
                .strip_prefix(&root_path)
                .unwrap_or(input_path)
                .display()
                .to_string();

            file_type_dependencies.push(copier::analyze::FileTypeDependencies {
                file_path: relative_path,
                types: resolved_types,
            });
        }

        // Shutdown LSP for this group
        client.shutdown()?;
        tracing::info!("Completed type dependency analysis for project: {}", project_name);

        projects.push(copier::analyze::ProjectTypeDependencies {
            project_name,
            project_type,
            files: file_type_dependencies,
        });
    }

    // Format output
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_type_dependencies(&projects);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Type dependencies written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    tracing::info!("Successfully analyzed type dependencies for {} files", args.inputs.len());

    Ok(())
}
