use copier::aggregate::FileEntry;
use copier::config::{AggregateConfig, FencePreference, OutputFormat};
use copier::render;

fn make_entry(relative: &str, contents: &str, language: Option<&str>) -> FileEntry {
    FileEntry {
        absolute: format!("/abs/{}", relative).into(),
        relative: relative.into(),
        contents: contents.to_string(),
        language: language.map(String::from),
    }
}

fn make_config(format: OutputFormat, fence: FencePreference) -> AggregateConfig {
    AggregateConfig {
        inputs: vec![],
        output: None,
        format,
        fence,
        respect_gitignore: true,
        ignore_files: vec![],
        excludes: vec![],
    }
}

#[test]
fn test_render_single_entry_simple_format() {
    let entry = make_entry("test.rs", "fn main() {}", Some("rust"));
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("test.rs"));
    assert!(output.contains("```rust"));
    assert!(output.contains("fn main() {}"));
    assert!(output.contains("```"));
}

#[test]
fn test_render_single_entry_comment_format() {
    let entry = make_entry("src/lib.rs", "pub fn hello() {}", Some("rust"));
    let config = make_config(OutputFormat::Comment, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("// src/lib.rs"));
    assert!(output.contains("```rust"));
    assert!(output.contains("pub fn hello() {}"));
}

#[test]
fn test_render_single_entry_heading_format() {
    let entry = make_entry("main.py", "def main():\n    pass", Some("python"));
    let config = make_config(OutputFormat::Heading, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("## `main.py`"));
    assert!(output.contains("```python"));
    assert!(output.contains("def main():"));
}

#[test]
fn test_render_multiple_entries() {
    let entries = vec![
        make_entry("file1.rs", "fn one() {}", Some("rust")),
        make_entry("file2.rs", "fn two() {}", Some("rust")),
        make_entry("file3.rs", "fn three() {}", Some("rust")),
    ];
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&entries, &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("file1.rs"));
    assert!(output.contains("file2.rs"));
    assert!(output.contains("file3.rs"));
    assert!(output.contains("fn one()"));
    assert!(output.contains("fn two()"));
    assert!(output.contains("fn three()"));

    // Should have separators between entries
    let parts: Vec<&str> = output.split("\n\n").collect();
    assert!(parts.len() >= 3);
}

#[test]
fn test_render_empty_entries() {
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert_eq!(output, "");
}

#[test]
fn test_render_entry_without_trailing_newline() {
    let entry = make_entry("test.txt", "no newline at end", None);
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should add newline before closing fence
    assert!(output.contains("no newline at end\n```"));
}

#[test]
fn test_render_entry_with_trailing_newline() {
    let entry = make_entry("test.txt", "has newline\n", None);
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should not add extra newline
    assert!(output.contains("has newline\n```"));
}

#[test]
fn test_fence_preference_backtick() {
    let entry = make_entry("test.rs", "fn main() {}", Some("rust"));
    let config = make_config(OutputFormat::Simple, FencePreference::Backtick);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("```rust"));
    assert!(output.contains("```\n"));
}

#[test]
fn test_fence_preference_tilde() {
    let entry = make_entry("test.rs", "fn main() {}", Some("rust"));
    let config = make_config(OutputFormat::Simple, FencePreference::Tilde);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("~~~rust"));
    assert!(output.contains("~~~\n"));
}

#[test]
fn test_fence_auto_uses_backtick_by_default() {
    let entry = make_entry("test.rs", "fn main() {}", Some("rust"));
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("```rust"));
}

#[test]
fn test_fence_auto_switches_to_tilde_when_backticks_in_content() {
    // Content contains all reasonable lengths of backticks to force tilde usage
    let entry = make_entry("test.md", "``` ```` ````` `````` ``````` ````````", None);
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should use tildes since content has many backtick sequences
    assert!(output.contains("~~~"));
}

#[test]
fn test_fence_increases_length_when_delimiter_in_content() {
    let entry = make_entry("test.txt", "```\ncode with ``` fences\n```", None);
    let config = make_config(OutputFormat::Simple, FencePreference::Backtick);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should use longer fence (4+ backticks) to avoid conflict
    assert!(
        output.contains("````") || output.contains("`````"),
        "Should use longer fence, got: {}",
        output
    );
}

#[test]
fn test_fence_handles_very_long_delimiter_sequences() {
    // Content with many backticks
    let content = "`````````"; // 9 backticks
    let entry = make_entry("test.txt", content, None);
    let config = make_config(OutputFormat::Simple, FencePreference::Backtick);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should cap at reasonable length or handle it gracefully
    assert!(output.contains(content));
}

#[test]
fn test_render_entry_without_language() {
    let entry = make_entry("unknown.xyz", "some content", None);
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should have fence without language specifier
    assert!(output.contains("```\n"));
    assert!(output.contains("some content"));
}

#[test]
fn test_render_entry_with_empty_language() {
    let entry = make_entry("test.txt", "content", Some(""));
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should render fence without language when language is empty
    assert!(output.contains("```\n"));
}

#[test]
fn test_comment_format_with_nested_path() {
    let entry = make_entry("src/nested/deep/file.rs", "// code", Some("rust"));
    let config = make_config(OutputFormat::Comment, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("// src/nested/deep/file.rs"));
}

#[test]
fn test_heading_format_with_special_characters() {
    let entry = make_entry("test-file_name.rs", "code", Some("rust"));
    let config = make_config(OutputFormat::Heading, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("## `test-file_name.rs`"));
}

#[test]
fn test_render_multiple_formats_mixed() {
    // Test that different configs produce different outputs
    let entry = make_entry("test.rs", "fn main() {}", Some("rust"));

    let simple_config = make_config(OutputFormat::Simple, FencePreference::Auto);
    let simple_output =
        render::render_entries(std::slice::from_ref(&entry), &simple_config).unwrap();

    let comment_config = make_config(OutputFormat::Comment, FencePreference::Auto);
    let comment_output =
        render::render_entries(std::slice::from_ref(&entry), &comment_config).unwrap();

    let heading_config = make_config(OutputFormat::Heading, FencePreference::Auto);
    let heading_output = render::render_entries(&[entry], &heading_config).unwrap();

    // Each format should be different
    assert_ne!(simple_output, comment_output);
    assert_ne!(simple_output, heading_output);
    assert_ne!(comment_output, heading_output);

    // But all should contain the code
    assert!(simple_output.contains("fn main() {}"));
    assert!(comment_output.contains("fn main() {}"));
    assert!(heading_output.contains("fn main() {}"));
}

#[test]
fn test_render_multiline_content() {
    let content = "line 1\nline 2\nline 3\n";
    let entry = make_entry("multi.txt", content, None);
    let config = make_config(OutputFormat::Simple, FencePreference::Auto);

    let result = render::render_entries(&[entry], &config);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.contains("line 1\nline 2\nline 3\n"));
}
