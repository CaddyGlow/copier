use clap::Parser;
use copier::analyze::{
    detect_project_root, extract_project_name, extract_symbols, get_formatter,
    get_lsp_server_with_config, LspClient, OutputFormat,
};
use copier::config::load_analyze_config;
use copier::error::Result;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
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
    // Validate all input files exist
    for input in &args.inputs {
        if !input.exists() {
            return Err(copier::error::CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Input file not found: {}", input.display()),
            )));
        }
    }

    // If single file, use simple mode; if multiple, use batch mode
    if args.inputs.len() == 1 {
        run_single_file(&args)
    } else {
        run_multiple_files(&args)
    }
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
