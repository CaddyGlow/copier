# Quickctx

A bidirectional file content aggregator and extractor that converts between files and markdown-formatted representations.

## Features

- **Aggregate** multiple files into a single markdown document
- **Extract** files from markdown back to the filesystem
- **Multiple output formats** (simple, comment-style, heading-style)
- **Smart fence detection** - automatically avoids code fence conflicts
- **Gitignore support** - respects `.gitignore` patterns by default
- **Language detection** - automatic syntax highlighting based on file extension
- **Bidirectional** - seamlessly convert back and forth

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

### From Source

```bash
git clone https://github.com/CaddyGlow/quickctx.git
cd quickctx
cargo build --release
```

The binary will be available at `target/release/quickctx`.

## Quick Start

### Aggregate Files

Convert files to markdown:

```bash
# Aggregate specific files
quickctx file1.rs file2.rs

# Aggregate entire directory
quickctx src/

# Use glob patterns
quickctx "src/**/*.rs"

# Output to file
quickctx src/ -o project.md
```

### Extract Files

Convert markdown back to files:

```bash
# Extract from file
quickctx extract project.md

# Extract from stdin
cat project.md | quickctx extract

# Extract to specific directory
quickctx extract project.md -o restored/
```

## Usage

### Aggregation Mode

```bash
quickctx [OPTIONS] <PATHS>...

Options:
  -o, --output <FILE>          Write output to file instead of stdout
  -f, --format <FORMAT>        Output format [default: simple]
                               [possible values: simple, comment, heading]
      --fence <FENCE>          Fence style [default: auto]
                               [possible values: backtick, tilde, auto]
      --no-gitignore           Don't respect .gitignore files
      --ignore-file <FILE>     Use custom ignore file
      --exclude <PATTERN>      Exclude files matching pattern
  -h, --help                   Print help
  -V, --version                Print version
```

### Extraction Mode

```bash
quickctx extract [OPTIONS] <INPUT>

Options:
  -o, --output <DIR>           Output directory [default: .]
      --conflict <ACTION>      How to handle existing files [default: prompt]
                               [possible values: skip, overwrite, prompt]
  -h, --help                   Print help
```

## Output Formats

### Simple Format

```markdown
file.c

```c
int main() {
    return 0;
}
```
```

### Comment Format

```markdown
```c
// src/main.c
int main() {
    return 0;
}
```
```

### Heading Format

```markdown
## `src/main.c`

```c
int main() {
    return 0;
}
```
```

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

[aggregate]
paths = ["src/", "tests/"]
format = "heading"
fence = "backtick"
respect_gitignore = true
exclude = ["*.tmp", "*.bak"]

[extractor]
conflict = "skip"
```

Use it with:

```bash
quickctx --config quickctx.toml src/
```

## Examples

### Example 1: Share Code Snippets

Aggregate your project files to share with others:

```bash
quickctx src/ tests/ -f comment -o share.md
```

### Example 2: AI-Assisted Development

Export your codebase to feed into an AI assistant:

```bash
quickctx "src/**/*.rs" "tests/**/*.rs" --format heading -o context.md
```

### Example 3: Documentation

Extract code examples from documentation:

```bash
quickctx extract API_EXAMPLES.md -o examples/
```

### Example 4: Project Templates

Create project templates as markdown:

```bash
# Create template
quickctx template-project/ -o rust-template.md

# Instantiate template
quickctx extract rust-template.md -o my-new-project/
```

## Use Cases

- **Code Reviews**: Share code context easily
- **AI Assistants**: Provide codebase context to LLMs
- **Documentation**: Embed live code examples
- **Teaching**: Create code tutorials with explanations
- **Migration**: Move code between projects
- **Templates**: Distribute project templates
- **Bug Reports**: Include minimal reproducible examples

## Building from Source

### Prerequisites

- Rust 1.70 or higher

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Run with Clippy

```bash
cargo clippy -- -D warnings
```

### Format Code

```bash
cargo fmt
```

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
- Built with modern Rust best practices
- Uses the excellent `ignore` crate for gitignore support

## Roadmap

- [ ] Binary file detection and handling
- [ ] File metadata preservation
- [ ] Incremental updates
- [ ] Interactive mode
- [ ] Git integration
- [ ] Watch mode
- [ ] Plugin system
- [ ] Syntax highlighting in terminal

## Support

If you encounter any issues or have questions, please [open an issue](https://github.com/CaddyGlow/quickctx/issues).
