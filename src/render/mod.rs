use crate::aggregate::FileEntry;
use crate::config::{AggregateConfig, FencePreference, OutputFormat};
use crate::error::Result;

pub fn render_entries(entries: &[FileEntry], config: &AggregateConfig) -> Result<String> {
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

fn render_entry(entry: &FileEntry, config: &AggregateConfig, buffer: &mut String) -> Result<()> {
    // Strategy pattern: each format defines preamble (before fence) and code_prefix (inside fence)
    let (preamble, code_prefix) = match config.format {
        OutputFormat::Simple => (format!("{}\n\n", entry.relative), None),
        OutputFormat::Comment => (String::new(), Some(format!("// {}\n", entry.relative))),
        OutputFormat::Heading => (format!("## `{}`\n\n", entry.relative), None),
    };

    buffer.push_str(&preamble);
    render_fenced(entry, config, buffer, code_prefix.as_deref())
}

fn render_fenced(
    entry: &FileEntry,
    config: &AggregateConfig,
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
