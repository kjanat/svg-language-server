use super::*;

pub(crate) fn suppression_code(diagnostic: &Diagnostic) -> Option<&str> {
    if diagnostic.source.as_deref() != Some("svg-lint") {
        return None;
    }

    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => code
            .parse::<svg_lint::DiagnosticCode>()
            .ok()
            .map(|_| code.as_str()),
        NumberOrString::Number(_) => None,
    }
}

fn line_indentation(source: &str, row: usize) -> String {
    source
        .lines()
        .nth(row)
        .unwrap_or_default()
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect()
}

fn line_start_range(row: u32) -> Range {
    Range::new(Position::new(row, 0), Position::new(row, 0))
}

fn file_suppression_insert_position(source: &str) -> Position {
    if source.starts_with("<?xml")
        && let Some(decl_end) = source.find("?>")
    {
        let mut offset = decl_end + 2;
        if source[offset..].starts_with("\r\n") {
            offset += 2;
        } else if source[offset..].starts_with('\n') {
            offset += 1;
        }
        return position_for_byte_offset(source.as_bytes(), offset);
    }

    Position::new(0, 0)
}

fn suppression_comment_text(code: &str, next_line: bool, indentation: &str) -> String {
    let directive = if next_line {
        "svg-lint-disable-next-line"
    } else {
        "svg-lint-disable"
    };
    format!("{indentation}<!-- {directive} {code} -->\n")
}

fn suppression_workspace_edit(uri: &Uri, range: Range, new_text: String) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(HashMap::from([(
            uri.clone(),
            vec![TextEdit { range, new_text }],
        )])),
        ..Default::default()
    }
}

pub(crate) fn suppression_code_actions_for_diagnostic(
    uri: &Uri,
    source: &str,
    diagnostic: &Diagnostic,
) -> Vec<CodeActionOrCommand> {
    let Some(code) = suppression_code(diagnostic) else {
        return Vec::new();
    };

    let line = diagnostic.range.start.line as usize;
    let indentation = line_indentation(source, line);
    let line_comment = suppression_comment_text(code, true, &indentation);
    let file_comment = suppression_comment_text(code, false, "");
    let file_position = file_suppression_insert_position(source);

    vec![
        CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Suppress {code} on this line"),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(suppression_workspace_edit(
                uri,
                line_start_range(diagnostic.range.start.line),
                line_comment,
            )),
            is_preferred: Some(false),
            ..Default::default()
        }),
        CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Suppress {code} in this file"),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(suppression_workspace_edit(
                uri,
                Range::new(file_position, file_position),
                file_comment,
            )),
            is_preferred: Some(false),
            ..Default::default()
        }),
    ]
}

pub(crate) fn copy_data_uri_code_action(uri: &Uri) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title: COPY_DATA_URI_ACTION_TITLE.to_owned(),
        kind: Some(CodeActionKind::SOURCE),
        command: Some(Command {
            title: COPY_DATA_URI_ACTION_TITLE.to_owned(),
            command: COPY_DATA_URI_COMMAND.to_owned(),
            arguments: Some(vec![Value::String(uri.as_str().to_owned())]),
        }),
        ..Default::default()
    })
}
