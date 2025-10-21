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
    match config.format {
        OutputFormat::Simple => render_simple(entry, config, buffer),
        OutputFormat::Comment => render_comment(entry, config, buffer),
        OutputFormat::Heading => render_heading(entry, config, buffer),
    }
}

fn render_simple(entry: &FileEntry, config: &AggregateConfig, buffer: &mut String) -> Result<()> {
    buffer.push_str(entry.relative.as_str());
    buffer.push('\n');
    buffer.push('\n');
    render_fenced(entry, config, buffer, None)
}

fn render_comment(entry: &FileEntry, config: &AggregateConfig, buffer: &mut String) -> Result<()> {
    let prefix_line = format!("// {}\n", entry.relative);
    render_fenced(entry, config, buffer, Some(prefix_line.as_str()))
}

fn render_heading(entry: &FileEntry, config: &AggregateConfig, buffer: &mut String) -> Result<()> {
    buffer.push_str("## `");
    buffer.push_str(entry.relative.as_str());
    buffer.push_str("`\n\n");
    render_fenced(entry, config, buffer, None)
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
        match preference {
            FencePreference::Backtick => Self::for_char(content, '`'),
            FencePreference::Tilde => Self::for_char(content, '~'),
            FencePreference::Auto => {
                let backtick = Self::for_char(content, '`');
                if content.contains(backtick.delimiter.as_str()) {
                    Self::for_char(content, '~')
                } else {
                    backtick
                }
            }
        }
    }

    fn for_char(content: &str, ch: char) -> Self {
        let mut count = 3usize;
        loop {
            let delimiter = ch.to_string().repeat(count);
            if !content.contains(&delimiter) {
                return Self { delimiter };
            }
            count += 1;
            if count > 8 {
                return Self { delimiter };
            }
        }
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
