use super::lsp_client::LspClient;
use super::path_types::FilePath;
use super::symbol_index::{SymbolIndex, SymbolLocation};
use super::type_extractor::{TypeContext, TypeReference};
use std::collections::HashMap;
use std::path::PathBuf;

/// A resolved type with its definition location
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedType {
    pub type_name: String,
    pub context: TypeContext,
    pub resolution: TypeResolution,
}

/// Where a type is defined
#[derive(Debug, Clone, PartialEq)]
pub enum TypeResolution {
    /// Type found in analyzed files (local)
    Local {
        file_path: FilePath,
        line: u32,
        kind: String,
    },
    /// Type found via LSP workspace search (external)
    External {
        file_path: Option<FilePath>,
        line: Option<u32>,
    },
    /// Type not found
    Unresolved,
}

/// Resolves type references to their definitions
pub struct TypeResolver<'a> {
    symbol_index: &'a SymbolIndex,
    use_lsp: bool,
}

impl<'a> TypeResolver<'a> {
    pub fn new(symbol_index: &'a SymbolIndex, use_lsp: bool) -> Self {
        Self {
            symbol_index,
            use_lsp,
        }
    }

    /// Resolve a collection of type references
    pub fn resolve_types(
        &self,
        type_refs: &[TypeReference],
        mut lsp_client: Option<&mut LspClient>,
    ) -> Vec<ResolvedType> {
        let mut resolved = Vec::new();
        let mut lsp_cache: HashMap<String, Vec<lsp_types::SymbolInformation>> = HashMap::new();

        for type_ref in type_refs {
            let resolution =
                self.resolve_single_type(type_ref, lsp_client.as_deref_mut(), &mut lsp_cache);

            resolved.push(ResolvedType {
                type_name: type_ref.type_name.clone(),
                context: type_ref.context.clone(),
                resolution,
            });
        }

        resolved
    }

    /// Resolve a single type reference using typeDefinition LSP request
    fn resolve_single_type(
        &self,
        type_ref: &TypeReference,
        lsp_client: Option<&mut LspClient>,
        _lsp_cache: &mut HashMap<String, Vec<lsp_types::SymbolInformation>>,
    ) -> TypeResolution {
        // First, check local symbol index (fast path)
        if let Some(locations) = self.symbol_index.lookup(&type_ref.type_name)
            && let Some(location) = Self::find_best_match(locations, &type_ref.type_name) {
            return TypeResolution::Local {
                file_path: location.file_path.clone(),
                line: location.line_start,
                kind: location.kind.clone(),
            };
        }

        // If not found locally and LSP is enabled, use typeDefinition
        if self.use_lsp
            && let Some(client) = lsp_client {
            // Use the position from the TypeReference to request typeDefinition
            match client.type_definition(&type_ref.uri, type_ref.position) {
                Ok(Some(response)) => {
                    // Extract location from GotoDefinitionResponse
                    if let Some((uri, range)) = Self::extract_first_location(response) {
                        return TypeResolution::External {
                            file_path: uri
                                .to_file_path()
                                .ok()
                                .map(|p| FilePath::from_absolute_unchecked(p)),
                            line: Some(range.start.line),
                        };
                    }
                }
                Ok(None) => {
                    tracing::debug!(
                        "No typeDefinition found for '{}' at {:?}",
                        type_ref.type_name,
                        type_ref.position
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to query typeDefinition for '{}': {}",
                        type_ref.type_name,
                        e
                    );
                }
            }
        }

        TypeResolution::Unresolved
    }

    /// Find the best matching symbol location (prefer type definitions)
    fn find_best_match<'b>(
        locations: &'b [SymbolLocation],
        _type_name: &str,
    ) -> Option<&'b SymbolLocation> {
        // Prefer type definitions over other kinds
        locations
            .iter()
            .find(|loc| {
                matches!(
                    loc.kind.as_str(),
                    "Struct" | "Class" | "Enum" | "Interface" | "TypeAlias"
                )
            })
            .or_else(|| locations.first())
    }

    /// Extract the first location from a GotoDefinitionResponse
    /// GotoDefinitionResponse can be Location, Location[], or LocationLink[]
    fn extract_first_location(
        response: lsp_types::GotoDefinitionResponse,
    ) -> Option<(lsp_types::Url, lsp_types::Range)> {
        use lsp_types::GotoDefinitionResponse;

        match response {
            GotoDefinitionResponse::Scalar(location) => Some((location.uri, location.range)),
            GotoDefinitionResponse::Array(locations) => {
                locations.first().map(|loc| (loc.uri.clone(), loc.range))
            }
            GotoDefinitionResponse::Link(links) => links
                .first()
                .map(|link| (link.target_uri.clone(), link.target_selection_range)),
        }
    }
}

/// Group resolved types by file for easier formatting
pub fn group_by_file(resolved_types: Vec<ResolvedType>) -> HashMap<PathBuf, Vec<ResolvedType>> {
    let mut by_file: HashMap<PathBuf, Vec<ResolvedType>> = HashMap::new();

    // We need to know which file each type came from
    // This requires passing that info through the resolution process
    // For now, we'll just return all types (can be improved later)

    for resolved in resolved_types {
        // Group by resolution file path
        match &resolved.resolution {
            TypeResolution::Local { file_path, .. } => {
                by_file
                    .entry(file_path.clone().into_path_buf())
                    .or_default()
                    .push(resolved);
            }
            TypeResolution::External { file_path, .. } => {
                if let Some(path) = file_path {
                    by_file
                        .entry(path.clone().into_path_buf())
                        .or_default()
                        .push(resolved);
                }
            }
            TypeResolution::Unresolved => {
                // Group unresolved types separately
                by_file
                    .entry(PathBuf::from("__unresolved__"))
                    .or_default()
                    .push(resolved);
            }
        }
    }

    by_file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::SymbolInfo;
    use crate::analyze::symbol_index::SymbolIndex;

    #[test]
    fn test_resolve_local_type() {
        use lsp_types::{Position, Range};

        let symbols = vec![SymbolInfo {
            name: "MyStruct".to_string(),
            kind: lsp_types::SymbolKind::STRUCT,
            detail: None,
            documentation: None,
            range: Range::new(Position::new(10, 0), Position::new(15, 0)),
            selection_range: Range::new(Position::new(10, 0), Position::new(10, 8)),
            children: vec![],
            type_dependencies: None,
        }];

        let file_symbols = vec![(PathBuf::from("/test.rs"), symbols)];
        let index = SymbolIndex::build_from_symbols(&file_symbols);
        let resolver = TypeResolver::new(&index, false);

        let type_refs = vec![TypeReference {
            type_name: "MyStruct".to_string(),
            context: TypeContext::FunctionParameter,
            position: lsp_types::Position {
                line: 0,
                character: 0,
            },
            uri: lsp_types::Url::parse("file:///test.rs").unwrap(),
            char_offset: None,
        }];

        let resolved = resolver.resolve_types(&type_refs, None);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].type_name, "MyStruct");
        assert!(matches!(
            resolved[0].resolution,
            TypeResolution::Local { .. }
        ));
    }

    #[test]
    fn test_resolve_unresolved_type() {
        let index = SymbolIndex::new();
        let resolver = TypeResolver::new(&index, false);

        let type_refs = vec![TypeReference {
            type_name: "UnknownType".to_string(),
            context: TypeContext::FunctionParameter,
            position: lsp_types::Position {
                line: 0,
                character: 0,
            },
            uri: lsp_types::Url::parse("file:///test.rs").unwrap(),
            char_offset: None,
        }];

        let resolved = resolver.resolve_types(&type_refs, None);

        assert_eq!(resolved.len(), 1);
        assert!(matches!(resolved[0].resolution, TypeResolution::Unresolved));
    }
}
