use camino::{Utf8Path, Utf8PathBuf};
use quickctx::utils::{is_probably_binary, language_for_path, looks_like_glob, relative_to};

#[test]
fn test_language_for_path_by_extension() {
    assert_eq!(language_for_path(Utf8Path::new("test.rs")), Some("rust"));
    assert_eq!(language_for_path(Utf8Path::new("test.py")), Some("python"));
    assert_eq!(
        language_for_path(Utf8Path::new("test.js")),
        Some("javascript")
    );
    assert_eq!(language_for_path(Utf8Path::new("test.go")), Some("go"));
    assert_eq!(language_for_path(Utf8Path::new("test.java")), Some("java"));
}

#[test]
fn test_language_for_path_by_filename() {
    assert_eq!(
        language_for_path(Utf8Path::new("Dockerfile")),
        Some("dockerfile")
    );
    assert_eq!(
        language_for_path(Utf8Path::new("Makefile")),
        Some("makefile")
    );
    assert_eq!(
        language_for_path(Utf8Path::new("CMakeLists.txt")),
        Some("cmake")
    );
    assert_eq!(language_for_path(Utf8Path::new("BUILD")), Some("starlark"));
    assert_eq!(
        language_for_path(Utf8Path::new("WORKSPACE")),
        Some("starlark")
    );
}

#[test]
fn test_language_for_path_case_insensitive() {
    // Extension matching should be case-insensitive
    assert_eq!(language_for_path(Utf8Path::new("TEST.RS")), Some("rust"));
    assert_eq!(language_for_path(Utf8Path::new("test.PY")), Some("python"));
}

#[test]
fn test_language_for_path_filename_lowercase_match() {
    // Test that lowercase filename matching works (e.g., "dockerfile" without extension)
    assert_eq!(
        language_for_path(Utf8Path::new("dockerfile")),
        Some("dockerfile")
    );
    assert_eq!(
        language_for_path(Utf8Path::new("makefile")),
        Some("makefile")
    );
}

#[test]
fn test_language_for_path_with_dots() {
    // Files with multiple dots should use the last extension
    assert_eq!(language_for_path(Utf8Path::new("my.file.rs")), Some("rust"));
    assert_eq!(
        language_for_path(Utf8Path::new("app.test.js")),
        Some("javascript")
    );
}

#[test]
fn test_language_for_path_unknown() {
    assert_eq!(language_for_path(Utf8Path::new("unknown.xyz")), None);
    assert_eq!(language_for_path(Utf8Path::new("noext")), None);
}

#[test]
fn test_language_for_path_nested() {
    assert_eq!(
        language_for_path(Utf8Path::new("src/main.rs")),
        Some("rust")
    );
    assert_eq!(
        language_for_path(Utf8Path::new("deep/nested/path/test.py")),
        Some("python")
    );
}

#[test]
fn test_looks_like_glob_true() {
    assert!(looks_like_glob("*.rs"));
    assert!(looks_like_glob("src/**/*.js"));
    assert!(looks_like_glob("test?.txt"));
    assert!(looks_like_glob("[abc].rs"));
    assert!(looks_like_glob("src/*.{rs,toml}"));
}

#[test]
fn test_looks_like_glob_false() {
    assert!(!looks_like_glob("src/main.rs"));
    assert!(!looks_like_glob("test.txt"));
    assert!(!looks_like_glob("directory/file.rs"));
}

#[test]
fn test_relative_to_with_base() {
    let path = Utf8Path::new("/home/user/project/src/main.rs");
    let base = Utf8Path::new("/home/user/project");
    assert_eq!(relative_to(path, base), Utf8PathBuf::from("src/main.rs"));
}

#[test]
fn test_relative_to_exact_match() {
    let path = Utf8Path::new("/home/user/project");
    let base = Utf8Path::new("/home/user/project");
    assert_eq!(relative_to(path, base), Utf8PathBuf::from(""));
}

#[test]
fn test_relative_to_no_common_prefix() {
    let path = Utf8Path::new("/var/log/test.txt");
    let base = Utf8Path::new("/home/user");
    // Should return the original path when not a prefix
    assert_eq!(
        relative_to(path, base),
        Utf8PathBuf::from("/var/log/test.txt")
    );
}

#[test]
fn test_relative_to_relative_paths() {
    let path = Utf8Path::new("src/main.rs");
    let base = Utf8Path::new("src");
    assert_eq!(relative_to(path, base), Utf8PathBuf::from("main.rs"));
}

#[test]
fn test_is_probably_binary_text() {
    let text = b"This is plain text content\nWith multiple lines\n";
    assert!(!is_probably_binary(text));
}

#[test]
fn test_is_probably_binary_with_null_byte() {
    let binary = b"Some text\x00with null byte";
    assert!(is_probably_binary(binary));
}

#[test]
fn test_is_probably_binary_with_many_control_chars() {
    // Create data with many control characters
    let mut binary = vec![0x01; 200]; // Many control characters
    binary.extend_from_slice(b"some text");
    assert!(is_probably_binary(&binary));
}

#[test]
fn test_is_probably_binary_large_text() {
    // Test with data larger than SAMPLE_LIMIT (1024 bytes)
    let large_text = "a".repeat(2000).into_bytes();
    assert!(!is_probably_binary(&large_text));
}

#[test]
fn test_is_probably_binary_large_binary() {
    // Test with binary data larger than SAMPLE_LIMIT
    let large_binary = vec![0x00; 2000];
    assert!(is_probably_binary(&large_binary));
}

#[test]
fn test_is_probably_binary_with_valid_control_chars() {
    // Newlines, tabs, and carriage returns should be acceptable
    let text = b"Line 1\nLine 2\r\nLine 3\tTabbed";
    assert!(!is_probably_binary(text));
}

#[test]
fn test_is_probably_binary_edge_case() {
    // Empty data
    assert!(!is_probably_binary(b""));

    // Single byte
    assert!(!is_probably_binary(b"a"));
    assert!(is_probably_binary(b"\x00"));
}

#[test]
fn test_is_probably_binary_unicode() {
    // UTF-8 text should not be considered binary
    let unicode = "Hello 世界 🌍".as_bytes();
    assert!(!is_probably_binary(unicode));
}

#[test]
fn test_language_for_common_extensions() {
    // Test various common programming languages
    assert_eq!(language_for_path(Utf8Path::new("test.c")), Some("c"));
    assert_eq!(language_for_path(Utf8Path::new("test.h")), Some("c"));
    assert_eq!(language_for_path(Utf8Path::new("test.cpp")), Some("cpp"));
    assert_eq!(language_for_path(Utf8Path::new("test.hpp")), Some("cpp"));
    assert_eq!(language_for_path(Utf8Path::new("test.rb")), Some("ruby"));
    assert_eq!(language_for_path(Utf8Path::new("test.kt")), Some("kotlin"));
    assert_eq!(
        language_for_path(Utf8Path::new("test.swift")),
        Some("swift")
    );
    assert_eq!(language_for_path(Utf8Path::new("test.php")), Some("php"));
    assert_eq!(language_for_path(Utf8Path::new("test.sh")), Some("bash"));
    assert_eq!(language_for_path(Utf8Path::new("test.sql")), Some("sql"));
}

#[test]
fn test_language_for_config_files() {
    assert_eq!(
        language_for_path(Utf8Path::new("config.yaml")),
        Some("yaml")
    );
    assert_eq!(language_for_path(Utf8Path::new("config.yml")), Some("yaml"));
    assert_eq!(
        language_for_path(Utf8Path::new("config.toml")),
        Some("toml")
    );
    assert_eq!(language_for_path(Utf8Path::new("data.json")), Some("json"));
    assert_eq!(language_for_path(Utf8Path::new("style.css")), Some("css"));
    assert_eq!(language_for_path(Utf8Path::new("style.scss")), Some("scss"));
}

#[test]
fn test_language_for_web_files() {
    assert_eq!(language_for_path(Utf8Path::new("index.html")), Some("html"));
    assert_eq!(language_for_path(Utf8Path::new("app.jsx")), Some("jsx"));
    assert_eq!(language_for_path(Utf8Path::new("app.tsx")), Some("tsx"));
    assert_eq!(
        language_for_path(Utf8Path::new("types.ts")),
        Some("typescript")
    );
}
