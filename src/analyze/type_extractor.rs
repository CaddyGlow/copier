use super::project_root::ProjectType;
use super::SymbolInfo;
use regex::Regex;
use std::collections::HashSet;

/// A type reference extracted from a symbol
#[derive(Debug, Clone, PartialEq)]
pub struct TypeReference {
    pub type_name: String,
    pub context: TypeContext,
    pub position: lsp_types::Position,  // Position in the source file where the type is referenced
    pub uri: lsp_types::Url,            // URI of the file containing the reference
    pub char_offset: Option<u32>,       // Character offset within the type annotation (for generics)
}

/// Context where a type is used
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeContext {
    FunctionParameter,
    FunctionReturn,
    StructField,
    TypeAlias,
    TraitBound,
}

/// Extracts type references from symbols
pub struct TypeExtractor {
    project_type: ProjectType,
    builtin_types: HashSet<String>,
}

impl TypeExtractor {
    pub fn new(project_type: ProjectType) -> Self {
        let builtin_types = Self::get_builtin_types(project_type);
        Self {
            project_type,
            builtin_types,
        }
    }

    /// Extract all type references from a symbol
    /// Uses documentSymbol children for parameters and calculates positions for types
    pub fn extract_types(&self, symbol: &SymbolInfo, uri: &lsp_types::Url) -> Vec<TypeReference> {
        use lsp_types::SymbolKind;

        let mut types = Vec::new();

        // Extract based on symbol kind
        match symbol.kind {
            SymbolKind::FUNCTION | SymbolKind::METHOD => {
                // Log what children we have for debugging
                tracing::debug!("Function '{}' detail: {:?}, doc: {:?}", symbol.name, symbol.detail, symbol.documentation);
                if !symbol.children.is_empty() {
                    tracing::debug!("Function '{}' has {} children", symbol.name, symbol.children.len());
                    for child in &symbol.children {
                        tracing::debug!("  Child: {} (kind: {:?}, detail: {:?}, doc: {:?})",
                            child.name, child.kind, child.detail, child.documentation.as_ref().map(|d| &d[..d.len().min(100)]));
                    }
                }

                // Extract from parameter children (use documentSymbol children)
                // Different LSPs use different SymbolKinds for parameters
                for child in &symbol.children {
                    // Check if this is a parameter (not a nested function/class)
                    let is_likely_parameter = matches!(
                        child.kind,
                        SymbolKind::VARIABLE | SymbolKind::CONSTANT
                    ) && !matches!(child.kind, SymbolKind::FUNCTION | SymbolKind::METHOD | SymbolKind::CLASS);

                    if !is_likely_parameter {
                        continue;
                    }

                    // Try to get type from detail field first (Rust, TypeScript)
                    let type_from_detail = child.detail.as_ref()
                        .filter(|d| !d.is_empty())
                        .map(|d| d.to_string());

                    // If detail is empty, try to extract from hover/documentation (Python)
                    let type_source = type_from_detail.or_else(|| {
                        child.documentation.as_ref().and_then(|doc| {
                            // Python LSPs put type info in hover like: "(parameter) name: Type"
                            self.extract_type_from_hover_docs(doc)
                        })
                    });

                    if let Some(type_str) = type_source {
                        // Extract type names and use child's position
                        for type_name in self.extract_type_names(&type_str) {
                            tracing::debug!("  Extracted parameter type '{}' from {}", type_name,
                                if child.detail.is_some() && !child.detail.as_ref().unwrap().is_empty() { "detail" } else { "hover" });
                            types.push(TypeReference {
                                type_name,
                                context: TypeContext::FunctionParameter,
                                position: child.selection_range.start,
                                uri: uri.clone(),
                                char_offset: None, // TODO: calculate offset for function parameters
                            });
                        }
                    }
                }

                // Extract return type from detail string if available (Rust, TypeScript)
                // Or from documentation/hover (Python)
                let return_type_source = symbol.detail.clone()
                    .or_else(|| {
                        symbol.documentation.as_ref().and_then(|doc| {
                            // Python hover format: "(function) def name(...) -> ReturnType"
                            self.extract_return_type_from_hover_docs(doc)
                        })
                    });

                if let Some(signature) = return_type_source {
                    types.extend(self.extract_return_type_from_signature(&signature, &symbol.selection_range, uri));
                }
            }
            SymbolKind::STRUCT | SymbolKind::CLASS => {
                // Extract from field children
                for child in &symbol.children {
                    if matches!(child.kind, SymbolKind::FIELD | SymbolKind::PROPERTY) {
                        if let Some(detail) = &child.detail {
                            // Calculate where the type annotation starts (after field name and `: `)
                            let type_annotation_start = child.selection_range.end.character + 2;

                            for (type_name, offset) in self.extract_type_names_with_offsets(detail) {
                                // Position points to the type name within the annotation
                                let type_position = lsp_types::Position {
                                    line: child.selection_range.start.line,
                                    character: type_annotation_start + offset as u32,
                                };

                                types.push(TypeReference {
                                    type_name,
                                    context: TypeContext::StructField,
                                    position: type_position,
                                    uri: uri.clone(),
                                    char_offset: Some(offset as u32),
                                });
                            }
                        }
                    }
                }
            }
            SymbolKind::FIELD | SymbolKind::PROPERTY => {
                if let Some(detail) = &symbol.detail {
                    // Calculate where the type annotation starts (after field name and `: `)
                    let type_annotation_start = symbol.selection_range.end.character + 2;

                    for (type_name, offset) in self.extract_type_names_with_offsets(detail) {
                        // Position points to the type name within the annotation
                        let type_position = lsp_types::Position {
                            line: symbol.selection_range.start.line,
                            character: type_annotation_start + offset as u32,
                        };

                        types.push(TypeReference {
                            type_name,
                            context: TypeContext::StructField,
                            position: type_position,
                            uri: uri.clone(),
                            char_offset: Some(offset as u32),
                        });
                    }
                }
            }
            SymbolKind::TYPE_PARAMETER => {
                if let Some(detail) = &symbol.detail {
                    for type_name in self.extract_type_names(detail) {
                        types.push(TypeReference {
                            type_name,
                            context: TypeContext::TypeAlias,
                            position: symbol.selection_range.start,
                            uri: uri.clone(),
                            char_offset: None,
                        });
                    }
                }
            }
            _ => {}
        }

        // Filter out built-in types
        types
            .into_iter()
            .filter(|t| !self.is_builtin(&t.type_name))
            .collect()
    }

    /// Extract return type from function signature
    /// Uses the function's range to estimate the position of the return type
    fn extract_return_type_from_signature(&self, detail: &str, range: &lsp_types::Range, uri: &lsp_types::Url) -> Vec<TypeReference> {
        let mut types = Vec::new();

        // Extract return type based on language
        let return_type_str = match self.project_type {
            ProjectType::Rust => {
                // Rust: fn name(...) -> ReturnType
                if let Some(arrow_pos) = detail.find("->") {
                    let return_type = &detail[arrow_pos + 2..].trim();
                    // Remove trailing 'where' clause if present
                    if let Some(where_pos) = return_type.find("where") {
                        return_type[..where_pos].trim()
                    } else {
                        return_type
                    }
                } else {
                    ""
                }
            }
            ProjectType::TypeScript | ProjectType::JavaScript => {
                // TypeScript: function name(...): ReturnType or (...) => ReturnType
                if let Some(colon_pos) = detail.rfind("):") {
                    &detail[colon_pos + 2..].trim()
                } else if let Some(arrow_pos) = detail.rfind("=>") {
                    &detail[arrow_pos + 2..].trim()
                } else {
                    ""
                }
            }
            ProjectType::Python => {
                // Python: def name(...) -> ReturnType
                if let Some(arrow_pos) = detail.find("->") {
                    &detail[arrow_pos + 2..].trim()
                } else {
                    ""
                }
            }
            ProjectType::Go => {
                // Go: func name(...) ReturnType
                if let Some(close_paren) = detail.find(')') {
                    &detail[close_paren + 1..].trim()
                } else {
                    ""
                }
            }
            ProjectType::Unknown => ""
        };

        if !return_type_str.is_empty() {
            for type_name in self.extract_type_names(return_type_str) {
                // For return types, we use the end of the function's selection range
                // This is an approximation - ideally we'd calculate the exact position
                types.push(TypeReference {
                    type_name,
                    context: TypeContext::FunctionReturn,
                    position: range.end,
                    uri: uri.clone(),
                    char_offset: None,
                });
            }
        }

        types
    }

    /// Extract type from hover documentation for Python parameters
    /// Python LSP format: "(parameter) name: Type" or "(parameter) name: Type\n```"
    fn extract_type_from_hover_docs(&self, hover_docs: &str) -> Option<String> {
        // Look for pattern: "(parameter) name: Type"
        if let Some(param_start) = hover_docs.find("(parameter)") {
            let after_param = &hover_docs[param_start + "(parameter)".len()..];
            // Find the colon
            if let Some(colon_pos) = after_param.find(':') {
                let after_colon = &after_param[colon_pos + 1..].trim_start();
                // Extract until newline or backticks
                let end_pos = after_colon.find('\n')
                    .or_else(|| after_colon.find('`'))
                    .unwrap_or(after_colon.len());
                let type_str = after_colon[..end_pos].trim();
                if !type_str.is_empty() && type_str != "Unknown" {
                    return Some(type_str.to_string());
                }
            }
        }
        None
    }

    /// Extract function signature from hover documentation for return type extraction
    /// Python LSP format: "(function) def name(...) -> ReturnType"
    fn extract_return_type_from_hover_docs(&self, hover_docs: &str) -> Option<String> {
        // Look for pattern: "(function) def name(...) -> ReturnType"
        if hover_docs.contains("(function)") || hover_docs.contains("(method)") {
            // Return the whole line containing the function signature
            // The extract_return_type_from_signature will parse it
            if let Some(def_start) = hover_docs.find("def ") {
                let line_end = hover_docs[def_start..].find('\n').unwrap_or(hover_docs.len() - def_start);
                return Some(hover_docs[def_start..def_start + line_end].trim().to_string());
            }
        }
        None
    }

    /// Extract type names from a type expression with their character offsets
    /// Handles generics, qualified paths, etc.
    /// Returns (type_name, char_offset) tuples
    fn extract_type_names_with_offsets(&self, type_expr: &str) -> Vec<(String, usize)> {
        let mut types = Vec::new();

        // Match identifiers and track their positions in the ORIGINAL string
        let re = match self.project_type {
            ProjectType::Python => {
                // Python: match lowercase identifiers and dotted paths (e.g., typing.List)
                Regex::new(r"([a-z_][a-zA-Z0-9_]*(?:\.[a-zA-Z][a-zA-Z0-9_]*)*|[A-Z][a-zA-Z0-9_]*)").unwrap()
            }
            _ => {
                // Rust/TypeScript/Go: match uppercase identifiers and :: paths
                Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*(?:::[a-zA-Z][a-zA-Z0-9_]*)*)").unwrap()
            }
        };

        for cap in re.captures_iter(type_expr) {
            if let Some(type_match) = cap.get(1) {
                let name = type_match.as_str();
                let offset = type_match.start();

                // Extract last component if it's a qualified path
                let simple_name = if name.contains("::") {
                    name.split("::").last().unwrap_or(name)
                } else if name.contains('.') {
                    // Python dotted paths
                    name.split('.').last().unwrap_or(name)
                } else {
                    name
                };

                if !simple_name.is_empty() {
                    types.push((simple_name.to_string(), offset));
                }
            }
        }

        tracing::debug!("Extracted type names with offsets from '{}': {:?}", type_expr, types);
        types
    }

    /// Extract type names from a type expression (without offsets, for backwards compatibility)
    fn extract_type_names(&self, type_expr: &str) -> Vec<String> {
        self.extract_type_names_with_offsets(type_expr)
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    /// Check if a type is a built-in
    fn is_builtin(&self, type_name: &str) -> bool {
        self.builtin_types.contains(type_name)
    }

    /// Get built-in types for a project type
    fn get_builtin_types(project_type: ProjectType) -> HashSet<String> {
        let mut types = HashSet::new();

        match project_type {
            ProjectType::Rust => {
                // Rust primitives and common std types
                types.extend(vec![
                    "bool", "char", "str", "String", "i8", "i16", "i32", "i64", "i128", "isize",
                    "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "Vec", "Option",
                    "Result", "Box", "Rc", "Arc", "RefCell", "Cell", "Mutex", "RwLock",
                    "HashMap", "HashSet", "BTreeMap", "BTreeSet", "Path", "PathBuf",
                ]);
            }
            ProjectType::TypeScript | ProjectType::JavaScript => {
                types.extend(vec![
                    "string",
                    "number",
                    "boolean",
                    "any",
                    "void",
                    "never",
                    "unknown",
                    "null",
                    "undefined",
                    "String",
                    "Number",
                    "Boolean",
                    "Array",
                    "Object",
                    "Function",
                    "Promise",
                    "Map",
                    "Set",
                    "Date",
                    "RegExp",
                ]);
            }
            ProjectType::Python => {
                types.extend(vec![
                    "str",
                    "int",
                    "float",
                    "bool",
                    "list",
                    "dict",
                    "tuple",
                    "set",
                    "frozenset",
                    "bytes",
                    "bytearray",
                    "List",
                    "Dict",
                    "Tuple",
                    "Set",
                    "FrozenSet",
                    "Optional",
                    "Union",
                    "Any",
                    "Callable",
                ]);
            }
            ProjectType::Go => {
                types.extend(vec![
                    "bool", "byte", "rune", "int", "int8", "int16", "int32", "int64", "uint",
                    "uint8", "uint16", "uint32", "uint64", "float32", "float64", "complex64",
                    "complex128", "string", "error",
                ]);
            }
            ProjectType::Unknown => {}
        }

        types.into_iter().map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_filtering() {
        let extractor = TypeExtractor::new(ProjectType::Rust);

        assert!(extractor.is_builtin("String"));
        assert!(extractor.is_builtin("Vec"));
        assert!(extractor.is_builtin("Result"));
        assert!(!extractor.is_builtin("MyCustomType"));
    }

    #[test]
    fn test_extract_type_names_with_generics() {
        let extractor = TypeExtractor::new(ProjectType::Rust);

        let types = extractor.extract_type_names("Option<CustomType>");
        // Should extract both Option (filtered as builtin) and CustomType
        assert!(types.contains(&"CustomType".to_string()));
    }
}
