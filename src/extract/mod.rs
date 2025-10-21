use std::io::{self, IsTerminal, Read};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use dialoguer::Confirm;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use tracing::{info, warn};

use crate::config::{AppContext, ConflictStrategy, ExtractConfig, InputSource};
use crate::error::{CopierError, Result};
use crate::fs;

pub fn run(_context: &AppContext, config: ExtractConfig) -> Result<()> {
    let markdown = read_input(&config.source)?;
    let blocks = parse_blocks(&markdown)?;

    for block in blocks {
        write_block(&config, &block)?;
    }

    info!("extraction complete");
    Ok(())
}

#[derive(Debug)]
struct FileBlock {
    path: Utf8PathBuf,
    contents: String,
}

fn read_input(source: &InputSource) -> Result<String> {
    match source {
        InputSource::File(path) => fs::read_to_string(path),
        InputSource::Stdin => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Explicit parser states - mutually exclusive and type-safe
enum ParserState {
    /// Between markdown elements, accumulating text that may be file path hints
    Idle {
        trailing_text: String,
        heading_hint: Option<String>,
    },
    /// Currently parsing a heading element
    InHeading {
        buffer: String,
        has_inline_code: bool,
        trailing_text: String,
    },
    /// Currently parsing a fenced code block
    InCodeBlock { state: BlockState },
}

/// Context for parsing markdown events into file blocks
struct ParserContext {
    state: ParserState,
}

impl ParserContext {
    fn new() -> Self {
        Self {
            state: ParserState::Idle {
                trailing_text: String::new(),
                heading_hint: None,
            },
        }
    }

    fn start_heading(&mut self) {
        let trailing_text = match &self.state {
            ParserState::Idle { trailing_text, .. } => trailing_text.clone(),
            _ => String::new(),
        };

        self.state = ParserState::InHeading {
            buffer: String::new(),
            has_inline_code: false,
            trailing_text,
        };
    }

    fn end_heading(&mut self) {
        if let ParserState::InHeading {
            buffer,
            has_inline_code,
            trailing_text,
        } = std::mem::replace(
            &mut self.state,
            ParserState::Idle {
                trailing_text: String::new(),
                heading_hint: None,
            },
        ) {
            let heading_hint = if has_inline_code {
                Some(buffer.trim().to_string())
            } else {
                None
            };

            self.state = ParserState::Idle {
                trailing_text,
                heading_hint,
            };
        }
    }

    fn start_code_block(&mut self) {
        let hint = match &mut self.state {
            ParserState::Idle {
                trailing_text,
                heading_hint,
            } => acquire_path_hint(trailing_text, heading_hint.take()),
            _ => None,
        };

        self.state = ParserState::InCodeBlock {
            state: BlockState::new(hint),
        };
    }

    fn end_code_block(&mut self) -> Result<Option<FileBlock>> {
        if let ParserState::InCodeBlock { state } = std::mem::replace(
            &mut self.state,
            ParserState::Idle {
                trailing_text: String::new(),
                heading_hint: None,
            },
        ) {
            Ok(Some(state.finish()?))
        } else {
            Ok(None)
        }
    }

    fn push_text(&mut self, text: &str) {
        match &mut self.state {
            ParserState::Idle { trailing_text, .. } => {
                trailing_text.push_str(text);
            }
            ParserState::InHeading { buffer, .. } => {
                buffer.push_str(text);
            }
            ParserState::InCodeBlock { state } => {
                state.push_text(text);
            }
        }
    }

    fn push_code(&mut self, text: &str) {
        match &mut self.state {
            ParserState::Idle { trailing_text, .. } => {
                trailing_text.push_str(text);
            }
            ParserState::InHeading {
                buffer,
                has_inline_code,
                ..
            } => {
                buffer.push_str(text);
                *has_inline_code = true;
            }
            ParserState::InCodeBlock { state } => {
                state.push_text(text);
            }
        }
    }

    fn push_char(&mut self, ch: char) {
        match &mut self.state {
            ParserState::Idle { trailing_text, .. } => {
                trailing_text.push(ch);
            }
            ParserState::InHeading { buffer, .. } => {
                buffer.push(ch);
            }
            ParserState::InCodeBlock { state } => {
                state.push_char(ch);
            }
        }
    }
}

fn parse_blocks(markdown: &str) -> Result<Vec<FileBlock>> {
    let mut blocks = Vec::new();
    let mut ctx = ParserContext::new();

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(markdown, options);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                ctx.start_heading();
            }
            Event::End(TagEnd::Heading(_)) => {
                ctx.end_heading();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                if !matches!(kind, CodeBlockKind::Fenced(_)) {
                    continue;
                }
                ctx.start_code_block();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = ctx.end_code_block()? {
                    blocks.push(block);
                }
            }
            Event::End(TagEnd::Paragraph) => {
                // Add newline at end of paragraphs to preserve line breaks in trailing text
                ctx.push_char('\n');
            }
            Event::Text(text) => ctx.push_text(&text),
            Event::Code(text) => ctx.push_code(&text),
            Event::Html(text) | Event::InlineHtml(text) => ctx.push_text(&text),
            Event::SoftBreak => ctx.push_char('\n'),
            Event::HardBreak => ctx.push_char('\n'),
            _ => {}
        }
    }

    Ok(blocks)
}

fn acquire_path_hint(trailing_text: &mut String, heading: Option<String>) -> Option<String> {
    // Heading takes priority if it was inline code (wrapped in backticks)
    if let Some(heading) = heading {
        trailing_text.clear();
        return Some(heading);
    }

    // Otherwise, look for trailing text hint
    let candidate = trailing_text.trim();
    let hint = candidate.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    trailing_text.clear();
    hint
}

struct BlockState {
    path_hint: Option<String>,
    contents: String,
}

impl BlockState {
    fn new(path_hint: Option<String>) -> Self {
        Self {
            path_hint,
            contents: String::new(),
        }
    }

    fn push_text(&mut self, text: &str) {
        self.contents.push_str(text);
    }

    fn push_char(&mut self, ch: char) {
        self.contents.push(ch);
    }

    fn finish(mut self) -> Result<FileBlock> {
        // Priority order:
        // 1. Comment hint inside code block (most explicit)
        // 2. Path hint from heading or trailing text
        let path = if let Some(comment_path) = extract_comment_hint(&mut self.contents) {
            comment_path
        } else if let Some(hint) = self.path_hint.take() {
            hint
        } else {
            return Err(CopierError::Markdown(
                "unable to determine file path".into(),
            ));
        };

        let path = sanitize_relative(&path)?;

        Ok(FileBlock {
            path,
            contents: self.contents,
        })
    }
}

fn extract_comment_hint(contents: &mut String) -> Option<String> {
    let prefix_candidates = ["//", "#", ";", "--"];
    for prefix in &prefix_candidates {
        let marker = format!("{prefix} ");
        if contents.starts_with(&marker) {
            if let Some(idx) = contents.find('\n') {
                let path = contents[marker.len()..idx].trim().to_string();
                let remainder = contents[idx + 1..].to_string();
                *contents = remainder;
                return Some(path);
            } else {
                let path = contents[marker.len()..].trim().to_string();
                contents.clear();
                return Some(path);
            }
        }
    }
    None
}

fn sanitize_relative(raw: &str) -> Result<Utf8PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CopierError::Markdown("empty file path".into()));
    }

    let candidate = Utf8PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return Err(CopierError::Markdown(format!(
            "absolute paths are not allowed: {trimmed}"
        )));
    }

    if candidate
        .components()
        .any(|c| matches!(c, Utf8Component::ParentDir))
    {
        return Err(CopierError::Markdown(format!(
            "parent directory segments are not allowed: {trimmed}"
        )));
    }

    Ok(candidate)
}

fn write_block(config: &ExtractConfig, block: &FileBlock) -> Result<()> {
    let destination = config.output_dir.join(&block.path);

    if destination.exists() && !should_overwrite(&destination, config.conflict)? {
        warn!(path = %destination, "skipping existing file");
        return Ok(());
    }

    fs::write(&destination, block.contents.as_bytes())?;
    info!(path = %destination, "wrote file");
    Ok(())
}

fn should_overwrite(path: &Utf8Path, strategy: ConflictStrategy) -> Result<bool> {
    match strategy {
        ConflictStrategy::Overwrite => Ok(true),
        ConflictStrategy::Skip => Ok(false),
        ConflictStrategy::Prompt => prompt_overwrite(path),
    }
}

fn prompt_overwrite(path: &Utf8Path) -> Result<bool> {
    if !io::stdout().is_terminal() {
        return Ok(false);
    }

    let prompt = format!("{path} exists. Overwrite?");
    let confirmed = Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|err| CopierError::Other(err.into()))?;
    Ok(confirmed)
}
