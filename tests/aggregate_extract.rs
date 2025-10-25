use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;

use quickctx::config::{
    AppContext, ConflictStrategy, CopyConfig, FencePreference, InputSource, OutputFormat,
    PasteConfig,
};
use quickctx::copy;
use quickctx::paste;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let mut base = env::temp_dir();
        let pid = std::process::id();
        for attempt in 0..1000 {
            base.push(format!("quickctx-test-{}-{}", pid, attempt));
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

fn utf8<P: AsRef<Path>>(path: P) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(path.as_ref().to_path_buf()).expect("utf8 path")
}

#[test]
fn aggregate_single_file_to_markdown_file() {
    let temp = TempDir::new();
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("aggregate.md"));
    let config = CopyConfig {
        inputs: vec!["src/main.rs".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();

    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();
    let expected = "src/main.rs\n\n```rust\nfn main() { println!(\"hi\"); }\n```\n\n";
    assert_eq!(markdown, expected);
}

#[test]
fn aggregate_respects_gitignore_patterns() {
    let temp = TempDir::new();
    fs::write(temp.path().join(".gitignore"), "*.log\n").unwrap();
    fs::write(temp.path().join("ignored.log"), "skip\n").unwrap();
    fs::write(temp.path().join("keep.txt"), "stay\n").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("doc.md"));
    let config = CopyConfig {
        inputs: vec![".".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();
    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();

    assert!(!markdown.contains("ignored.log"));
    assert!(markdown.contains("keep.txt"));
}

#[test]
fn extract_round_trip_recreates_files() {
    let temp = TempDir::new();
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let aggregate_output = utf8(temp.path().join("roundtrip.md"));
    let aggregate_config = CopyConfig {
        inputs: vec!["src/lib.rs".to_string()],
        output: Some(aggregate_output.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };
    copy::run(&context, aggregate_config).unwrap();

    let extract_config = PasteConfig {
        source: InputSource::File(aggregate_output.clone()),
        output_dir: utf8(temp.path().join("restored")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    let restored = temp.path().join("restored/src/lib.rs");
    let contents = fs::read_to_string(restored).unwrap();
    assert!(contents.contains("pub fn add"));
}

/// Regression test: Headings without backticks should NOT be used as file paths
#[test]
fn extract_ignores_headings_without_backticks() {
    let temp = TempDir::new();

    // Create markdown with heading without backticks
    let markdown = r#"# This is a regular heading

```rust
// src/main.rs
fn main() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // Should extract to src/main.rs (from comment hint), NOT "This is a regular heading"
    let extracted = temp.path().join("output/src/main.rs");
    assert!(
        extracted.exists(),
        "File should be extracted to src/main.rs"
    );

    let bad_path = temp.path().join("output/This is a regular heading");
    assert!(
        !bad_path.exists(),
        "Should not create file from heading without backticks"
    );
}

/// Regression test: Headings with backticks SHOULD be used as file paths
#[test]
fn extract_uses_headings_with_backticks() {
    let temp = TempDir::new();

    // Create markdown with heading in backticks
    let markdown = r#"# `src/lib.rs`

```rust
pub fn hello() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // Should extract to src/lib.rs (from heading in backticks)
    let extracted = temp.path().join("output/src/lib.rs");
    assert!(
        extracted.exists(),
        "File should be extracted to src/lib.rs from backtick heading"
    );

    let contents = fs::read_to_string(extracted).unwrap();
    assert!(contents.contains("pub fn hello"));
}

/// Regression test: Comment hints should take priority over heading/trailing text hints
#[test]
fn extract_comment_hint_takes_priority() {
    let temp = TempDir::new();

    // Create markdown with both heading hint and comment hint
    let markdown = r#"# `wrong/path.rs`

```rust
// correct/path.rs
fn main() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // Should use comment hint (correct/path.rs), NOT heading hint (wrong/path.rs)
    let correct = temp.path().join("output/correct/path.rs");
    assert!(
        correct.exists(),
        "File should be extracted using comment hint"
    );

    let wrong = temp.path().join("output/wrong/path.rs");
    assert!(
        !wrong.exists(),
        "Should not use heading hint when comment hint exists"
    );

    let contents = fs::read_to_string(correct).unwrap();
    assert!(contents.contains("fn main"));
}

/// Regression test: Comment hints work with various comment styles
#[test]
fn extract_supports_multiple_comment_styles() {
    let temp = TempDir::new();

    // Test different comment styles
    let test_cases = [
        ("// file.rs\nfn main() {}", "file.rs", "rust"),
        ("# file.py\ndef hello():\n    pass", "file.py", "python"),
        ("; file.lisp\n(defun hello ())", "file.lisp", "lisp"),
        ("-- file.sql\nSELECT * FROM users;", "file.sql", "sql"),
    ];

    for (i, (code, expected_file, lang)) in test_cases.iter().enumerate() {
        let markdown = format!("```{}\n{}\n```", lang, code);
        let md_path = temp.path().join(format!("input{}.md", i));
        fs::write(&md_path, &markdown).unwrap();

        let context = AppContext {
            cwd: utf8(temp.path()),
            verbosity: 0,
        };

        let output_dir = temp.path().join(format!("output{}", i));
        let extract_config = PasteConfig {
            source: InputSource::File(utf8(&md_path)),
            output_dir: utf8(&output_dir),
            conflict: ConflictStrategy::Overwrite,
        };

        paste::run(&context, extract_config).unwrap();

        let extracted = output_dir.join(expected_file);
        assert!(
            extracted.exists(),
            "File {} should be extracted for comment style {}",
            expected_file,
            lang
        );
    }
}

/// Regression test: Error messages should include the file path
#[test]
fn extract_error_includes_file_path() {
    let temp = TempDir::new();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let nonexistent = utf8(temp.path().join("nonexistent.md"));
    let extract_config = PasteConfig {
        source: InputSource::File(nonexistent.clone()),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    let result = paste::run(&context, extract_config);

    assert!(result.is_err(), "Should fail when file doesn't exist");

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("nonexistent.md"),
        "Error message should contain the file path, got: {}",
        err_msg
    );
}

/// Edge case: Multiple headings before code block - only last one should apply
#[test]
fn extract_multiple_headings_uses_last_one() {
    let temp = TempDir::new();

    let markdown = r#"# `wrong/first.rs`

## `wrong/second.rs`

### `correct/third.rs`

```rust
fn main() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // Should use the last heading
    let correct = temp.path().join("output/correct/third.rs");
    assert!(
        correct.exists(),
        "File should be extracted using last heading"
    );

    let wrong1 = temp.path().join("output/wrong/first.rs");
    let wrong2 = temp.path().join("output/wrong/second.rs");
    assert!(!wrong1.exists(), "Should not use first heading");
    assert!(!wrong2.exists(), "Should not use second heading");
}

/// Edge case: Heading after code block should not apply retroactively
#[test]
fn extract_heading_after_codeblock_ignored() {
    let temp = TempDir::new();

    let markdown = r#"```rust
// correct.rs
fn main() {}
```

## `should-be-ignored.rs`
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    let correct = temp.path().join("output/correct.rs");
    assert!(
        correct.exists(),
        "File should be extracted using comment hint"
    );

    let wrong = temp.path().join("output/should-be-ignored.rs");
    assert!(
        !wrong.exists(),
        "Heading after code block should not be used"
    );
}

/// Edge case: Mixed content with multiple code blocks in different formats
#[test]
fn extract_mixed_format_code_blocks() {
    let temp = TempDir::new();

    let markdown = r#"# Project Files

Here's the main file:

src/main.rs

```rust
fn main() {
    println!("Hello!");
}
```

And here's a helper:

## `src/lib.rs`

```rust
pub fn helper() {}
```

Finally, a test:

```rust
// tests/test.rs
#[test]
fn test_it() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // All three files should be extracted
    let main_file = temp.path().join("output/src/main.rs");
    let lib_file = temp.path().join("output/src/lib.rs");
    let test_file = temp.path().join("output/tests/test.rs");

    assert!(
        main_file.exists(),
        "main.rs should be extracted (simple format)"
    );
    assert!(
        lib_file.exists(),
        "lib.rs should be extracted (heading format)"
    );
    assert!(
        test_file.exists(),
        "test.rs should be extracted (comment format)"
    );

    let main_contents = fs::read_to_string(main_file).unwrap();
    assert!(main_contents.contains("Hello!"));
}

/// Edge case: Simple format with multiple lines of trailing text
#[test]
fn extract_simple_format_multiline_trailing_text() {
    let temp = TempDir::new();

    let markdown = r#"Here is some prose.

And more prose.

src/file.rs

```rust
fn code() {}
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    let extracted = temp.path().join("output/src/file.rs");
    assert!(
        extracted.exists(),
        "File should be extracted using last non-empty line before code block"
    );
}

/// Test aggregate with glob patterns
#[test]
fn aggregate_with_glob_patterns() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::create_dir_all(temp.path().join("tests")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(temp.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(temp.path().join("tests/test.rs"), "#[test] fn t() {}").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("output.md"));
    let config = CopyConfig {
        inputs: vec!["src/*.rs".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();
    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();

    // Should match both files in src/
    assert!(markdown.contains("src/lib.rs") || markdown.contains("src\\lib.rs"));
    assert!(markdown.contains("src/main.rs") || markdown.contains("src\\main.rs"));
    // Should not match tests/
    assert!(!markdown.contains("tests/test.rs"));
}

/// Test aggregate with exclude patterns
#[test]
fn aggregate_with_exclude_patterns() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(temp.path().join("src/test.rs"), "test").unwrap();
    fs::write(temp.path().join("src/lib.rs"), "lib").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("output.md"));
    let config = CopyConfig {
        inputs: vec!["src/".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: vec!["**/test.rs".to_string()],
    };

    copy::run(&context, config).unwrap();
    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();

    assert!(markdown.contains("main.rs"));
    assert!(markdown.contains("lib.rs"));
    // test.rs should be excluded
    assert!(!markdown.contains("test.rs"));
}

/// Test aggregate skips binary files
#[test]
fn aggregate_skips_binary_files() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
    // Write a binary file (with null bytes)
    fs::write(
        temp.path().join("src/binary.bin"),
        b"\x00\x01\x02\x03\x00\xFF",
    )
    .unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("output.md"));
    let config = CopyConfig {
        inputs: vec!["src/".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();
    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();

    assert!(markdown.contains("main.rs"));
    // Binary file should be skipped
    assert!(!markdown.contains("binary.bin"));
}

/// Test aggregate with no gitignore
#[test]
fn aggregate_without_gitignore() {
    let temp = TempDir::new();
    fs::write(temp.path().join(".gitignore"), "ignored.txt").unwrap();
    fs::write(temp.path().join("ignored.txt"), "ignored content").unwrap();
    fs::write(temp.path().join("included.txt"), "included content").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("output.md"));
    let config = CopyConfig {
        inputs: vec![".".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Simple,
        fence: FencePreference::Auto,
        respect_gitignore: false, // Disable gitignore
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();
    let markdown = fs::read_to_string(output_path.as_std_path()).unwrap();

    // Both files should be included when gitignore is disabled
    assert!(markdown.contains("ignored.txt"));
    assert!(markdown.contains("included.txt"));
}

/// Test extract error: empty path
#[test]
fn extract_error_empty_path() {
    let temp = TempDir::new();

    let markdown = r#"
```rust

```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    let result = paste::run(&context, extract_config);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unable to determine file path")
    );
}

/// Test extract error: absolute path
#[test]
fn extract_error_absolute_path() {
    let temp = TempDir::new();

    let markdown = r#"/etc/passwd

```text
should not work
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    let result = paste::run(&context, extract_config);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("absolute paths are not allowed")
    );
}

/// Test extract error: parent directory traversal
#[test]
fn extract_error_parent_directory() {
    let temp = TempDir::new();

    let markdown = r#"../../../etc/passwd

```text
should not work
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    let result = paste::run(&context, extract_config);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("parent directory segments are not allowed")
    );
}

/// Test extract with skip strategy
#[test]
fn extract_conflict_skip_strategy() {
    let temp = TempDir::new();

    // Create existing file
    fs::create_dir_all(temp.path().join("output/src")).unwrap();
    fs::write(temp.path().join("output/src/main.rs"), "original content").unwrap();

    let markdown = r#"src/main.rs

```rust
new content
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Skip,
    };

    paste::run(&context, extract_config).unwrap();

    // File should still have original content
    let content = fs::read_to_string(temp.path().join("output/src/main.rs")).unwrap();
    assert_eq!(content, "original content");
}

/// Test extract with overwrite strategy
#[test]
fn extract_conflict_overwrite_strategy() {
    let temp = TempDir::new();

    // Create existing file
    fs::create_dir_all(temp.path().join("output/src")).unwrap();
    fs::write(temp.path().join("output/src/main.rs"), "original content").unwrap();

    let markdown = r#"src/main.rs

```rust
new content
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    // File should have new content
    let content = fs::read_to_string(temp.path().join("output/src/main.rs")).unwrap();
    assert_eq!(content, "new content\n");
}

/// Test extract single-line comment hint
#[test]
fn extract_single_line_comment_hint() {
    let temp = TempDir::new();

    let markdown = r#"
```rust
// src/single.rs
```
"#;

    let md_path = temp.path().join("input.md");
    fs::write(&md_path, markdown).unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let extract_config = PasteConfig {
        source: InputSource::File(utf8(&md_path)),
        output_dir: utf8(temp.path().join("output")),
        conflict: ConflictStrategy::Overwrite,
    };

    paste::run(&context, extract_config).unwrap();

    let extracted = temp.path().join("output/src/single.rs");
    assert!(extracted.exists());

    // Content should be empty after comment is removed
    let content = fs::read_to_string(extracted).unwrap();
    assert_eq!(content.trim(), "");
}

// ============================================================================
// Heredoc Format Tests
// ============================================================================

/// Test heredoc format generates valid bash syntax
#[test]
fn heredoc_format_generates_valid_bash() {
    let temp = TempDir::new();
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("script.sh"));
    let config = CopyConfig {
        inputs: vec!["src/main.rs".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Heredoc,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();

    let script = fs::read_to_string(output_path.as_std_path()).unwrap();

    // Verify basic structure
    assert!(script.contains("mkdir -p 'src'"));
    assert!(script.contains("cat > 'src/main.rs'"));
    assert!(script.contains("EOF"));
    assert!(script.contains("fn main() {}"));
}

/// Test heredoc format with path normalization
#[test]
fn heredoc_format_normalizes_paths() {
    let temp = TempDir::new();

    // Create a file outside the temp dir to test absolute path handling
    let external_file = env::temp_dir().join("quickctx-test-external.txt");
    fs::write(&external_file, "external content\n").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("script.sh"));
    let config = CopyConfig {
        inputs: vec![external_file.to_string_lossy().to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Heredoc,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();

    let script = fs::read_to_string(output_path.as_std_path()).unwrap();

    // Should use just filename, not absolute path
    assert!(!script.contains("cat > '/"));
    assert!(script.contains("cat > 'quickctx-test-external.txt'"));

    // Cleanup
    let _ = fs::remove_file(external_file);
}

/// Test heredoc delimiter selection avoids conflicts
#[test]
fn heredoc_format_avoids_delimiter_conflicts() {
    let temp = TempDir::new();
    let file_path = temp.path().join("file.txt");

    // Create content that contains EOF
    fs::write(&file_path, "This has EOF\nand more EOF\nEOF\n").unwrap();

    let context = AppContext {
        cwd: utf8(temp.path()),
        verbosity: 0,
    };

    let output_path = utf8(temp.path().join("script.sh"));
    let config = CopyConfig {
        inputs: vec!["file.txt".to_string()],
        output: Some(output_path.clone()),
        format: OutputFormat::Heredoc,
        fence: FencePreference::Auto,
        respect_gitignore: true,
        ignore_files: Vec::new(),
        excludes: Vec::new(),
    };

    copy::run(&context, config).unwrap();

    let script = fs::read_to_string(output_path.as_std_path()).unwrap();

    // Should use a different delimiter than EOF
    assert!(script.contains("cat > 'file.txt' << 'END'") ||
            script.contains("cat > 'file.txt' << 'HEREDOC'") ||
            script.contains("cat > 'file.txt' << 'CONTENT'"));
}
