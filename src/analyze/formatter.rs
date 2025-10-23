use crate::analyze::extractor::{get_functions, get_types, get_variables, SymbolInfo};
use lsp_types::SymbolKind;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Json,
}

pub trait Formatter {
    fn format(&self, symbols: &[SymbolInfo], file_path: &str) -> String;
    fn format_multiple(&self, files: &[(String, Vec<SymbolInfo>)]) -> String;
}

pub struct MarkdownFormatter;
pub struct JsonFormatter;

impl Formatter for MarkdownFormatter {
    fn format(&self, symbols: &[SymbolInfo], file_path: &str) -> String {
        let mut output = String::new();

        // File header
        output.push_str(&format!("# Code Analysis: `{}`\n\n", file_path));

        // Separate symbols by category
        let functions = get_functions(symbols);
        let types = get_types(symbols);
        let variables = get_variables(symbols);

        // Functions
        if !functions.is_empty() {
            output.push_str("## Functions\n\n");
            for func in &functions {
                output.push_str(&format_symbol_markdown(func));
                output.push_str("\n---\n\n");
            }
        }

        // Types
        if !types.is_empty() {
            output.push_str("## Types\n\n");
            for typ in &types {
                output.push_str(&format_symbol_markdown(typ));
                output.push_str("\n---\n\n");
            }
        }

        // Variables & Constants
        if !variables.is_empty() {
            output.push_str("## Variables & Constants\n\n");
            for var in &variables {
                output.push_str(&format_symbol_markdown(var));
                output.push_str("\n---\n\n");
            }
        }

        // Other symbols
        let other: Vec<_> = symbols
            .iter()
            .filter(|s| {
                !functions.contains(s) && !types.contains(s) && !variables.contains(s)
            })
            .collect();

        if !other.is_empty() {
            output.push_str("## Other Symbols\n\n");
            for symbol in other {
                output.push_str(&format_symbol_markdown(symbol));
                output.push_str("\n---\n\n");
            }
        }

        output
    }

    fn format_multiple(&self, files: &[(String, Vec<SymbolInfo>)]) -> String {
        let mut output = String::new();

        output.push_str("# Code Analysis\n\n");
        output.push_str(&format!("Analyzed {} file(s)\n\n", files.len()));
        output.push_str("---\n\n");

        for (file_path, symbols) in files {
            output.push_str(&format!("## File: `{}`\n\n", file_path));
            output.push_str(&self.format(symbols, file_path));
            output.push_str("\n---\n\n");
        }

        output
    }
}

fn format_symbol_markdown(symbol: &SymbolInfo) -> String {
    let mut output = String::new();

    // Symbol name and kind
    output.push_str(&format!("### `{}` ({})\n\n", symbol.name, symbol_kind_to_string(symbol.kind)));

    // Detail (e.g., function signature)
    if let Some(detail) = &symbol.detail {
        output.push_str(&format!("**Signature:** `{}`\n\n", detail));
    }

    // Documentation
    if let Some(docs) = &symbol.documentation {
        output.push_str("**Documentation:**\n\n");
        output.push_str(docs);
        output.push_str("\n\n");
    }

    // Location info
    output.push_str(&format!(
        "**Location:** Line {}-{}\n\n",
        symbol.range.start.line + 1,
        symbol.range.end.line + 1
    ));

    // Fields/Members (children)
    if !symbol.children.is_empty() {
        output.push_str("**Fields:**\n\n");
        for child in &symbol.children {
            let child_detail = child.detail.as_deref().unwrap_or("");
            output.push_str(&format!("- `{}`: {} ({})\n",
                child.name,
                child_detail,
                symbol_kind_to_string(child.kind)
            ));
            if let Some(docs) = &child.documentation {
                output.push_str(&format!("  - {}\n", docs.lines().next().unwrap_or("")));
            }
        }
        output.push_str("\n");
    }

    output
}

fn symbol_kind_to_string(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "File",
        SymbolKind::MODULE => "Module",
        SymbolKind::NAMESPACE => "Namespace",
        SymbolKind::PACKAGE => "Package",
        SymbolKind::CLASS => "Class",
        SymbolKind::METHOD => "Method",
        SymbolKind::PROPERTY => "Property",
        SymbolKind::FIELD => "Field",
        SymbolKind::CONSTRUCTOR => "Constructor",
        SymbolKind::ENUM => "Enum",
        SymbolKind::INTERFACE => "Interface",
        SymbolKind::FUNCTION => "Function",
        SymbolKind::VARIABLE => "Variable",
        SymbolKind::CONSTANT => "Constant",
        SymbolKind::STRING => "String",
        SymbolKind::NUMBER => "Number",
        SymbolKind::BOOLEAN => "Boolean",
        SymbolKind::ARRAY => "Array",
        SymbolKind::OBJECT => "Object",
        SymbolKind::KEY => "Key",
        SymbolKind::NULL => "Null",
        SymbolKind::ENUM_MEMBER => "Enum Member",
        SymbolKind::STRUCT => "Struct",
        SymbolKind::EVENT => "Event",
        SymbolKind::OPERATOR => "Operator",
        SymbolKind::TYPE_PARAMETER => "Type Parameter",
        _ => "Unknown",
    }
}

#[derive(Debug, Serialize)]
struct JsonSymbol {
    name: String,
    kind: String,
    detail: Option<String>,
    documentation: Option<String>,
    line_start: u32,
    line_end: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<JsonSymbol>,
}

impl From<&SymbolInfo> for JsonSymbol {
    fn from(symbol: &SymbolInfo) -> Self {
        Self {
            name: symbol.name.clone(),
            kind: symbol_kind_to_string(symbol.kind).to_string(),
            detail: symbol.detail.clone(),
            documentation: symbol.documentation.clone(),
            line_start: symbol.range.start.line + 1,
            line_end: symbol.range.end.line + 1,
            children: symbol.children.iter().map(JsonSymbol::from).collect(),
        }
    }
}

impl Formatter for JsonFormatter {
    fn format(&self, symbols: &[SymbolInfo], file_path: &str) -> String {
        let json_symbols: Vec<JsonSymbol> = symbols.iter().map(JsonSymbol::from).collect();

        let output = serde_json::json!({
            "file": file_path,
            "symbols": json_symbols
        });

        serde_json::to_string_pretty(&output).unwrap_or_else(|e| {
            format!("{{\"error\": \"Failed to serialize: {}\"}}", e)
        })
    }

    fn format_multiple(&self, files: &[(String, Vec<SymbolInfo>)]) -> String {
        let mut file_outputs = Vec::new();

        for (file_path, symbols) in files {
            let json_symbols: Vec<JsonSymbol> = symbols.iter().map(JsonSymbol::from).collect();
            file_outputs.push(serde_json::json!({
                "file": file_path,
                "symbols": json_symbols
            }));
        }

        let output = serde_json::json!({
            "files": file_outputs
        });

        serde_json::to_string_pretty(&output).unwrap_or_else(|e| {
            format!("{{\"error\": \"Failed to serialize: {}\"}}", e)
        })
    }
}

pub fn get_formatter(format: OutputFormat) -> Box<dyn Formatter> {
    match format {
        OutputFormat::Markdown => Box::new(MarkdownFormatter),
        OutputFormat::Json => Box::new(JsonFormatter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Range;

    fn create_test_symbol(name: &str, kind: SymbolKind) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind,
            detail: Some(format!("fn {}()", name)),
            documentation: Some("Test documentation".to_string()),
            range: Range::default(),
            selection_range: Range::default(),
            children: vec![],
        }
    }

    #[test]
    fn test_markdown_formatter() {
        let symbols = vec![
            create_test_symbol("foo", SymbolKind::FUNCTION),
            create_test_symbol("Bar", SymbolKind::STRUCT),
        ];

        let formatter = MarkdownFormatter;
        let output = formatter.format(&symbols, "src/test.rs");

        assert!(output.contains("Code Analysis"));
        assert!(output.contains("src/test.rs"));
        assert!(output.contains("## Functions"));
        assert!(output.contains("## Types"));
        assert!(output.contains("`foo`"));
        assert!(output.contains("`Bar`"));
    }

    #[test]
    fn test_json_formatter() {
        let symbols = vec![create_test_symbol("foo", SymbolKind::FUNCTION)];

        let formatter = JsonFormatter;
        let output = formatter.format(&symbols, "src/test.rs");

        assert!(output.contains("\"file\""));
        assert!(output.contains("src/test.rs"));
        assert!(output.contains("\"name\": \"foo\""));
        assert!(output.contains("\"kind\": \"Function\""));
        assert!(output.contains("\"documentation\""));
    }
}
