use clap::Parser;
use copier::analyze::{
    detect_project_root, extract_symbols, get_formatter, get_lsp_server, LspClient, OutputFormat,
};
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

    tracing::info!("Project root: {}", root_path.display());
    tracing::info!("Project type: {:?}", project_type);

    // Get LSP server configuration
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::new(cmd.clone(), vec![])
    } else {
        get_lsp_server(project_type)
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Canonicalize input file path
    let input_path = input.canonicalize().map_err(copier::error::CopierError::Io)?;

    // Read file content
    let content = fs::read_to_string(&input_path).map_err(copier::error::CopierError::Io)?;

    // Create LSP client
    let mut client = LspClient::new(
        &lsp_config.command,
        &lsp_config.args,
        &root_path,
        project_type,
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
    // Detect project root from first file or use override
    let first_input = &args.inputs[0];
    let (root_path, project_type) = if let Some(ref root) = args.project_root {
        let (detected_root, detected_type) = detect_project_root(root)?;
        if detected_root == *root {
            (root.clone(), detected_type)
        } else {
            (root.clone(), copier::analyze::ProjectType::Unknown)
        }
    } else {
        detect_project_root(first_input)?
    };

    tracing::info!("Project root: {}", root_path.display());
    tracing::info!("Project type: {:?}", project_type);
    tracing::info!("Analyzing {} files", args.inputs.len());

    // Get LSP server configuration
    let lsp_config = if let Some(ref cmd) = args.lsp_server {
        copier::analyze::LspServerConfig::new(cmd.clone(), vec![])
    } else {
        get_lsp_server(project_type)
    };

    tracing::info!("Using LSP server: {}", lsp_config.command);

    // Create LSP client once for all files
    let mut client = LspClient::new(
        &lsp_config.command,
        &lsp_config.args,
        &root_path,
        project_type,
    )?;

    // Initialize LSP
    tracing::info!("Initializing LSP...");
    client.initialize()?;

    // Process each file
    let mut all_files = Vec::new();

    for input in &args.inputs {
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

        all_files.push((relative_path, symbols));
    }

    // Format output for all files
    let formatter = get_formatter(args.format.into());
    let output_text = formatter.format_multiple(&all_files);

    // Write output
    if let Some(ref output_path) = args.output {
        fs::write(output_path, output_text).map_err(copier::error::CopierError::Io)?;
        println!("Analysis written to: {}", output_path.display());
    } else {
        print!("{}", output_text);
    }

    // Shutdown LSP
    client.shutdown()?;

    tracing::info!("Successfully analyzed {} files", args.inputs.len());

    Ok(())
}
