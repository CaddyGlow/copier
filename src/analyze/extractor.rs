use crate::analyze::lsp_client::LspClient;
use crate::analyze::type_resolver::ResolvedType;
use crate::error::Result;
use lsp_types::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<SymbolInfo>,
    pub type_dependencies: Option<Vec<ResolvedType>>,
}

/// Extract symbols and their documentation from a file using LSP
pub fn extract_symbols(client: &mut LspClient, uri: &Url) -> Result<Vec<SymbolInfo>> {
    let symbol_response = client.document_symbols(uri)?;

    let mut symbols = Vec::new();

    match symbol_response {
        DocumentSymbolResponse::Flat(symbol_info_vec) => {
            // Handle flat SymbolInformation response
            for symbol in symbol_info_vec {
                let hover = client
                    .hover(uri, symbol.location.range.start)
                    .ok()
                    .flatten();

                symbols.push(SymbolInfo {
                    name: symbol.name,
                    kind: symbol.kind,
                    detail: None,
                    documentation: hover.and_then(extract_hover_docs),
                    range: symbol.location.range,
                    selection_range: symbol.location.range,
                    children: vec![],
                    type_dependencies: None,
                });
            }
        }
        DocumentSymbolResponse::Nested(document_symbols) => {
            // Handle hierarchical DocumentSymbol response - preserve hierarchy
            for symbol in document_symbols {
                symbols.push(convert_document_symbol(client, uri, symbol)?);
            }
        }
    }

    Ok(symbols)
}

/// Convert DocumentSymbol to SymbolInfo, preserving hierarchy
fn convert_document_symbol(
    client: &mut LspClient,
    uri: &Url,
    symbol: DocumentSymbol,
) -> Result<SymbolInfo> {
    let hover = client
        .hover(uri, symbol.selection_range.start)
        .ok()
        .flatten();

    let mut children = Vec::new();
    if let Some(child_symbols) = symbol.children {
        for child in child_symbols {
            children.push(convert_document_symbol(client, uri, child)?);
        }
    }

    Ok(SymbolInfo {
        name: symbol.name,
        kind: symbol.kind,
        detail: symbol.detail,
        documentation: hover.and_then(extract_hover_docs),
        range: symbol.range,
        selection_range: symbol.selection_range,
        children,
        type_dependencies: None,
    })
}

/// Extract documentation from hover response
fn extract_hover_docs(hover: Hover) -> Option<String> {
    match hover.contents {
        HoverContents::Scalar(markup) => Some(markup_to_string(markup)),
        HoverContents::Array(markups) => {
            let docs: Vec<String> = markups.into_iter().map(markup_to_string).collect();
            Some(docs.join("\n\n"))
        }
        HoverContents::Markup(markup) => Some(markup.value),
    }
}

/// Convert MarkedString to plain string
fn markup_to_string(markup: MarkedString) -> String {
    match markup {
        MarkedString::String(s) => s,
        MarkedString::LanguageString(ls) => ls.value,
    }
}

/// Filter symbols by kind (e.g., only functions, only types, etc.)
pub fn filter_symbols_by_kind(symbols: &[SymbolInfo], kinds: &[SymbolKind]) -> Vec<SymbolInfo> {
    symbols
        .iter()
        .filter(|s| kinds.contains(&s.kind))
        .cloned()
        .collect()
}

/// Get only function symbols
pub fn get_functions(symbols: &[SymbolInfo]) -> Vec<SymbolInfo> {
    filter_symbols_by_kind(symbols, &[SymbolKind::FUNCTION, SymbolKind::METHOD])
}

/// Get only type symbols (struct, class, interface, etc.)
pub fn get_types(symbols: &[SymbolInfo]) -> Vec<SymbolInfo> {
    filter_symbols_by_kind(
        symbols,
        &[
            SymbolKind::STRUCT,
            SymbolKind::CLASS,
            SymbolKind::INTERFACE,
            SymbolKind::ENUM,
            SymbolKind::TYPE_PARAMETER,
        ],
    )
}

/// Get only variable/constant symbols
pub fn get_variables(symbols: &[SymbolInfo]) -> Vec<SymbolInfo> {
    filter_symbols_by_kind(symbols, &[SymbolKind::VARIABLE, SymbolKind::CONSTANT])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_functions() {
        let symbols = vec![
            SymbolInfo {
                name: "foo".to_string(),
                kind: SymbolKind::FUNCTION,
                detail: None,
                documentation: None,
                range: Range::default(),
                selection_range: Range::default(),
                children: vec![],
                type_dependencies: None,
            },
            SymbolInfo {
                name: "Bar".to_string(),
                kind: SymbolKind::STRUCT,
                detail: None,
                documentation: None,
                range: Range::default(),
                selection_range: Range::default(),
                children: vec![],
                type_dependencies: None,
            },
        ];

        let functions = get_functions(&symbols);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "foo");
    }

    #[test]
    fn test_filter_types() {
        let symbols = vec![
            SymbolInfo {
                name: "foo".to_string(),
                kind: SymbolKind::FUNCTION,
                detail: None,
                documentation: None,
                range: Range::default(),
                selection_range: Range::default(),
                children: vec![],
                type_dependencies: None,
            },
            SymbolInfo {
                name: "Bar".to_string(),
                kind: SymbolKind::STRUCT,
                detail: None,
                documentation: None,
                range: Range::default(),
                selection_range: Range::default(),
                children: vec![],
                type_dependencies: None,
            },
        ];

        let types = get_types(&symbols);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].name, "Bar");
    }
}
