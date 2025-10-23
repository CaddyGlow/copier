# copier-analyze

A companion tool to `copier` that uses Language Server Protocol (LSP) to extract code symbols, documentation, and type information from source files.

## Features

- **Auto-detection**: Automatically detects project root and type (Rust, Python, TypeScript, Go)
- **LSP Integration**: Uses existing LSP servers (rust-analyzer, pylsp, etc.) for accurate analysis
- **Symbol Extraction**: Extracts functions, methods, types, variables, and global symbols
- **Hierarchical Structure**: Preserves struct/enum fields as nested children
- **Multiple Files**: Analyze multiple files in a single invocation
- **Documentation**: Retrieves doc comments and hover information via LSP
- **Multiple Formats**: Output as Markdown (human-readable) or JSON (programmatic)
- **Lightweight**: Custom JSON-RPC implementation with minimal dependencies

## Installation

```bash
cargo install copier
```

This will install both `copier` and `copier-analyze` binaries.

## Usage

### Basic Usage

```bash
# Analyze a single file (auto-detects project root and LSP server)
copier-analyze src/main.rs

# Analyze multiple files at once
copier-analyze src/cli.rs src/lib.rs src/main.rs

# Output to file
copier-analyze src/lib.rs -o analysis.md

# JSON format for programmatic use
copier-analyze src/main.rs --format json

# Multiple files with JSON output
copier-analyze src/*.rs --format json -o analysis.json
```

### Advanced Options

```bash
# Use custom config file
copier-analyze src/main.rs --config my-config.toml

# Override project root
copier-analyze src/main.rs --project-root /path/to/project

# Override LSP server (highest priority)
copier-analyze src/main.rs --lsp-server rust-analyzer

# Override with custom path and arguments
copier-analyze src/main.rs --lsp-server "~/.local/share/nvim/mason/bin/rust-analyzer --verbose"

# Verbose logging
copier-analyze src/main.rs -vv
```

## Requirements

You need the appropriate LSP server installed for your language:

- **Rust**: `rust-analyzer` (usually installed with rustup)
- **Python**: `pylsp` or `pyright-langserver`
- **TypeScript/JavaScript**: `typescript-language-server`
- **Go**: `gopls`

## Configuration

You can customize LSP server paths and commands via `copier.toml`:

```toml
[analyze]
# Optional: default output format
format = "markdown"

# Additional paths to search for LSP server binaries
# These paths will be prepended to PATH when spawning LSP servers
# Supports tilde expansion (e.g., ~/.local/bin)
bin_paths = [
    "~/.local/share/nvim/mason/bin",  # Mason LSP servers
    "~/mycode/.venv/bin",              # Python virtual environment
]

# LSP server commands/paths by language
[analyze.lsp_servers]
rust = "rust-analyzer"
python = "pyright-langserver --stdio"
typescript = "typescript-language-server --stdio"
javascript = "typescript-language-server --stdio"
go = "gopls"
```

### Binary Search Path (`bin_paths`)

The `bin_paths` configuration allows you to extend the PATH when searching for LSP server binaries. This is especially useful if you have LSP servers installed in non-standard locations, such as:

- **Mason** (neovim package manager): `~/.local/share/nvim/mason/bin`
- **Python virtual environments**: `~/myproject/.venv/bin`
- **Custom installations**: `~/.local/bin`, `/opt/lsp-servers/bin`

Features:
- **Tilde expansion**: `~/` is automatically expanded to your home directory
- **Prepended to PATH**: These paths are added before the existing PATH
- **Multiple paths**: Add as many directories as needed

Example:
```toml
[analyze]
bin_paths = [
    "~/.local/share/nvim/mason/bin",
    "~/projects/myapp/.venv/bin",
    "/opt/custom-lsp/bin",
]
```

This allows you to use simple command names in `lsp_servers` configuration:
```toml
[analyze.lsp_servers]
python = "pyright-langserver --stdio"  # Will be found in Mason's bin directory
```

### Configuration Priority

Settings are applied in this order (highest to lowest priority):

1. **CLI flag**: `--lsp-server` overrides everything
2. **Config file**: `[analyze.lsp_servers]` in `copier.toml`
3. **Built-in defaults**: Fallback LSP server commands

### Example with Mason LSP Servers

If you have LSP servers installed via Mason (neovim package manager), you can either:

**Option 1: Use `bin_paths` (Recommended)**
```toml
[analyze]
bin_paths = ["~/.local/share/nvim/mason/bin"]

[analyze.lsp_servers]
rust = "rust-analyzer"
python = "pyright-langserver --stdio"
go = "gopls"
```

**Option 2: Use full paths in `lsp_servers`**
```toml
[analyze.lsp_servers]
rust = "~/.local/share/nvim/mason/bin/rust-analyzer"
python = "~/.local/share/nvim/mason/bin/pyright-langserver --stdio"
go = "~/.local/share/nvim/mason/bin/gopls"
```

## How It Works

1. **Project Detection**: Walks up the directory tree to find project markers (Cargo.toml, package.json, etc.)
2. **LSP Server**: Spawns the appropriate LSP server as a subprocess
3. **JSON-RPC Communication**: Communicates with the server over stdin/stdout using JSON-RPC 2.0
4. **Symbol Extraction**: Requests document symbols and hover information
5. **Formatting**: Renders output as Markdown or JSON

## Example Output

### Markdown Format

```markdown
# Code Analysis: `src/analyze/lsp_client.rs`

## Types

### `LspClient` (Struct)

**Location:** Line 9-14

**Fields:**

- `transport`: JsonRpcTransport (Field)
- `root_uri`: Url (Field)
- `project_type`: ProjectType (Field)
- `initialized`: bool (Field)

---

## Functions

### `new` (Function)

**Signature:** `fn(server_cmd: &str, args: &[String], root_path: &Path, project_type: ProjectType) -> Result<Self>`

**Location:** Line 17-68

---
```

### JSON Format

```json
{
  "file": "src/analyze/lsp_client.rs",
  "symbols": [
    {
      "name": "LspClient",
      "kind": "Struct",
      "detail": null,
      "documentation": null,
      "line_start": 9,
      "line_end": 14,
      "children": [
        {
          "name": "transport",
          "kind": "Field",
          "detail": "JsonRpcTransport",
          "documentation": null,
          "line_start": 10,
          "line_end": 10
        },
        {
          "name": "root_uri",
          "kind": "Field",
          "detail": "Url",
          "documentation": null,
          "line_start": 11,
          "line_end": 11
        }
      ]
    }
  ]
}
```

## Implementation Details

See [PLAN.md](PLAN.md) for the complete implementation plan and architecture details.

### Key Components

- **JSON-RPC Transport** (`src/analyze/jsonrpc.rs`): Custom implementation of JSON-RPC 2.0 over stdio
- **LSP Client** (`src/analyze/lsp_client.rs`): High-level LSP protocol operations
- **Project Detection** (`src/analyze/project_root.rs`): Automatic project root and type detection
- **Symbol Extraction** (`src/analyze/extractor.rs`): Combines LSP responses to build symbol information
- **Formatters** (`src/analyze/formatter.rs`): Markdown and JSON output renderers

## Use Cases

- **AI Context**: Generate comprehensive code context for AI assistants
- **Documentation**: Automatically extract API documentation
- **Code Analysis**: Programmatic access to code structure via JSON output
- **Code Review**: Quick overview of functions and types in a file
- **Project Exploration**: Understand unfamiliar codebases

## Limitations

- Requires LSP server to be installed for the target language
- Performance depends on LSP server initialization time
- Some LSP servers may have different capabilities

## License

MIT
