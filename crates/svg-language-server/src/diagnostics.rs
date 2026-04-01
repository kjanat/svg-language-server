use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range,
};

use crate::positions::byte_col_to_utf16;

pub fn lint_diagnostic_to_lsp(source: &[u8], diagnostic: svg_lint::SvgDiagnostic) -> Diagnostic {
    let start_char = byte_col_to_utf16(source, diagnostic.start_row, diagnostic.start_col);
    let end_char = byte_col_to_utf16(source, diagnostic.end_row, diagnostic.end_col);
    let severity = match diagnostic.severity {
        svg_lint::Severity::Error => DiagnosticSeverity::ERROR,
        svg_lint::Severity::Warning => DiagnosticSeverity::WARNING,
        svg_lint::Severity::Information => DiagnosticSeverity::INFORMATION,
        svg_lint::Severity::Hint => DiagnosticSeverity::HINT,
    };

    let tags = match diagnostic.code {
        svg_lint::DiagnosticCode::DeprecatedElement
        | svg_lint::DiagnosticCode::DeprecatedAttribute => Some(vec![DiagnosticTag::DEPRECATED]),
        _ => None,
    };

    Diagnostic {
        range: Range::new(
            Position::new(
                u32::try_from(diagnostic.start_row).unwrap_or(u32::MAX),
                start_char,
            ),
            Position::new(
                u32::try_from(diagnostic.end_row).unwrap_or(u32::MAX),
                end_char,
            ),
        ),
        severity: Some(severity),
        code: Some(NumberOrString::String(diagnostic.code.as_str().to_owned())),
        source: Some("svg-lint".to_owned()),
        message: diagnostic.message,
        tags,
        ..Default::default()
    }
}
