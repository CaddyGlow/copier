use crate::analyze::ProjectType;
use crate::analyze::extractor::{SymbolInfo, get_functions, get_types, get_variables};
use crate::analyze::path_types::RelativePath;
use crate::analyze::type_resolver::{ResolvedType, TypeResolution};
use lsp_types::SymbolKind;
use serde::Serialize;

/// A file with its path and associated symbols
type FileSymbols = (String, Vec<SymbolInfo>);

/// A project with its name, type, and files
type ProjectSymbols = (String, ProjectType, Vec<FileSymbols>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Json,
}

/// Diagnostics for a single file
#[derive(Debug, Clone)]
pub struct FileDiagnostics {
    pub file_path: RelativePath,
    pub diagnostics: Vec<lsp_types::Diagnostic>,
}

/// Diagnostics for a project
#[derive(Debug, Clone)]
pub struct ProjectDiagnostics {
    pub project_name: String,
    pub project_type: ProjectType,
    pub files: Vec<FileDiagnostics>,
}

/// Type dependencies for a single file
#[derive(Debug, Clone)]
pub struct FileTypeDependencies {
    pub file_path: RelativePath,
    pub types: Vec<ResolvedType>,
}

/// Type dependencies for a project
#[derive(Debug, Clone)]
pub struct ProjectTypeDependencies {
    pub project_name: String,
    pub project_type: ProjectType,
    pub files: Vec<FileTypeDependencies>,
}

pub trait Formatter {
    fn format(&self, symbols: &[SymbolInfo], file_path: &str) -> String;
    fn format_multiple(&self, files: &[FileSymbols]) -> String;
    fn format_by_projects(&self, projects: &[ProjectSymbols]) -> String;
    fn format_diagnostics(&self, projects: &[ProjectDiagnostics]) -> String;
    fn format_type_dependencies(&self, projects: &[ProjectTypeDependencies]) -> String;
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
            .filter(|s| !functions.contains(s) && !types.contains(s) && !variables.contains(s))
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

    fn format_by_projects(
        &self,
        projects: &[(String, ProjectType, Vec<(String, Vec<SymbolInfo>)>)],
    ) -> String {
        let mut output = String::new();

        // Header with project summary
        output.push_str("# Code Analysis\n\n");

        let total_files: usize = projects.iter().map(|(_, _, files)| files.len()).sum();
        output.push_str(&format!(
            "Analyzed {} file(s) across {} project(s)\n\n",
            total_files,
            projects.len()
        ));

        // List all projects
        output.push_str("**Projects:**\n\n");
        for (project_name, project_type, files) in projects {
            output.push_str(&format!(
                "- **{}** ({:?}): {} file(s)\n",
                project_name,
                project_type,
                files.len()
            ));
        }
        output.push_str("\n---\n\n");

        // Detailed analysis per project
        for (project_name, project_type, files) in projects {
            output.push_str(&format!(
                "## Project: {} ({:?})\n\n",
                project_name, project_type
            ));

            for (file_path, symbols) in files {
                output.push_str(&format!("### File: `{}`\n\n", file_path));
                output.push_str(&self.format(symbols, file_path));
                output.push('\n');
            }

            output.push_str("---\n\n");
        }

        output
    }

    fn format_diagnostics(&self, projects: &[ProjectDiagnostics]) -> String {
        let mut output = String::new();

        // Header
        output.push_str("# Diagnostics Report\n\n");

        // Count totals
        let total_files: usize = projects.iter().map(|p| p.files.len()).sum();
        let mut total_errors = 0;
        let mut total_warnings = 0;
        let mut total_info = 0;
        let mut total_hint = 0;

        for project in projects {
            for file in &project.files {
                for diag in &file.diagnostics {
                    match diag.severity {
                        Some(lsp_types::DiagnosticSeverity::ERROR) => total_errors += 1,
                        Some(lsp_types::DiagnosticSeverity::WARNING) => total_warnings += 1,
                        Some(lsp_types::DiagnosticSeverity::INFORMATION) => total_info += 1,
                        Some(lsp_types::DiagnosticSeverity::HINT) => total_hint += 1,
                        _ => {}
                    }
                }
            }
        }

        output.push_str(&format!(
            "Analyzed {} file(s) across {} project(s)\n\n",
            total_files,
            projects.len()
        ));
        output.push_str("**Summary:**\n\n");
        output.push_str(&format!("- Errors: {}\n", total_errors));
        output.push_str(&format!("- Warnings: {}\n", total_warnings));
        output.push_str(&format!("- Info: {}\n", total_info));
        output.push_str(&format!("- Hints: {}\n\n", total_hint));
        output.push_str("---\n\n");

        // Detailed diagnostics per project
        for project in projects {
            output.push_str(&format!(
                "## Project: {} ({:?})\n\n",
                project.project_name, project.project_type
            ));

            for file in &project.files {
                if file.diagnostics.is_empty() {
                    continue;
                }

                // Count diagnostics by severity for this file
                let file_errors = file
                    .diagnostics
                    .iter()
                    .filter(|d| matches!(d.severity, Some(lsp_types::DiagnosticSeverity::ERROR)))
                    .count();
                let file_warnings = file
                    .diagnostics
                    .iter()
                    .filter(|d| matches!(d.severity, Some(lsp_types::DiagnosticSeverity::WARNING)))
                    .count();

                output.push_str(&format!("### File: `{}`\n\n", file.file_path));
                output.push_str(&format!(
                    "**{} error(s), {} warning(s)**\n\n",
                    file_errors, file_warnings
                ));

                // Group by severity
                let errors: Vec<_> = file
                    .diagnostics
                    .iter()
                    .filter(|d| matches!(d.severity, Some(lsp_types::DiagnosticSeverity::ERROR)))
                    .collect();
                let warnings: Vec<_> = file
                    .diagnostics
                    .iter()
                    .filter(|d| matches!(d.severity, Some(lsp_types::DiagnosticSeverity::WARNING)))
                    .collect();
                let info: Vec<_> = file
                    .diagnostics
                    .iter()
                    .filter(|d| {
                        matches!(d.severity, Some(lsp_types::DiagnosticSeverity::INFORMATION))
                    })
                    .collect();
                let hints: Vec<_> = file
                    .diagnostics
                    .iter()
                    .filter(|d| matches!(d.severity, Some(lsp_types::DiagnosticSeverity::HINT)))
                    .collect();

                if !errors.is_empty() {
                    output.push_str("#### Errors\n\n");
                    for diag in errors {
                        output.push_str(&format_diagnostic(diag));
                    }
                }

                if !warnings.is_empty() {
                    output.push_str("#### Warnings\n\n");
                    for diag in warnings {
                        output.push_str(&format_diagnostic(diag));
                    }
                }

                if !info.is_empty() {
                    output.push_str("#### Information\n\n");
                    for diag in info {
                        output.push_str(&format_diagnostic(diag));
                    }
                }

                if !hints.is_empty() {
                    output.push_str("#### Hints\n\n");
                    for diag in hints {
                        output.push_str(&format_diagnostic(diag));
                    }
                }

                output.push('\n');
            }

            output.push_str("---\n\n");
        }

        output
    }

    fn format_type_dependencies(&self, projects: &[ProjectTypeDependencies]) -> String {
        use crate::analyze::type_extractor::TypeContext;

        let mut output = String::new();

        // Header
        output.push_str("# Type Dependencies Report\n\n");

        // Count totals
        let total_files: usize = projects.iter().map(|p| p.files.len()).sum();
        let total_types: usize = projects
            .iter()
            .flat_map(|p| &p.files)
            .map(|f| f.types.len())
            .sum();

        let local_types: usize = projects
            .iter()
            .flat_map(|p| &p.files)
            .flat_map(|f| &f.types)
            .filter(|t| matches!(t.resolution, TypeResolution::Local { .. }))
            .count();

        let external_types: usize = projects
            .iter()
            .flat_map(|p| &p.files)
            .flat_map(|f| &f.types)
            .filter(|t| matches!(t.resolution, TypeResolution::External { .. }))
            .count();

        let unresolved_types: usize = projects
            .iter()
            .flat_map(|p| &p.files)
            .flat_map(|f| &f.types)
            .filter(|t| matches!(t.resolution, TypeResolution::Unresolved))
            .count();

        output.push_str(&format!(
            "Analyzed {} file(s) across {} project(s)\n\n",
            total_files,
            projects.len()
        ));
        output.push_str("**Summary:**\n\n");
        output.push_str(&format!("- Total type references: {}\n", total_types));
        output.push_str(&format!("- Local (in analyzed files): {}\n", local_types));
        output.push_str(&format!("- External: {}\n", external_types));
        output.push_str(&format!("- Unresolved: {}\n\n", unresolved_types));
        output.push_str("---\n\n");

        // Detailed dependencies per project
        for project in projects {
            output.push_str(&format!(
                "## Project: {} ({:?})\n\n",
                project.project_name, project.project_type
            ));

            for file in &project.files {
                if file.types.is_empty() {
                    continue;
                }

                output.push_str(&format!("### File: `{}`\n\n", file.file_path));

                // Group by context
                let params: Vec<_> = file
                    .types
                    .iter()
                    .filter(|t| matches!(t.context, TypeContext::FunctionParameter))
                    .collect();
                let returns: Vec<_> = file
                    .types
                    .iter()
                    .filter(|t| matches!(t.context, TypeContext::FunctionReturn))
                    .collect();
                let fields: Vec<_> = file
                    .types
                    .iter()
                    .filter(|t| matches!(t.context, TypeContext::StructField))
                    .collect();
                let aliases: Vec<_> = file
                    .types
                    .iter()
                    .filter(|t| matches!(t.context, TypeContext::TypeAlias))
                    .collect();
                let traits: Vec<_> = file
                    .types
                    .iter()
                    .filter(|t| matches!(t.context, TypeContext::TraitBound))
                    .collect();

                if !params.is_empty() || !returns.is_empty() {
                    output.push_str("#### Function Signatures\n\n");
                    if !params.is_empty() {
                        output.push_str("**Parameters:**\n\n");
                        for typ in params {
                            output.push_str(&format_resolved_type(typ));
                        }
                    }
                    if !returns.is_empty() {
                        output.push_str("**Return Types:**\n\n");
                        for typ in returns {
                            output.push_str(&format_resolved_type(typ));
                        }
                    }
                }

                if !fields.is_empty() {
                    output.push_str("#### Struct/Class Fields\n\n");
                    for typ in fields {
                        output.push_str(&format_resolved_type(typ));
                    }
                }

                if !aliases.is_empty() {
                    output.push_str("#### Type Aliases\n\n");
                    for typ in aliases {
                        output.push_str(&format_resolved_type(typ));
                    }
                }

                if !traits.is_empty() {
                    output.push_str("#### Trait/Interface Bounds\n\n");
                    for typ in traits {
                        output.push_str(&format_resolved_type(typ));
                    }
                }

                output.push('\n');
            }

            output.push_str("---\n\n");
        }

        output
    }
}

fn format_resolved_type(resolved: &ResolvedType) -> String {
    match &resolved.resolution {
        TypeResolution::Local {
            file_path,
            line,
            kind,
        } => {
            format!(
                "- `{}` → defined in `{}:{}` ({})\n",
                resolved.type_name,
                file_path,
                line,
                kind
            )
        }
        TypeResolution::External { file_path, line } => {
            if let (Some(path), Some(l)) = (file_path, line) {
                format!("- `{}` → external: `{}:{}`\n", resolved.type_name, path, l)
            } else {
                format!("- `{}` → external\n", resolved.type_name)
            }
        }
        TypeResolution::Unresolved => {
            format!("- `{}` → unresolved\n", resolved.type_name)
        }
    }
}

fn format_diagnostic(diag: &lsp_types::Diagnostic) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "- **Line {}, Col {}**: {}\n",
        diag.range.start.line + 1,
        diag.range.start.character + 1,
        diag.message
    ));

    if let Some(source) = &diag.source {
        output.push_str(&format!("  - Source: `{}`\n", source));
    }

    if let Some(code) = &diag.code {
        let code_str = match code {
            lsp_types::NumberOrString::Number(n) => n.to_string(),
            lsp_types::NumberOrString::String(s) => s.clone(),
        };
        output.push_str(&format!("  - Code: `{}`\n", code_str));
    }

    output.push('\n');

    output
}

fn format_symbol_markdown(symbol: &SymbolInfo) -> String {
    let mut output = String::new();

    // Symbol name and kind
    output.push_str(&format!(
        "### `{}` ({})\n\n",
        symbol.name,
        symbol_kind_to_string(symbol.kind)
    ));

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

    // Type dependencies
    if let Some(type_deps) = &symbol.type_dependencies
        && !type_deps.is_empty() {
        output.push_str("**Type Dependencies:**\n\n");
        for resolved_type in type_deps {
            match &resolved_type.resolution {
                TypeResolution::Local {
                    file_path,
                    line,
                    kind,
                } => {
                    output.push_str(&format!(
                        "- `{}` → local: `{}:{}` ({})\n",
                        resolved_type.type_name,
                        file_path,
                        line + 1,
                        kind
                    ));
                }
                TypeResolution::External { file_path, line } => {
                    if let (Some(path), Some(line_num)) = (file_path, line) {
                        output.push_str(&format!(
                            "- `{}` → external: `{}:{}`\n",
                            resolved_type.type_name,
                            path,
                            line_num + 1
                        ));
                    } else {
                        output
                            .push_str(&format!("- `{}` → external\n", resolved_type.type_name));
                    }
                }
                TypeResolution::Unresolved => {
                    output.push_str(&format!("- `{}` → unresolved\n", resolved_type.type_name));
                }
            }
        }
        output.push('\n');
    }

    // Fields/Members (children)
    if !symbol.children.is_empty() {
        output.push_str("**Fields:**\n\n");
        for child in &symbol.children {
            let child_detail = child.detail.as_deref().unwrap_or("");
            output.push_str(&format!(
                "- `{}`: {} ({})\n",
                child.name,
                child_detail,
                symbol_kind_to_string(child.kind)
            ));
            if let Some(docs) = &child.documentation {
                output.push_str(&format!("  - {}\n", docs.lines().next().unwrap_or("")));
            }
        }
        output.push('\n');
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

        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
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

        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
    }

    fn format_by_projects(
        &self,
        projects: &[(String, ProjectType, Vec<(String, Vec<SymbolInfo>)>)],
    ) -> String {
        let mut project_outputs = Vec::new();

        for (project_name, project_type, files) in projects {
            let mut file_outputs = Vec::new();

            for (file_path, symbols) in files {
                let json_symbols: Vec<JsonSymbol> = symbols.iter().map(JsonSymbol::from).collect();
                file_outputs.push(serde_json::json!({
                    "file": file_path,
                    "symbols": json_symbols
                }));
            }

            project_outputs.push(serde_json::json!({
                "name": project_name,
                "type": format!("{:?}", project_type),
                "files": file_outputs
            }));
        }

        let output = serde_json::json!({
            "projects": project_outputs
        });

        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
    }

    fn format_diagnostics(&self, projects: &[ProjectDiagnostics]) -> String {
        let mut project_outputs = Vec::new();

        for project in projects {
            let mut file_outputs = Vec::new();

            for file in &project.files {
                let diagnostics_json: Vec<_> = file
                    .diagnostics
                    .iter()
                    .map(|d| {
                        let severity = match d.severity {
                            Some(lsp_types::DiagnosticSeverity::ERROR) => "Error",
                            Some(lsp_types::DiagnosticSeverity::WARNING) => "Warning",
                            Some(lsp_types::DiagnosticSeverity::INFORMATION) => "Information",
                            Some(lsp_types::DiagnosticSeverity::HINT) => "Hint",
                            _ => "Unknown",
                        };

                        let code = d.code.as_ref().map(|c| match c {
                            lsp_types::NumberOrString::Number(n) => n.to_string(),
                            lsp_types::NumberOrString::String(s) => s.clone(),
                        });

                        serde_json::json!({
                            "severity": severity,
                            "line": d.range.start.line + 1,
                            "column": d.range.start.character + 1,
                            "message": d.message,
                            "source": d.source,
                            "code": code,
                        })
                    })
                    .collect();

                file_outputs.push(serde_json::json!({
                    "file": file.file_path,
                    "diagnostics": diagnostics_json
                }));
            }

            project_outputs.push(serde_json::json!({
                "name": project.project_name,
                "type": format!("{:?}", project.project_type),
                "files": file_outputs
            }));
        }

        let output = serde_json::json!({
            "projects": project_outputs
        });

        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
    }

    fn format_type_dependencies(&self, projects: &[ProjectTypeDependencies]) -> String {
        let mut project_outputs = Vec::new();

        for project in projects {
            let mut file_outputs = Vec::new();

            for file in &project.files {
                let types_json: Vec<_> = file
                    .types
                    .iter()
                    .map(|t| {
                        let (resolution_type, resolution_data) = match &t.resolution {
                            TypeResolution::Local {
                                file_path,
                                line,
                                kind,
                            } => (
                                "local",
                                serde_json::json!({
                                    "file": file_path.to_string(),
                                    "line": line,
                                    "kind": kind,
                                }),
                            ),
                            TypeResolution::External { file_path, line } => (
                                "external",
                                serde_json::json!({
                                    "file": file_path,
                                    "line": line,
                                }),
                            ),
                            TypeResolution::Unresolved => ("unresolved", serde_json::json!({})),
                        };

                        let context = match t.context {
                            crate::analyze::type_extractor::TypeContext::FunctionParameter => {
                                "function_parameter"
                            }
                            crate::analyze::type_extractor::TypeContext::FunctionReturn => {
                                "function_return"
                            }
                            crate::analyze::type_extractor::TypeContext::StructField => {
                                "struct_field"
                            }
                            crate::analyze::type_extractor::TypeContext::TypeAlias => "type_alias",
                            crate::analyze::type_extractor::TypeContext::TraitBound => {
                                "trait_bound"
                            }
                        };

                        serde_json::json!({
                            "type_name": t.type_name,
                            "context": context,
                            "resolution": resolution_type,
                            "resolution_data": resolution_data,
                        })
                    })
                    .collect();

                file_outputs.push(serde_json::json!({
                    "file": file.file_path,
                    "types": types_json
                }));
            }

            project_outputs.push(serde_json::json!({
                "name": project.project_name,
                "type": format!("{:?}", project.project_type),
                "files": file_outputs
            }));
        }

        let output = serde_json::json!({
            "projects": project_outputs
        });

        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize: {}\"}}", e))
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
            type_dependencies: None,
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
