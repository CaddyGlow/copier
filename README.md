# Quickctx

A bidirectional file content copy-paste tool with LSP-powered code analysis. Convert between files and markdown representations, and extract rich symbol information from source code.

## Features

### Core Features
- **Copy** multiple files into a single markdown document
- **Paste** files from markdown back to the filesystem
- **Analyze** code using Language Server Protocol (LSP) for accurate symbol extraction
- **Bidirectional** - seamlessly convert back and forth between files and markdown

### Copy & Paste
- **Multiple output formats** (simple, comment-style, heading-style)
- **Smart fence detection** - automatically avoids code fence conflicts
- **Gitignore support** - respects `.gitignore` patterns by default
- **Language detection** - automatic syntax highlighting based on file extension

### Analysis Features
- **Auto-detection** - automatically detects project root and language type
- **LSP integration** - uses existing LSP servers (rust-analyzer, pylsp, etc.)
- **Symbol extraction** - functions, methods, types, variables, and global symbols
- **Documentation** - retrieves doc comments and hover information
- **Multiple formats** - markdown (human-readable), JSON, CSV, compact, symbol-list
- **Caching** - symbol cache for improved performance
- **Diagnostics** - optional error and warning reporting
- **Symbol filtering** - extract only specific symbols

## Installation

### Using Install Script (Recommended)

**Unix-like systems (Linux, macOS, Android):**
```bash
curl -fsSL https://raw.githubusercontent.com/CaddyGlow/quickctx/main/scripts/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/CaddyGlow/quickctx/main/scripts/install.ps1 | iex
```

For custom installation options, see [scripts/install.sh](scripts/install.sh) or [scripts/install.ps1](scripts/install.ps1).

### From Cargo

```bash
# Install from crates.io
cargo install quickctx

# Or use cargo-binstall for faster binary installation
cargo binstall quickctx
```

This installs both `quickctx` and `quickctx-analyze` binaries.

### From Source

```bash
git clone https://github.com/CaddyGlow/quickctx.git
cd quickctx
cargo build --release
```

The binaries will be available at:
- `target/release/quickctx`
- `target/release/quickctx-analyze`

## Quick Start

### Copy Files to Markdown

Convert files to markdown:

```bash
# Copy specific files
quickctx file1.rs file2.rs

# Copy entire directory
quickctx src/

# Use glob patterns
quickctx "src/**/*.rs"

# Output to file
quickctx src/ -o project.md
```

### Paste Files from Markdown

Convert markdown back to files:

```bash
# Paste from file
quickctx extract project.md

# Paste from stdin
cat project.md | quickctx extract

# Paste to specific directory
quickctx extract project.md -o restored/
```

### Analyze Code

Extract symbols and documentation using LSP:

```bash
# Analyze a single file (auto-detects project and LSP server)
quickctx-analyze src/main.rs

# Analyze multiple files
quickctx-analyze src/cli.rs src/lib.rs src/main.rs

# Output to file
quickctx-analyze src/lib.rs -o analysis.md

# JSON format for programmatic use
quickctx-analyze src/main.rs --format json

# Compact format for quick overview
quickctx-analyze src/main.rs --format compact
```

## Usage

### Copy Mode

```bash
quickctx [OPTIONS] [PATH]... [COMMAND]

Arguments:
  [PATH]...                Files, directories, or glob patterns to copy

Options:
      --config <FILE>      Path to configuration file (defaults to quickctx.toml)
  -v, --verbose...         Increase log verbosity (repeatable)
  -o, --output <FILE>      Write output to file instead of stdout
  -f, --format <FORMAT>    Output format [possible values: simple, comment, heading]
      --fence <FENCE>      Fence style [possible values: auto, backtick, tilde]
      --no-gitignore       Don't respect .gitignore files
      --ignore-file <FILE> Additional ignore file(s) to apply
      --exclude <GLOB>     Exclude glob pattern(s)
  -h, --help               Print help
  -V, --version            Print version
```

### Paste Mode

```bash
quickctx extract [OPTIONS] [INPUT]

Arguments:
  [INPUT]                  Markdown input file (omit to read from stdin)

Options:
  -o, --output <DIR>       Output directory [default: current directory]
      --conflict <ACTION>  Conflict handling [possible values: prompt, skip, overwrite]
  -h, --help               Print help
```

### Analysis Mode

```bash
quickctx-analyze [OPTIONS] <FILE>...

Arguments:
  <FILE>...                Source file(s) to analyze

Options:
  -f, --format <FORMAT>    Output format [possible values: markdown, json, csv,
                           compact, symbol-list] [default: markdown]
  -o, --output <OUTPUT>    Output file (defaults to stdout)
      --config <FILE>      Path to configuration file (defaults to quickctx.toml)
      --project-root <DIR> Override project root directory
      --lsp-server <CMD>   Override LSP server command
  -v, --verbose...         Increase log verbosity (repeatable)
      --diagnostics        Show diagnostics (errors/warnings) instead of symbols
      --diagnostics-timeout <SECS>
                           Timeout for diagnostics [default: 30]
      --no-gitignore       Don't respect .gitignore when walking directories
      --hidden             Include hidden files and directories
      --lsp-timeout <SECS> LSP server readiness timeout [default: 30]
      --filter-symbols <NAMES>
                           Filter to specific symbol names (comma-separated or file)
      --no-cache           Disable symbol cache (force fresh extraction)
      --clear-cache        Clear cache before running
  -h, --help               Print help
  -V, --version            Print version
```

## Output Formats

### Copy Formats

#### Simple Format

```markdown
file.c

```c
int main() {
    return 0;
}
```
```

#### Comment Format

```markdown
```c
// src/main.c
int main() {
    return 0;
}
```
```

#### Heading Format

```markdown
## `src/main.c`

```c
int main() {
    return 0;
}
```
```

### Analysis Formats

#### Markdown Format

```markdown
# Code Analysis: `src/lib.rs`

## Functions

### `run` (Function)

**Signature:** `fn(cfg: RuntimeConfig) -> Result<()>`

**Location:** Line 42-58

**Documentation:** Main entry point for the application...

---
```

#### JSON Format

```json
{
  "file": "src/lib.rs",
  "symbols": [
    {
      "name": "run",
      "kind": "Function",
      "detail": "fn(cfg: RuntimeConfig) -> Result<()>",
      "documentation": "Main entry point...",
      "line_start": 42,
      "line_end": 58
    }
  ]
}
```

#### Compact Format

One-line summaries per symbol for quick scanning.

#### CSV Format

Structured data for spreadsheet analysis:
```csv
file,name,kind,signature,line_start,line_end
src/lib.rs,run,Function,"fn(cfg: RuntimeConfig) -> Result<()>",42,58
```

#### Symbol List Format

Simple list of symbol names, one per line.

## Smart Fence Detection

Quickctx automatically detects code fences in your files and adjusts the delimiter to avoid conflicts:

**Input file contains:**
````markdown
```rust
let x = 42;
```
````

**Output uses 4 backticks:**
`````markdown
example.md

````markdown
```rust
let x = 42;
```
````
`````

## Gitignore Support

By default, quickctx respects `.gitignore` files:

```bash
# Respects .gitignore (default)
quickctx src/

# Include ignored files
quickctx src/ --no-gitignore

# Use custom ignore file
quickctx src/ --ignore-file .customignore

# Add additional exclude patterns
quickctx src/ --exclude "*.tmp" --exclude "*.bak"
```

## Configuration File

Create a `quickctx.toml` file for project-specific settings:

```toml
[general]
verbose = 1

[copy]
# Copy mode settings
paths = ["src/", "tests/"]
format = "heading"
fence = "backtick"
respect_gitignore = true
exclude = ["*.tmp", "*.bak"]
# output = "project.md"
# ignore_files = [".customignore"]

[extractor]
# Paste mode settings
# output_dir = "restored/"
conflict = "skip"

[analyze]
# Optional: default output format
format = "markdown"

# Additional paths to search for LSP server binaries
# These paths will be prepended to PATH when spawning LSP servers
# Supports tilde expansion (e.g., ~/.local/bin)
bin_paths = [
    "~/.local/share/nvim/mason/bin",  # Mason LSP servers
    # "~/mycode/.venv/bin",            # Python virtual environment
]

# LSP server commands/paths by language
[analyze.lsp_servers]
rust = "rust-analyzer"
python = "pyright-langserver --stdio"
typescript = "typescript-language-server --stdio"
javascript = "typescript-language-server --stdio"
go = "gopls"
```

Use it with:

```bash
# Config file is loaded automatically if quickctx.toml exists
quickctx src/

# Or specify a custom config file
quickctx --config my-config.toml src/
```

### Binary Search Paths

The `bin_paths` configuration in `[analyze]` allows you to extend the PATH when searching for LSP server binaries. This is useful for LSP servers installed in non-standard locations:

- **Mason** (neovim package manager): `~/.local/share/nvim/mason/bin`
- **Python virtual environments**: `~/myproject/.venv/bin`
- **Custom installations**: `~/.local/bin`, `/opt/lsp-servers/bin`

Features:
- **Tilde expansion**: `~/` is automatically expanded to your home directory
- **Prepended to PATH**: These paths are added before the existing PATH
- **Multiple paths**: Add as many directories as needed

### Configuration Priority

Settings are applied in this order (highest to lowest priority):

1. **CLI arguments** - highest priority
2. **Configuration file** - `quickctx.toml` or `--config` file
3. **Built-in defaults** - lowest priority

## Requirements

### For Copy/Paste Operations
No additional dependencies required.

### For Analysis
You need the appropriate LSP server installed for your language:

- **Rust**: `rust-analyzer` (usually installed with rustup)
- **Python**: `pylsp` or `pyright-langserver`
- **TypeScript/JavaScript**: `typescript-language-server`
- **Go**: `gopls`

## Examples

### Example 1: Share Code Snippets

Copy your project files to share with others:

```bash
quickctx src/ tests/ -f comment -o share.md
```

### Example 2: AI-Assisted Development

Export your codebase context for AI assistants:

```bash
# Copy code structure
quickctx "src/**/*.rs" --format heading -o context.md

# Analyze symbols for detailed context
quickctx-analyze src/main.rs src/lib.rs -o symbols.md
```

### Example 3: Documentation

Paste code examples from documentation:

```bash
quickctx extract API_EXAMPLES.md -o examples/
```

### Example 4: Project Templates

Create and instantiate project templates:

```bash
# Copy template
quickctx template-project/ -o rust-template.md

# Paste template
quickctx extract rust-template.md -o my-new-project/
```

### Example 5: Code Analysis

Generate comprehensive code analysis:

```bash
# Analyze all source files
quickctx-analyze src/*.rs --format json -o analysis.json

# Extract just function names
quickctx-analyze src/lib.rs --format symbol-list

# Get diagnostics
quickctx-analyze src/main.rs --diagnostics

# Filter specific symbols
quickctx-analyze src/lib.rs --filter-symbols "run,main"
```

### Example 6: Multiple Files Analysis

Analyze multiple files efficiently:

```bash
# Analyze all files in one pass (shares LSP server)
quickctx-analyze src/main.rs src/lib.rs src/config.rs -o analysis.md

# Use glob pattern with find
find src -name "*.rs" -exec quickctx-analyze {} -o {}.analysis.md \;
```

## Use Cases

- **Code Reviews**: Share code context easily with reviewers
- **AI Assistants**: Provide comprehensive codebase context to LLMs
- **Documentation**: Embed live code examples and auto-extract API docs
- **Teaching**: Create code tutorials with explanations
- **Migration**: Move code between projects
- **Templates**: Distribute project templates as markdown
- **Bug Reports**: Include minimal reproducible examples
- **Code Analysis**: Programmatic access to code structure via JSON/CSV output
- **Project Exploration**: Understand unfamiliar codebases quickly

## Building from Source

### Prerequisites

- Rust 1.70 or higher (Rust 2024 edition)

### Build

```bash
cargo build --release
```

### Run Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test copy_single_file_to_markdown_file

# Run tests with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Lint with Clippy
cargo clippy -- -D warnings

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check
```

## How It Works

### Copy & Paste Operations

1. **Copy**: Walks directory tree respecting gitignore → loads file contents → renders to markdown with smart fence detection
2. **Paste**: Parses markdown with pulldown-cmark → detects file paths from format-specific patterns → writes files with conflict handling

### Analysis

1. **Project Detection**: Walks up directory tree to find project markers (Cargo.toml, package.json, etc.)
2. **LSP Server**: Spawns appropriate LSP server as subprocess
3. **JSON-RPC Communication**: Communicates with server over stdin/stdout using JSON-RPC 2.0
4. **Symbol Extraction**: Requests document symbols and hover information
5. **Caching**: Stores results for improved performance on repeated analysis
6. **Formatting**: Renders output in selected format (markdown, JSON, CSV, etc.)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by the need to share code context with AI assistants
- Built with modern Rust 2024 edition best practices
- Uses the excellent `ignore` crate for gitignore support
- LSP integration for accurate, language-aware code analysis

## Support

If you encounter any issues or have questions, please [open an issue](https://github.com/CaddyGlow/quickctx/issues).
