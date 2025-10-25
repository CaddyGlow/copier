// LSP Integration Tests for Cache Functionality
//
// These tests verify that the analyze command's caching system works correctly.
// They invoke the quickctx-analyze binary which uses rust-analyzer as an LSP server.
//
// Behavior:
// - If rust-analyzer is available (via `rust-analyzer --version`), tests run normally
// - If rust-analyzer is not available (e.g., in CI), tests automatically skip
// - Set QUICKCTX_TEST_LSP=1 to force running tests even if rust-analyzer isn't found
//
// In CI environments without rust-analyzer installed, these tests will gracefully
// skip with a message rather than failing. This allows the test suite to pass
// without requiring LSP server installation in the CI environment.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to get the quickctx-analyze binary path
fn get_analyze_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    // Use the same profile as the test binary (debug or release)
    if cfg!(debug_assertions) {
        path.push("debug");
    } else {
        path.push("release");
    }
    path.push("quickctx-analyze");
    path
}

/// Helper to create a simple Rust test file
fn create_rust_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let file_path = dir.join(name);
    fs::write(&file_path, content).expect("Failed to create test file");
    file_path
}

/// Strip ANSI escape codes from a string
fn strip_ansi_codes(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
    re.replace_all(s, "").to_string()
}

/// Get combined stdout and stderr output
fn get_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    strip_ansi_codes(&format!("{}{}", stdout, stderr))
}

/// Returns true if LSP integration tests should run
fn should_run_lsp_tests() -> bool {
    // Check environment variable
    std::env::var("QUICKCTX_TEST_LSP").is_ok()
        // OR check if rust-analyzer is available
        || Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[test]
fn test_cache_enables_faster_subsequent_runs() {
    if !should_run_lsp_tests() {
        eprintln!(
            "Skipping LSP integration test (rust-analyzer not available or QUICKCTX_TEST_LSP not set)"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = create_rust_file(
        temp_dir.path(),
        "test.rs",
        r#"
        pub struct MyStruct {
            field: i32,
        }

        pub fn my_function() -> i32 {
            42
        }
        "#,
    );

    let binary = get_analyze_binary();

    // First run - should populate cache
    let output1 = Command::new(&binary)
        .arg("-vv")
        .arg("--config")
        .arg("/dev/null") // Avoid loading any config
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output1_text = get_output(&output1);
    assert!(
        output1_text.contains("Cache miss") || output1_text.contains("Extracting symbols"),
        "First run should be a cache miss: {}",
        output1_text
    );
    assert!(
        output1.status.success(),
        "First run should succeed: {}",
        output1_text
    );

    // Second run - should use cache
    let output2 = Command::new(&binary)
        .arg("-vv")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output2_text = get_output(&output2);
    assert!(
        output2_text.contains("Cache hit") || output2_text.contains("Using cached symbols"),
        "Second run should be a cache hit: {}",
        output2_text
    );
    assert!(
        output2.status.success(),
        "Second run should succeed: {}",
        output2_text
    );
}

#[test]
fn test_cache_invalidates_on_file_modification() {
    if !should_run_lsp_tests() {
        eprintln!(
            "Skipping LSP integration test (rust-analyzer not available or QUICKCTX_TEST_LSP not set)"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = create_rust_file(temp_dir.path(), "test.rs", "pub fn foo() {}");

    let binary = get_analyze_binary();

    // First run - populate cache
    let output1 = Command::new(&binary)
        .arg("-v")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    assert!(output1.status.success());

    // Modify the file
    std::thread::sleep(std::time::Duration::from_millis(100));
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&test_file)
        .unwrap();
    writeln!(file, "pub fn bar() {{}}").unwrap();
    drop(file);

    // Second run - cache should be invalid
    let output2 = Command::new(&binary)
        .arg("-vv")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output2_text = get_output(&output2);
    assert!(
        output2_text.contains("Cache invalid") || output2_text.contains("Cache miss"),
        "Modified file should invalidate cache: {}",
        output2_text
    );
    assert!(output2.status.success());
}

#[test]
fn test_no_cache_flag_bypasses_cache() {
    if !should_run_lsp_tests() {
        eprintln!(
            "Skipping LSP integration test (rust-analyzer not available or QUICKCTX_TEST_LSP not set)"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = create_rust_file(temp_dir.path(), "test.rs", "pub fn foo() {}");

    let binary = get_analyze_binary();

    // First run - populate cache
    let output1 = Command::new(&binary)
        .arg("-v")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    assert!(output1.status.success());

    // Second run with --no-cache should not use cache
    let output2 = Command::new(&binary)
        .arg("-vv")
        .arg("--no-cache")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output2_text = get_output(&output2);
    // Should not mention cache at all when disabled
    assert!(
        !output2_text.contains("Cache hit") && !output2_text.contains("Using cached symbols"),
        "--no-cache should bypass cache: {}",
        output2_text
    );
    assert!(output2.status.success());
}

#[test]
fn test_clear_cache_flag() {
    if !should_run_lsp_tests() {
        eprintln!(
            "Skipping LSP integration test (rust-analyzer not available or QUICKCTX_TEST_LSP not set)"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = create_rust_file(temp_dir.path(), "test.rs", "pub fn foo() {}");

    let binary = get_analyze_binary();

    // First run - populate cache
    let output1 = Command::new(&binary)
        .arg("-v")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    assert!(output1.status.success());

    // Check that cache directory exists and has content
    let cache_path = cache_dir.path().join("quickctx").join("analyze");
    assert!(cache_path.exists(), "Cache directory should exist");

    // Run with --clear-cache
    let output2 = Command::new(&binary)
        .arg("-v")
        .arg("--clear-cache")
        .arg("--config")
        .arg("/dev/null")
        .arg(&test_file)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output2_text = get_output(&output2);
    assert!(
        output2_text.contains("Cache cleared"),
        "Should report cache cleared: {}",
        output2_text
    );
    assert!(output2.status.success());

    // Cache directory should still exist but be empty (recreated)
    assert!(cache_path.exists(), "Cache directory should be recreated");
}

#[test]
fn test_cache_works_with_multiple_files() {
    if !should_run_lsp_tests() {
        eprintln!(
            "Skipping LSP integration test (rust-analyzer not available or QUICKCTX_TEST_LSP not set)"
        );
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = TempDir::new().unwrap();

    // Create multiple test files
    let file1 = create_rust_file(temp_dir.path(), "file1.rs", "pub fn foo() {}");
    let file2 = create_rust_file(temp_dir.path(), "file2.rs", "pub fn bar() {}");
    let file3 = create_rust_file(temp_dir.path(), "file3.rs", "pub fn baz() {}");

    let binary = get_analyze_binary();

    // First run - populate cache for all files
    let output1 = Command::new(&binary)
        .arg("-vv")
        .arg("--config")
        .arg("/dev/null")
        .arg(&file1)
        .arg(&file2)
        .arg(&file3)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    assert!(output1.status.success());

    // Modify only the middle file
    std::thread::sleep(std::time::Duration::from_millis(100));
    fs::write(&file2, "pub fn bar() {}\npub fn modified() {}").unwrap();

    // Second run - only file2 should be cache miss
    let output2 = Command::new(&binary)
        .arg("-vv")
        .arg("--config")
        .arg("/dev/null")
        .arg(&file1)
        .arg(&file2)
        .arg(&file3)
        .env("XDG_CACHE_HOME", cache_dir.path().to_str().unwrap())
        .output()
        .expect("Failed to execute quickctx-analyze");

    let output2_text = get_output(&output2);

    // Count cache hits and misses
    let hits = output2_text.matches("Cache hit").count()
        + output2_text.matches("Using cached symbols").count();
    let misses =
        output2_text.matches("Cache miss").count() + output2_text.matches("Cache invalid").count();

    // We expect 2 hits (file1 and file3) and at least 1 miss (file2)
    assert!(
        hits >= 2,
        "Should have at least 2 cache hits for unmodified files, got {}: {}",
        hits,
        output2_text
    );
    assert!(
        misses >= 1,
        "Should have at least 1 cache miss for modified file, got {}: {}",
        misses,
        output2_text
    );
    assert!(output2.status.success());
}
