use crate::config::{CopyConfig, FencePreference, OutputFormat};
use crate::copy::FileEntry;
use crate::error::Result;

pub fn render_entries(entries: &[FileEntry], config: &CopyConfig) -> Result<String> {
    let mut buffer = String::new();

    for (idx, entry) in entries.iter().enumerate() {
        if idx > 0 {
            buffer.push_str("\n\n");
        }
        render_entry(entry, config, &mut buffer)?;
    }

    if !entries.is_empty() {
        buffer.push('\n');
    }

    Ok(buffer)
}

fn render_entry(entry: &FileEntry, config: &CopyConfig, buffer: &mut String) -> Result<()> {
    match config.format {
        OutputFormat::Heredoc => render_heredoc(entry, buffer),
        _ => {
            // Strategy pattern: each format defines preamble (before fence) and code_prefix (inside fence)
            let (preamble, code_prefix) = match config.format {
                OutputFormat::Simple => (format!("{}\n\n", entry.relative), None),
                OutputFormat::Comment => (String::new(), Some(format!("// {}\n", entry.relative))),
                OutputFormat::Heading => (format!("## `{}`\n\n", entry.relative), None),
                OutputFormat::Heredoc => unreachable!(),
            };

            buffer.push_str(&preamble);
            render_fenced(entry, config, buffer, code_prefix.as_deref())
        }
    }
}

fn render_heredoc(entry: &FileEntry, buffer: &mut String) -> Result<()> {
    let delimiter = HeredocDelimiter::determine(&entry.contents);

    // Determine the output path: use basename for files outside cwd or above it
    let output_path = compute_heredoc_path(&entry.relative);

    // Add directory creation if the file is in a subdirectory
    if let Some(parent) = std::path::Path::new(output_path.as_str()).parent() {
        if parent != std::path::Path::new("") {
            buffer.push_str(&format!("mkdir -p '{}'\n", parent.display()));
        }
    }

    // Generate heredoc command
    buffer.push_str(&format!(
        "cat > '{}' << '{}'\n",
        output_path, delimiter.text
    ));
    buffer.push_str(&entry.contents);

    // Ensure content ends with newline before closing delimiter
    if !entry.contents.ends_with('\n') {
        buffer.push('\n');
    }

    buffer.push_str(&delimiter.text);
    buffer.push('\n');
    Ok(())
}

fn compute_heredoc_path(relative: &camino::Utf8Path) -> String {
    let path_str = relative.as_str();

    // If it's an absolute path, use just the filename
    if path_str.starts_with('/') {
        return relative
            .file_name()
            .unwrap_or("output")
            .to_string();
    }

    // If it contains ../ (going up), use just the filename
    if path_str.contains("../") || path_str.starts_with("..") {
        return relative
            .file_name()
            .unwrap_or("output")
            .to_string();
    }

    // Otherwise, it's a proper relative path within cwd, use it as-is
    path_str.to_string()
}

fn render_fenced(
    entry: &FileEntry,
    config: &CopyConfig,
    buffer: &mut String,
    prefix: Option<&str>,
) -> Result<()> {
    let fence = Fence::determine(&entry.contents, config.fence);
    buffer.push_str(&fence.open_line(entry.language.as_deref()));
    buffer.push('\n');

    if let Some(prefix) = prefix {
        buffer.push_str(prefix);
    }

    buffer.push_str(&entry.contents);
    if !entry.contents.ends_with('\n') {
        buffer.push('\n');
    }

    buffer.push_str(fence.close_line());
    buffer.push('\n');
    Ok(())
}

struct Fence {
    delimiter: String,
}

impl Fence {
    fn determine(content: &str, preference: FencePreference) -> Self {
        let ch = match preference {
            FencePreference::Backtick => '`',
            FencePreference::Tilde => '~',
            FencePreference::Auto => {
                if content.contains("```") {
                    '~'
                } else {
                    '`'
                }
            }
        };
        Self::for_char(content, ch)
    }

    fn for_char(content: &str, ch: char) -> Self {
        let delimiter = (3..=8)
            .map(|count| ch.to_string().repeat(count))
            .find(|delim| !content.contains(delim))
            .unwrap_or_else(|| ch.to_string().repeat(8));
        Self { delimiter }
    }

    fn open_line(&self, language: Option<&str>) -> String {
        match language {
            Some(lang) if !lang.is_empty() => format!("{}{}", self.delimiter, lang),
            _ => self.delimiter.clone(),
        }
    }

    fn close_line(&self) -> &str {
        &self.delimiter
    }
}

struct HeredocDelimiter {
    text: String,
}

impl HeredocDelimiter {
    fn determine(content: &str) -> Self {
        // Try standard delimiters first
        let candidates = ["EOF", "END", "HEREDOC", "CONTENT", "DATA"];

        for base in candidates {
            if !Self::content_contains_line(content, base) {
                return Self {
                    text: base.to_string(),
                };
            }
        }

        // If all standard delimiters are taken, append a number
        for i in 1..=99 {
            let candidate = format!("EOF{}", i);
            if !Self::content_contains_line(content, &candidate) {
                return Self { text: candidate };
            }
        }

        // Fallback to a very unlikely delimiter
        Self {
            text: "EOF_DELIMITER_999".to_string(),
        }
    }

    fn content_contains_line(content: &str, needle: &str) -> bool {
        // Check if the content contains the delimiter as a standalone line
        content.lines().any(|line| line.trim() == needle)
    }
}
