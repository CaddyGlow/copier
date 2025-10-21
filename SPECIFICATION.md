# Copier - File Content Aggregator and Extractor

## Overview

Copier is a bidirectional file content tool that can:
1. **Aggregate**: Read multiple files/folders and output their contents in markdown format
2. **Extract**: Parse markdown-formatted file contents and create the actual files

## Core Features

### 1. File to Markdown Aggregation

#### Input
- Accept a list of file paths and/or directory paths
- Support glob patterns (e.g., `src/**/*.rs`)
- Support reading from stdin, config file, or command-line arguments
- **Respect `.gitignore` files** - automatically skip ignored files when traversing directories

#### Output Formats
The tool should support multiple output rendering styles:

**Format 1: Simple**
```
file.c

```c
int main() {
    return 0;
}
```
```

**Format 2: Path as Comment**
```
```c
// some/path/myfile.c
int main() {
    return 0;
}
```
```

**Format 3: Heading Style**
```
## `file.c`

```c
int main() {
    return 0;
}
```
```

### 2. Smart Fence Detection

The tool must intelligently handle code fence conflicts:

- **Backtick Detection**: Scan file content for existing backtick sequences
- **Adaptive Fencing**:
  - If content contains ` ``` `, use ` ```` ` (4 backticks)
  - If content contains ` ```` `, use ` ````` ` (5 backticks), etc.
  - As alternative, switch to tilde fences: `~~~`, `~~~~`, etc.
- **Language Detection**: Automatically detect language from file extension
  - `.rs` → `rust`
  - `.c` → `c`
  - `.py` → `python`
  - etc.

### 3. Gitignore Support

- **Automatic Detection**: Look for `.gitignore` files in the project root and parent directories
- **Respect Rules**: Skip files/directories matching gitignore patterns
- **Override Option**: Provide flag to disable gitignore (e.g., `--no-gitignore`)
- **Manual Gitignore**: Allow specifying custom gitignore file path
- **Ignore Files**: Support other ignore files (`.ignore`, `.copierignore`)

#### Gitignore Behavior

```bash
# Default: respect .gitignore
copier src/

# Ignore gitignore rules
copier src/ --no-gitignore

# Use custom ignore file
copier src/ --ignore-file .customignore

# Combine with explicit patterns
copier src/ --exclude "*.tmp"
```

### 4. Markdown to Files Extraction (Reverse Operation)

#### Input
- Parse markdown content containing file definitions
- Support all output formats mentioned above
- Handle both stdin and file input

#### Processing
- Extract file paths from various format styles
- Detect code fence delimiters (backticks or tildes)
- Identify language hints (optional)
- Extract file content from code blocks

#### Output
- Create directory structure as needed
- Write files with extracted content
- Preserve file permissions where applicable
- Handle conflicts (overwrite, skip, prompt modes)

### 5. Configuration

Support configuration through:
- Command-line flags
- Configuration file (e.g., `copier.toml`)
- Environment variables

#### Configurable Options
- Output format style (simple, comment, heading)
- Fence style preference (backtick, tilde, auto)
- Include/exclude patterns
- Gitignore behavior (respect, ignore)
- Base path for relative paths
- Overwrite behavior
- Output destination (stdout, file)

## Technical Requirements

### Architecture

```
copier/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library interface and run coordinator
│   ├── cli.rs               # CLI argument definitions (clap)
│   ├── config/
│   │   └── mod.rs           # Configuration loading and merging
│   ├── aggregate/
│   │   └── mod.rs           # File collection and aggregation pipeline
│   ├── extract/
│   │   └── mod.rs           # Markdown parsing and file extraction
│   ├── render/
│   │   └── mod.rs           # Output formatting and smart fence detection
│   ├── fs/
│   │   └── mod.rs           # File system utilities and path handling
│   ├── utils/
│   │   ├── mod.rs           # Shared utilities
│   │   └── language.rs      # Language detection from file extensions
│   ├── telemetry.rs         # Logging setup (tracing)
│   └── error.rs             # Error types and Result alias
├── tests/
│   ├── aggregate_extract.rs # Integration tests
│   ├── config_test.rs       # Config loading tests
│   ├── fs_test.rs           # File system tests
│   ├── integration_test.rs  # End-to-end tests
│   ├── render_test.rs       # Rendering tests
│   └── utils_test.rs        # Utility tests
├── Cargo.toml
└── README.md
```

### Rust Best Practices

1. **Error Handling**
   - Use `Result<T, E>` for fallible operations
   - Define custom error types using `thiserror`
   - Provide helpful error messages

2. **Code Organization**
   - Separate library (`lib.rs`) from binary (`main.rs`)
   - Use modules for logical separation
   - Keep functions small and focused

3. **Dependencies**
   - `clap` - Command-line argument parsing (with derive feature)
   - `serde` - Serialization/deserialization
   - `toml` - Configuration file parsing
   - `camino` - UTF-8 path handling
   - `glob` - Basic glob pattern matching
   - `globwalk` - Walking directory trees with glob patterns
   - `globset` - Efficient glob matching
   - `ignore` - Gitignore-aware directory traversal (replaces walkdir)
   - `pulldown-cmark` - Markdown parsing
   - `dialoguer` - Interactive prompts for conflict resolution
   - `thiserror` - Error type definitions
   - `anyhow` - Error context and handling
   - `tracing` + `tracing-subscriber` - Structured logging
   - `once_cell` - Lazy static initialization
   - `strum` - Enum utilities (Display, EnumString)
   - `serde_path_to_error` - Better error messages for config parsing

4. **Testing**
   - Unit tests for each module
   - Integration tests for end-to-end workflows
   - Property-based tests for fence detection
   - Test fixtures for real-world scenarios
   - Tests for gitignore behavior

5. **Documentation**
   - Doc comments for public APIs
   - Examples in doc comments
   - README with usage examples
   - Contributing guidelines

## CLI Interface

### Aggregation Mode

```bash
# Basic usage (respects .gitignore by default)
copier <file1> <file2> <dir/>

# With glob patterns
copier "src/**/*.rs"

# Specify output format
copier --format heading src/

# Output to file
copier src/ -o output.md

# Use tilde fences
copier --fence tilde src/

# Ignore .gitignore rules
copier src/ --no-gitignore

# Use custom ignore file
copier src/ --ignore-file .customignore

# Exclude additional patterns
copier src/ --exclude "*.tmp" --exclude "*.bak"

# From config file
copier --config copier.toml
```

### Extraction Mode

```bash
# Extract from file
copier extract input.md

# Extract from stdin
cat files.md | copier extract

# Specify output directory
copier extract input.md -o target/

# Conflict handling
copier extract input.md --conflict skip|overwrite|prompt
```

## Usage Examples

### Example 1: Aggregate Rust Project

```bash
copier "src/**/*.rs" --format comment -o project.md
```

Output:
```markdown
```rust
// src/main.rs
fn main() {
    println!("Hello, world!");
}
```

```rust
// src/lib.rs
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```
```

### Example 2: Extract Files

```bash
copier extract project.md -o restored/
```

Creates:
```
restored/
├── src/
│   ├── main.rs
│   └── lib.rs
```

### Example 3: Smart Fence Detection

Input file contains:
````markdown
# Example

```rust
let code = "test";
```
````

Output uses 4 backticks:
`````markdown
example.md

````markdown
# Example

```rust
let code = "test";
```
````
`````

### Example 4: Gitignore Support

Given `.gitignore`:
```
target/
*.log
.env
```

```bash
# This will skip target/, *.log, and .env files
copier src/ target/ --format simple

# This will include everything
copier src/ target/ --no-gitignore
```

## Success Criteria

1. Successfully aggregate files into markdown format
2. Correctly detect and avoid fence conflicts
3. Parse markdown and recreate file structure
4. Properly respect .gitignore patterns
5. Handle edge cases (empty files, binary files, special characters)
6. Provide clear error messages
7. Achieve >90% test coverage
8. Zero unsafe code (unless absolutely necessary)
9. Pass `clippy` with no warnings
10. Format code with `rustfmt`
11. Comprehensive documentation

## Future Enhancements

- Support for file metadata (permissions, timestamps)
- Incremental updates (only changed files)
- Compression for large outputs
- Syntax highlighting in terminal
- Interactive mode with file selection
- Git integration (aggregate changed files)
- Watch mode (auto-update on file changes)
- Plugin system for custom formatters
- Support for `.dockerignore`, `.npmignore`, etc.
- Dry-run mode for extraction
