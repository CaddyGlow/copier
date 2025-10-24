use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quickctx::cli::{AggregateArgs, Cli, Commands, ExtractArgs};
use quickctx::config::{
    self, AggregateConfig, ConflictStrategy, FencePreference, ModeConfig, OutputFormat,
};

// Mutex to serialize tests that change current directory
static CWD_LOCK: Mutex<()> = Mutex::new(());

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let mut base = env::temp_dir();
        let pid = std::process::id();
        for attempt in 0..1000 {
            base.push(format!("quickctx-config-test-{}-{}", pid, attempt));
            if fs::create_dir(&base).is_ok() {
                return Self { path: base };
            }
            base.pop();
        }
        panic!("failed to create temp dir");
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_output_format_default() {
    assert_eq!(OutputFormat::default(), OutputFormat::Simple);
}

#[test]
fn test_fence_preference_default() {
    assert_eq!(FencePreference::default(), FencePreference::Auto);
}

#[test]
fn test_conflict_strategy_default() {
    assert_eq!(ConflictStrategy::default(), ConflictStrategy::Prompt);
}

#[test]
fn test_aggregate_config_require_inputs_empty() {
    let config = AggregateConfig {
        inputs: vec![],
        output: None,
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    let result = config.require_inputs();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no input paths"));
}

#[test]
fn test_aggregate_config_require_inputs_with_paths() {
    let config = AggregateConfig {
        inputs: vec!["src/".to_string()],
        output: None,
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    assert!(config.require_inputs().is_ok());
}

#[test]
fn test_load_config_default_aggregate_mode() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    assert_eq!(runtime_config.context.verbosity, 0);

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            assert_eq!(cfg.inputs.len(), 1);
            assert_eq!(cfg.inputs[0], "src/");
            assert_eq!(cfg.format, OutputFormat::Simple);
            assert_eq!(cfg.fence, FencePreference::Auto);
            assert!(cfg.respect_gitignore);
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_explicit_aggregate_mode() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let cli = Cli {
        config: None,
        verbose: 1,
        aggregate: AggregateArgs::default(),
        command: Some(Commands::Aggregate(AggregateArgs {
            paths: vec![PathBuf::from("lib/")],
            output: Some(PathBuf::from("out.md")),
            format: Some(OutputFormat::Comment),
            fence: Some(FencePreference::Tilde),
            no_gitignore: true,
            ignore_file: vec![],
            exclude: vec!["*.log".to_string()],
        })),
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    assert_eq!(runtime_config.context.verbosity, 1);

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            assert_eq!(cfg.inputs[0], "lib/");
            assert!(cfg.output.is_some());
            assert_eq!(cfg.format, OutputFormat::Comment);
            assert_eq!(cfg.fence, FencePreference::Tilde);
            assert!(!cfg.respect_gitignore);
            assert_eq!(cfg.excludes.len(), 1);
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_extract_mode_from_file() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    // Create a markdown file
    let input_path = temp.path().join("input.md");
    fs::write(&input_path, "# test\n```rust\nfn main() {}\n```").unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs::default(),
        command: Some(Commands::Extract(ExtractArgs {
            input: Some(input_path.clone()),
            output_dir: Some(PathBuf::from("extracted/")),
            conflict: Some(ConflictStrategy::Overwrite),
        })),
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();

    match runtime_config.mode {
        ModeConfig::Extract(cfg) => {
            assert_eq!(cfg.conflict, ConflictStrategy::Overwrite);
            assert_eq!(cfg.output_dir.as_str(), "extracted/");
        }
        _ => panic!("Expected Extract mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_extract_mode_from_stdin() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let cli = Cli {
        config: None,
        verbose: 2,
        aggregate: AggregateArgs::default(),
        command: Some(Commands::Extract(ExtractArgs {
            input: None,
            output_dir: None,
            conflict: Some(ConflictStrategy::Skip),
        })),
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    assert_eq!(runtime_config.context.verbosity, 2);

    match runtime_config.mode {
        ModeConfig::Extract(cfg) => {
            assert_eq!(cfg.conflict, ConflictStrategy::Skip);
            // Should use current directory as output_dir
        }
        _ => panic!("Expected Extract mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_with_toml_file() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    // Create a quickctx.toml config file
    let config_content = r#"
[general]
verbose = 1

[aggregate]
paths = ["src/", "tests/"]
format = "heading"
fence = "backtick"
respect_gitignore = false
exclude = ["*.tmp"]

[extractor]
conflict = "overwrite"
"#;

    fs::write(temp.path().join("quickctx.toml"), config_content).unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs::default(),
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    // File config verbose (1) should be used
    assert_eq!(runtime_config.context.verbosity, 1);

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            assert_eq!(cfg.inputs.len(), 2);
            assert!(cfg.inputs.contains(&"src/".to_string()));
            assert!(cfg.inputs.contains(&"tests/".to_string()));
            assert_eq!(cfg.format, OutputFormat::Heading);
            assert_eq!(cfg.fence, FencePreference::Backtick);
            assert!(!cfg.respect_gitignore);
            assert!(cfg.excludes.contains(&"*.tmp".to_string()));
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_cli_overrides_file() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    // Create a quickctx.toml config file
    let config_content = r#"
[aggregate]
paths = ["from-file/"]
format = "simple"
"#;

    fs::write(temp.path().join("quickctx.toml"), config_content).unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("from-cli/")],
            format: Some(OutputFormat::Comment),
            output: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            // CLI paths should be appended to file paths
            assert!(cfg.inputs.contains(&"from-cli/".to_string()));
            // CLI format should override file format
            assert_eq!(cfg.format, OutputFormat::Comment);
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_custom_config_path() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let config_content = r#"
[general]
verbose = 3
"#;

    let custom_config_path = temp.path().join("custom.toml");
    fs::write(&custom_config_path, config_content).unwrap();

    let cli = Cli {
        config: Some(custom_config_path),
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    assert_eq!(runtime_config.context.verbosity, 3);

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_load_config_invalid_toml() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let invalid_config = "this is not valid TOML ][[[";
    let config_path = temp.path().join("quickctx.toml");
    fs::write(&config_path, invalid_config).unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_err());

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_aggregate_config_with_multiple_ignore_files() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    // Create ignore files
    let ignore1 = temp.path().join(".ignore1");
    let ignore2 = temp.path().join(".ignore2");
    fs::write(&ignore1, "*.log").unwrap();
    fs::write(&ignore2, "*.tmp").unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![ignore1.clone(), ignore2.clone()],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            assert_eq!(cfg.ignore_files.len(), 2);
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_aggregate_config_no_gitignore_flag() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let cli = Cli {
        config: None,
        verbose: 0,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: true,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();

    match runtime_config.mode {
        ModeConfig::Aggregate(cfg) => {
            assert!(!cfg.respect_gitignore);
        }
        _ => panic!("Expected Aggregate mode"),
    }

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_verbosity_cli_plus_file() {
    let _lock = CWD_LOCK.lock().unwrap();
    let temp = TempDir::new();
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp.path()).unwrap();

    let config_content = r#"
[general]
verbose = 2
"#;

    fs::write(temp.path().join("quickctx.toml"), config_content).unwrap();

    let cli = Cli {
        config: None,
        verbose: 1,
        aggregate: AggregateArgs {
            paths: vec![PathBuf::from("src/")],
            output: None,
            format: None,
            fence: None,
            no_gitignore: false,
            ignore_file: vec![],
            exclude: vec![],
        },
        command: None,
    };

    let result = config::load(&cli);
    assert!(result.is_ok());

    let runtime_config = result.unwrap();
    // Verbosity should be additive: CLI (1) + file (2) = 3
    assert_eq!(runtime_config.context.verbosity, 3);

    env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_output_format_display() {
    assert_eq!(format!("{}", OutputFormat::Simple), "simple");
    assert_eq!(format!("{}", OutputFormat::Comment), "comment");
    assert_eq!(format!("{}", OutputFormat::Heading), "heading");
}

#[test]
fn test_fence_preference_display() {
    assert_eq!(format!("{}", FencePreference::Auto), "auto");
    assert_eq!(format!("{}", FencePreference::Backtick), "backtick");
    assert_eq!(format!("{}", FencePreference::Tilde), "tilde");
}

#[test]
fn test_conflict_strategy_display() {
    assert_eq!(format!("{}", ConflictStrategy::Prompt), "prompt");
    assert_eq!(format!("{}", ConflictStrategy::Skip), "skip");
    assert_eq!(format!("{}", ConflictStrategy::Overwrite), "overwrite");
}
