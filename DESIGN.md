# Architecture & Design

## Architectural Style
- The binary follows a layered design: **CLI orchestration → mode coordinators → reusable services**. `main.rs` wires command-line parsing (via `clap`) into an `App` that executes either aggregation or extraction.
- Each mode is modelled as a `Command` with a shared `Context` (resolved config, ignore settings, logging), enabling future subcommands (e.g., `watch`, `diff`) without rewriting the driver.
- Core behaviors are implemented as composable pipelines. Services expose small traits so they can be swapped for testing (e.g., in-memory sources) or extended (new renderers, alternative parsers).

## Data Flow
### Aggregation Mode
1. **Input Resolution** - Expands glob patterns, resolves paths to absolute form, uses `ignore::WalkBuilder` for directory traversal.
2. **Filtering** - `WalkBuilder` respects `.gitignore` by default (unless `--no-gitignore`), applies custom ignore files, and excludes patterns via `GlobSet`.
3. **Loading** - Reads file contents into `FileEntry { absolute, relative, contents, language }`. Language is detected from file extension via `utils::language` module.
4. **Rendering** - Calls `render::render_entries()` which dispatches to format-specific functions (simple/comment/heading). Each uses `Fence::determine()` to select safe delimiters.
5. **Emission** - Writes to stdout (via locked handle) or to file path specified in config. Uses standard `std::io::Write`.

### Extraction Mode
1. **Document Ingestion** - Reads markdown from stdin or file into memory as a `String`.
2. **Block Parsing** - `pulldown-cmark::Parser` produces event stream. State machine (`ParserState`) tracks Idle/InHeading/InCodeBlock states to extract file blocks.
3. **Path Detection** - Three strategies: (1) text before code block looks like path, (2) heading with backticks contains path, (3) first line of code block has `// path` comment.
4. **Conflict Resolution** - Before writing, checks if file exists. Applies strategy: `skip` (log and continue), `overwrite` (replace), `prompt` (ask via `dialoguer` if terminal).
5. **File Writing** - Creates parent directories, writes file content via `fs::write_string()`. Logs each file written.

## Module Layout (current structure)
- `src/main.rs` – CLI entry point, parses arguments with clap and delegates to `lib::run()`.
- `src/lib.rs` – Main orchestration, loads config, initializes telemetry, and routes to modes.
- `src/cli.rs` – CLI argument definitions using clap's derive API.
- `src/config/` – Configuration loading/merging from CLI, file, and defaults; enum types for formats.
- `src/fs/` – File system utilities (read, write, path canonicalization) without gitignore logic.
- `src/aggregate/` – File collection pipeline using `ignore` crate WalkBuilder, renders via `render` module.
- `src/extract/` – Markdown parser using `pulldown-cmark`, state machine for path detection, file writing.
- `src/render/` – Output formatting (Simple, Comment, Heading) and smart fence detection logic.
- `src/utils/` – Shared helpers including language detection tables (in `language.rs`).
- `src/telemetry.rs` – Logging initialization using `tracing` and `tracing-subscriber`.
- `src/error.rs` – Custom error types and Result alias.
- `tests/` – Integration tests that exercise aggregate/extract round-trips.

## Key Components & Patterns
- **Configuration**: `RuntimeConfig` merges CLI arguments (via `clap`), `copier.toml` file (via `serde` + `toml`), and defaults. Config is split into `AppContext` (verbosity, working dir) and mode-specific configs (`AggregateConfig` or `ExtractConfig`).
- **Output Formats**: `OutputFormat` enum (Simple, Comment, Heading) determines rendering strategy. Each format is implemented as a function (`render_simple`, `render_comment`, `render_heading`) that delegates to `render_fenced` with format-specific prefixes.
- **Smart Fence Detection**: `Fence::determine()` scans file contents to find a safe fence delimiter. Uses configurable `FencePreference` (Auto, Backtick, Tilde) and automatically increments fence length (3+) to avoid conflicts.
- **Mode Functions**: Aggregation and extraction are implemented as `aggregate::run()` and `extract::run()` functions, each taking context and mode-specific config. Simple function-based architecture rather than trait-based.
- **Markdown Parsing**: Extract mode uses explicit state machine (`ParserState` enum with Idle, InHeading, InCodeBlock states) built on `pulldown-cmark::Event` stream. Handles multiple path hint formats (simple, comment, heading).
- **Error Handling**: Custom `CopierError` enum with variants for different error types (Config, IO, Parse, etc.). Type alias `Result<T>` = `std::result::Result<T, CopierError>` used throughout.

## Cross-Cutting Concerns
- **Logging & UX**: Uses `tracing` with `tracing-subscriber` for structured logs, configured by verbosity flags (`-v`). Aggregation logs skipped files, extraction logs written files and conflicts.
- **Performance**: Uses streaming IO where practical. File traversal is single-threaded via `ignore::WalkBuilder`. Content is loaded into memory (not chunked for large files in current implementation).
- **Testing**: Integration tests in `tests/` directory test round-trip behavior (aggregate then extract). Unit tests are co-located in module files within `#[cfg(test)]` blocks.
- **Extensibility**: New output formats can be added by extending the `OutputFormat` enum and adding a rendering function. Configuration uses Rust enums with `ValueEnum` derive for CLI and `Deserialize` for TOML.

## External Dependencies (current)
- `clap` (v4.5) - CLI parsing with derive macros
- `ignore` (v0.4) - Gitignore-aware directory traversal
- `pulldown-cmark` (v0.13) - Markdown event stream parser
- `camino` (v1.1) - UTF-8 path handling (Utf8Path, Utf8PathBuf)
- `serde` (v1.0) + `toml` (v0.9) - Config file parsing
- `tracing` (v0.1) + `tracing-subscriber` (v0.3) - Structured logging
- `dialoguer` (v0.12) - Interactive prompts for conflict resolution
- `glob` (v0.3), `globwalk` (v0.9), `globset` (v0.4) - Glob pattern handling
- `thiserror` (v1.0) - Error type derive macros
- `anyhow` (v1.0) - Error context and propagation
- `once_cell` (v1.19) - Lazy static initialization
- `strum` (v0.27) - Enum string conversions (Display, EnumString)
- `serde_path_to_error` (v0.1) - Better config parsing errors

## Future Enhancements
- Abstract a `Workspace` service to support remote sources or virtual files.
- Add incremental aggregation by hashing file contents and emitting change sets.
- Layer plugin registry for custom renderers or metadata emitters without modifying core crates.
