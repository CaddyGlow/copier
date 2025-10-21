use std::env;
use std::fs;
use std::path::PathBuf;

use camino::Utf8PathBuf;

use copier::config::{
    AggregateConfig, AppContext, ConflictStrategy, ExtractConfig, FencePreference, InputSource,
    OutputFormat,
};
use copier::{aggregate, extract};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let mut base = env::temp_dir();
        let pid = std::process::id();
        for attempt in 0..1000 {
            base.push(format!("copier-integration-test-{}-{}", pid, attempt));
            if fs::create_dir(&base).is_ok() {
                return Self { path: base };
            }
            base.pop();
        }
        panic!("failed to create temp dir");
    }

    fn path(&self) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(self.path.clone()).expect("utf8 path")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Full pipeline integration test: Multi-file project with different formats
#[test]
fn integration_multifile_project_all_formats() {
    let temp = TempDir::new();

    // Create a small project structure
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("tests")).unwrap();

    fs::write(
        temp.path().join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
    }
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("tests/integration.rs"),
        r#"#[test]
fn integration_test() {
    assert!(true);
}
"#,
    )
    .unwrap();

    fs::write(
        temp.path().join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
"#,
    )
    .unwrap();

    let context = AppContext {
        cwd: temp.path(),
        verbosity: 0,
    };

    // Test each format
    for (format, format_name) in [
        (OutputFormat::Simple, "simple"),
        (OutputFormat::Comment, "comment"),
        (OutputFormat::Heading, "heading"),
    ] {
        let aggregate_output = temp.path().join(format!("{}_aggregate.md", format_name));

        // Aggregate
        let aggregate_config = AggregateConfig {
            inputs: vec![
                "src/".to_string(),
                "tests/".to_string(),
                "Cargo.toml".to_string(),
            ],
            output: Some(aggregate_output.clone()),
            format,
            fence: FencePreference::Auto,
            respect_gitignore: true,
            ignore_files: Vec::new(),
            excludes: Vec::new(),
        };

        aggregate::run(&context, aggregate_config).unwrap();
        assert!(aggregate_output.exists());

        // Extract to different directory
        let extract_output = temp.path().join(format!("{}_extracted", format_name));
        let extract_config = ExtractConfig {
            source: InputSource::File(aggregate_output.clone()),
            output_dir: extract_output.clone(),
            conflict: ConflictStrategy::Overwrite,
        };

        extract::run(&context, extract_config).unwrap();

        // Verify all files were extracted correctly
        assert!(extract_output.join("src/main.rs").exists());
        assert!(extract_output.join("src/lib.rs").exists());
        assert!(extract_output.join("tests/integration.rs").exists());
        assert!(extract_output.join("Cargo.toml").exists());

        // Verify content preservation
        let original_main =
            fs::read_to_string(temp.path().join("src/main.rs").as_std_path()).unwrap();
        let extracted_main =
            fs::read_to_string(extract_output.join("src/main.rs").as_std_path()).unwrap();
        assert!(extracted_main.contains("Hello, world!"));
        assert_eq!(original_main.trim(), extracted_main.trim());
    }
}

/// Integration test: Nested directory structure preservation
#[test]
fn integration_nested_directory_structure() {
    let temp = TempDir::new();

    // Create deeply nested structure
    let paths = ["a/b/c/deep.txt", "a/b/shallow.txt", "a/top.txt", "root.txt"];

    for path in &paths {
        let full_path = temp.path().join(path);
        fs::create_dir_all(full_path.parent().unwrap().as_std_path()).unwrap();
        fs::write(full_path.as_std_path(), format!("Content of {}", path)).unwrap();
    }

    let context = AppContext {
        cwd: temp.path(),
        verbosity: 0,
    };

    // Aggregate all files
    let aggregate_output = temp.path().join("nested.md");
    let aggregate_config = AggregateConfig {
        inputs: vec![".".to_string()],
        output: Some(aggregate_output.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    aggregate::run(&context, aggregate_config).unwrap();

    // Extract to new location
    let extract_output = temp.path().join("restored");
    let extract_config = ExtractConfig {
        source: InputSource::File(aggregate_output),
        output_dir: extract_output.clone(),
        conflict: ConflictStrategy::Overwrite,
    };

    extract::run(&context, extract_config).unwrap();

    // Verify all nested paths exist
    for path in &paths {
        let extracted = extract_output.join(path);
        assert!(extracted.exists(), "Expected {} to exist", extracted);
        let content = fs::read_to_string(extracted.as_std_path()).unwrap();
        assert_eq!(content.trim(), format!("Content of {}", path).trim());
    }
}

/// Integration test: Mixed content types (code, config, documentation)
#[test]
fn integration_mixed_content_types() {
    let temp = TempDir::new();

    // Create files of different types
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("docs")).unwrap();

    fs::write(
        temp.path().join("src/app.py"),
        "def hello():\n    return 'Hello'\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("docs/README.md"),
        "Project Documentation\n\nThis is a test file.\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("config.json"),
        r#"{
  "setting": "value"
}
"#,
    )
    .unwrap();

    fs::write(temp.path().join("setup.sh"), "#!/bin/bash\necho 'Setup'\n").unwrap();

    let context = AppContext {
        cwd: temp.path(),
        verbosity: 0,
    };

    // Aggregate with heading format
    let aggregate_output = temp.path().join("mixed.md");
    let aggregate_config = AggregateConfig {
        inputs: vec![".".to_string()],
        output: Some(aggregate_output.clone()),
        format: OutputFormat::Heading,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    aggregate::run(&context, aggregate_config).unwrap();

    let markdown = fs::read_to_string(aggregate_output.as_std_path()).unwrap();

    // Debug: Print markdown to see what was generated
    // eprintln!("Generated markdown:\n{}", markdown);

    // Verify language detection worked
    assert!(markdown.contains("```python") || markdown.contains("```py"));
    assert!(markdown.contains("```json"));
    assert!(markdown.contains("```bash") || markdown.contains("```sh"));
    assert!(markdown.contains("```markdown") || markdown.contains("```md"));

    // Verify heading format
    assert!(markdown.contains("## `"));

    // Extract and verify
    let extract_output = temp.path().join("mixed_extracted");
    let extract_config = ExtractConfig {
        source: InputSource::File(temp.path().join("mixed.md")),
        output_dir: extract_output.clone(),
        conflict: ConflictStrategy::Overwrite,
    };

    extract::run(&context, extract_config).unwrap();

    // Check all files exist and have correct content
    assert!(
        extract_output.join("src/app.py").exists(),
        "src/app.py should exist"
    );
    assert!(
        extract_output.join("docs/README.md").exists(),
        "docs/README.md should exist"
    );
    assert!(
        extract_output.join("config.json").exists(),
        "config.json should exist"
    );
    assert!(
        extract_output.join("setup.sh").exists(),
        "setup.sh should exist"
    );

    let py_content = fs::read_to_string(extract_output.join("src/app.py").as_std_path()).unwrap();
    assert!(py_content.contains("def hello()"));

    let json_content =
        fs::read_to_string(extract_output.join("config.json").as_std_path()).unwrap();
    assert!(json_content.contains("setting"));
}

/// Integration test: Glob patterns and excludes
#[test]
fn integration_glob_patterns_with_excludes() {
    let temp = TempDir::new();

    // Create test structure
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("tests")).unwrap();
    fs::create_dir_all(temp.path().join("target")).unwrap();

    fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(temp.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(temp.path().join("tests/test.rs"), "test").unwrap();
    fs::write(temp.path().join("target/debug.txt"), "should be excluded").unwrap();
    fs::write(temp.path().join("src/main_test.rs"), "should be excluded").unwrap();

    let context = AppContext {
        cwd: temp.path(),
        verbosity: 0,
    };

    // Aggregate with glob and excludes
    let aggregate_output = temp.path().join("filtered.md");
    let aggregate_config = AggregateConfig {
        inputs: vec!["**/*.rs".to_string()],
        output: Some(aggregate_output.clone()),
        format: OutputFormat::Comment,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: vec!["**/target/**".to_string(), "**/*_test.rs".to_string()],
    };

    aggregate::run(&context, aggregate_config).unwrap();

    let markdown = fs::read_to_string(aggregate_output.as_std_path()).unwrap();

    // Should include
    assert!(markdown.contains("main.rs") && !markdown.contains("main_test.rs"));
    assert!(markdown.contains("lib.rs"));
    assert!(markdown.contains("test.rs")); // tests/test.rs is OK

    // Should exclude
    assert!(!markdown.contains("target/"));
    assert!(!markdown.contains("main_test.rs"));

    // Extract and verify
    let extract_output = temp.path().join("filtered_extracted");
    let extract_config = ExtractConfig {
        source: InputSource::File(aggregate_output),
        output_dir: extract_output.clone(),
        conflict: ConflictStrategy::Overwrite,
    };

    extract::run(&context, extract_config).unwrap();

    assert!(extract_output.join("src/main.rs").exists());
    assert!(extract_output.join("src/lib.rs").exists());
    assert!(!extract_output.join("src/main_test.rs").exists());
    assert!(!extract_output.join("target/debug.txt").exists());
}
