use super::{
    Client, Diagnostic, DiagnosticSeverity, DiagnosticTag, NumberOrString, Position, Range, Uri,
    byte_col_to_utf16,
};

fn lint_diagnostic_to_lsp(source: &[u8], diagnostic: svg_lint::SvgDiagnostic) -> Diagnostic {
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
        svg_lint::DiagnosticCode::ExperimentalElement
        | svg_lint::DiagnosticCode::ExperimentalAttribute => Some(vec![DiagnosticTag::UNNECESSARY]),
        _ => None,
    };

    Diagnostic {
        range: Range::new(
            Position::new(diagnostic.start_row as u32, start_char),
            Position::new(diagnostic.end_row as u32, end_char),
        ),
        severity: Some(severity),
        code: Some(NumberOrString::String(diagnostic.code.as_str().to_owned())),
        source: Some("svg-lint".to_owned()),
        message: diagnostic.message,
        tags,
        ..Default::default()
    }
}

pub(crate) async fn publish_lint_diagnostics(
    client: &Client,
    uri: Uri,
    source: &[u8],
    diagnostics: Vec<svg_lint::SvgDiagnostic>,
) {
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| lint_diagnostic_to_lsp(source, diagnostic))
        .collect();
    client.publish_diagnostics(uri, diagnostics, None).await;
}
