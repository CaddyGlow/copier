use super::SymbolInfo;
use lsp_types::SymbolKind;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Location where a symbol is defined
#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub file_path: PathBuf,
    pub line_start: u32,
    pub line_end: u32,
    pub kind: String,
    pub detail: Option<String>,
}

/// Index of all symbols for quick type lookup
#[derive(Debug, Default)]
pub struct SymbolIndex {
    /// Map from symbol name to all locations where it's defined
    symbols: HashMap<String, Vec<SymbolLocation>>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    /// Build index from collected symbols
    pub fn build_from_symbols(file_symbols: &[(PathBuf, Vec<SymbolInfo>)]) -> Self {
        let mut index = Self::new();

        for (file_path, symbols) in file_symbols {
            index.add_symbols_from_file(file_path, symbols);
        }

        index
    }

    /// Add symbols from a single file
    fn add_symbols_from_file(&mut self, file_path: &Path, symbols: &[SymbolInfo]) {
        for symbol in symbols {
            self.add_symbol(file_path, symbol);

            // Also add child symbols (e.g., struct fields, enum variants)
            for child in &symbol.children {
                self.add_symbol(file_path, child);
            }
        }
    }

    /// Add a single symbol to the index
    fn add_symbol(&mut self, file_path: &Path, symbol: &SymbolInfo) {
        // We're primarily interested in type definitions
        let is_type_definition = matches!(
            symbol.kind,
            lsp_types::SymbolKind::STRUCT
                | lsp_types::SymbolKind::CLASS
                | lsp_types::SymbolKind::ENUM
                | lsp_types::SymbolKind::INTERFACE
                | lsp_types::SymbolKind::TYPE_PARAMETER
        );

        if is_type_definition || should_index_symbol(symbol.kind) {
            let location = SymbolLocation {
                file_path: file_path.to_path_buf(),
                line_start: symbol.range.start.line,
                line_end: symbol.range.end.line,
                kind: symbol_kind_to_string(symbol.kind).to_string(),
                detail: symbol.detail.clone(),
            };

            self.symbols
                .entry(symbol.name.clone())
                .or_default()
                .push(location);
        }
    }

    /// Look up a type by name
    pub fn lookup(&self, type_name: &str) -> Option<&Vec<SymbolLocation>> {
        self.symbols.get(type_name)
    }

    /// Get all indexed symbol names
    pub fn all_names(&self) -> Vec<&String> {
        self.symbols.keys().collect()
    }

    /// Get total number of indexed symbols
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Determine if a symbol should be indexed
fn should_index_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::STRUCT
            | SymbolKind::CLASS
            | SymbolKind::ENUM
            | SymbolKind::INTERFACE
            | SymbolKind::TYPE_PARAMETER
            | SymbolKind::MODULE
            | SymbolKind::NAMESPACE
    )
}

/// Convert SymbolKind to string representation
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_index_basic() {
        let symbols = vec![
            SymbolInfo {
                name: "MyStruct".to_string(),
                kind: "Struct".to_string(),
                line_start: 10,
                line_end: 15,
                detail: None,
                documentation: None,
                children: None,
            },
            SymbolInfo {
                name: "MyEnum".to_string(),
                kind: "Enum".to_string(),
                line_start: 20,
                line_end: 25,
                detail: None,
                documentation: None,
                children: None,
            },
        ];

        let file_symbols = vec![(PathBuf::from("test.rs"), symbols)];
        let index = SymbolIndex::build_from_symbols(&file_symbols);

        assert_eq!(index.len(), 2);
        assert!(index.lookup("MyStruct").is_some());
        assert!(index.lookup("MyEnum").is_some());
        assert!(index.lookup("NonExistent").is_none());
    }

    #[test]
    fn test_symbol_index_with_children() {
        let symbols = vec![SymbolInfo {
            name: "MyStruct".to_string(),
            kind: "Struct".to_string(),
            line_start: 10,
            line_end: 15,
            detail: None,
            documentation: None,
            children: Some(vec![
                SymbolInfo {
                    name: "field1".to_string(),
                    kind: "Field".to_string(),
                    line_start: 11,
                    line_end: 11,
                    detail: Some("String".to_string()),
                    documentation: None,
                    children: None,
                },
                SymbolInfo {
                    name: "field2".to_string(),
                    kind: "Field".to_string(),
                    line_start: 12,
                    line_end: 12,
                    detail: Some("i32".to_string()),
                    documentation: None,
                    children: None,
                },
            ]),
        }];

        let file_symbols = vec![(PathBuf::from("test.rs"), symbols)];
        let index = SymbolIndex::build_from_symbols(&file_symbols);

        // Should index the struct and its fields
        assert!(index.lookup("MyStruct").is_some());
        assert!(index.lookup("field1").is_some());
        assert!(index.lookup("field2").is_some());
    }
}
