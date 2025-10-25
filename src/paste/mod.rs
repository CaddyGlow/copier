mod path_hint;

use std::fs;
use std::io::{self, IsTerminal, Read};

use camino::{Utf8Path, Utf8PathBuf};
use dialoguer::Confirm;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use tracing::{info, warn};

use crate::config::{AppContext, ConflictStrategy, InputSource, PasteConfig};
use crate::error::{QuickctxError, Result};
use crate::utils;

pub fn run(_context: &AppContext, config: PasteConfig) -> Result<()> {
    let markdown = read_input(&config.source)?;
    let blocks = parse_blocks(&markdown)?;

    for block in blocks {
        write_block(&config, &block)?;
    }

    info!("paste complete");
    Ok(())
}

#[derive(Debug)]
struct FileBlock {
    path: Utf8PathBuf,
    contents: String,
}

fn read_input(source: &InputSource) -> Result<String> {
    match source {
        InputSource::File(path) => fs::read_to_string(path.as_std_path())
            .map_err(|e| QuickctxError::Io(io::Error::new(e.kind(), format!("{}: {}", path, e)))),
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

impl ParserState {
    /// Transition from Idle state to InHeading state
    fn transition_to_heading(self) -> Self {
        match self {
            ParserState::Idle { trailing_text, .. } => ParserState::InHeading {
                buffer: String::new(),
                has_inline_code: false,
                trailing_text,
            },
            _ => ParserState::InHeading {
                buffer: String::new(),
                has_inline_code: false,
                trailing_text: String::new(),
            },
        }
    }

    /// Transition from InHeading state to Idle state
    fn transition_to_idle_from_heading(self) -> Self {
        match self {
            ParserState::InHeading {
                buffer,
                has_inline_code,
                trailing_text,
            } => {
                let heading_hint = if has_inline_code {
                    Some(buffer.trim().to_string())
                } else {
                    None
                };
                ParserState::Idle {
                    trailing_text,
                    heading_hint,
                }
            }
            _ => ParserState::Idle {
                trailing_text: String::new(),
                heading_hint: None,
            },
        }
    }

    /// Transition from Idle state to InCodeBlock state
    fn transition_to_code_block(self) -> Self {
        let hint = match self {
            ParserState::Idle {
                mut trailing_text,
                heading_hint,
            } => path_hint::acquire_path_hint(&mut trailing_text, heading_hint),
            _ => None,
        };
        ParserState::InCodeBlock {
            state: BlockState::new(hint),
        }
    }

    /// Transition from InCodeBlock state to Idle state, returning the finished block
    fn transition_to_idle_from_code_block(self) -> Result<(Self, Option<FileBlock>)> {
        match self {
            ParserState::InCodeBlock { state } => {
                let block = state.finish()?;
                Ok((
                    ParserState::Idle {
                        trailing_text: String::new(),
                        heading_hint: None,
                    },
                    Some(block),
                ))
            }
            _ => Ok((
                ParserState::Idle {
                    trailing_text: String::new(),
                    heading_hint: None,
                },
                None,
            )),
        }
    }

    /// Delegate text pushing to the appropriate state variant
    fn push_text(&mut self, text: &str) {
        match self {
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

    /// Delegate code pushing to the appropriate state variant
    fn push_code(&mut self, text: &str) {
        match self {
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

    /// Delegate character pushing to the appropriate state variant
    fn push_char(&mut self, ch: char) {
        match self {
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
    let mut state = ParserState::Idle {
        trailing_text: String::new(),
        heading_hint: None,
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(markdown, options);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                let old_state = std::mem::replace(
                    &mut state,
                    ParserState::Idle {
                        trailing_text: String::new(),
                        heading_hint: None,
                    },
                );
                state = old_state.transition_to_heading();
            }
            Event::End(TagEnd::Heading(_)) => {
                let old_state = std::mem::replace(
                    &mut state,
                    ParserState::Idle {
                        trailing_text: String::new(),
                        heading_hint: None,
                    },
                );
                state = old_state.transition_to_idle_from_heading();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                if !matches!(kind, CodeBlockKind::Fenced(_)) {
                    continue;
                }
                let old_state = std::mem::replace(
                    &mut state,
                    ParserState::Idle {
                        trailing_text: String::new(),
                        heading_hint: None,
                    },
                );
                state = old_state.transition_to_code_block();
            }
            Event::End(TagEnd::CodeBlock) => {
                let old_state = std::mem::replace(
                    &mut state,
                    ParserState::Idle {
                        trailing_text: String::new(),
                        heading_hint: None,
                    },
                );
                let (new_state, block) = old_state.transition_to_idle_from_code_block()?;
                state = new_state;
                if let Some(block) = block {
                    blocks.push(block);
                }
            }
            Event::End(TagEnd::Paragraph) => {
                // Add newline at end of paragraphs to preserve line breaks in trailing text
                state.push_char('\n');
            }
            Event::Text(text) => state.push_text(&text),
            Event::Code(text) => state.push_code(&text),
            Event::Html(text) | Event::InlineHtml(text) => state.push_text(&text),
            Event::SoftBreak => state.push_char('\n'),
            Event::HardBreak => state.push_char('\n'),
            _ => {}
        }
    }

    Ok(blocks)
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
        let path = if let Some(comment_path) = path_hint::extract_comment_hint(&mut self.contents) {
            comment_path
        } else if let Some(hint) = self.path_hint.take() {
            hint
        } else {
            return Err(QuickctxError::Markdown(
                "unable to determine file path".into(),
            ));
        };

        let path = path_hint::sanitize_relative(&path)?;

        Ok(FileBlock {
            path,
            contents: self.contents,
        })
    }
}

fn write_block(config: &PasteConfig, block: &FileBlock) -> Result<()> {
    let destination = config.output_dir.join(&block.path);

    if destination.exists() && !should_overwrite(&destination, config.conflict)? {
        warn!(path = %destination, "skipping existing file");
        return Ok(());
    }

    utils::write_with_parent(&destination, block.contents.as_bytes())?;
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
        .map_err(std::io::Error::other)?;
    Ok(confirmed)
}
