use std::collections::HashMap;

use serde_json::Value;
use tower_lsp_server::ls_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Command, Diagnostic, NumberOrString, Position,
    Range, TextEdit, Uri, WorkspaceEdit,
};

use super::{COPY_DATA_URI_ACTION_TITLE, COPY_DATA_URI_COMMAND};
use crate::positions::{byte_offset_for_position, position_for_byte_offset, u32_from_usize};

/// Compute the effective row for inserting a suppression comment.
///
/// If the diagnostic targets a node inside a multiline `start_tag` or
/// `self_closing_tag`, returns the tag's start row so the comment is placed
/// before the `<tag` line instead of inside the attribute list (which would
/// produce invalid XML). Falls back to `diagnostic.range.start.line`.
pub fn effective_suppression_row(
    source: &[u8],
    tree: &tree_sitter::Tree,
    diagnostic: &Diagnostic,
) -> u32 {
    let diag_row = diagnostic.range.start.line;
    let byte_offset = byte_offset_for_position(source, diagnostic.range.start);
    let node = tree
        .root_node()
        .descendant_for_byte_range(byte_offset, byte_offset)
        .unwrap_or_else(|| tree.root_node());

    // Walk ancestors looking for a start_tag/self_closing_tag
    let mut current = node;
    loop {
        let kind = current.kind();
        if kind == "start_tag" || kind == "self_closing_tag" {
            let tag_row = u32_from_usize(current.start_position().row);
            // Only redirect if the diagnostic is on a later row than the tag start
            // (i.e. the tag is multiline and the diag is inside the attr list)
            if tag_row < diag_row {
                return tag_row;
            }
            return diag_row;
        }
        // Stop at element boundary or root
        if kind == "element" || kind == "svg_root_element" || kind == "document" {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    diag_row
}

pub fn suppression_code(diagnostic: &Diagnostic) -> Option<&str> {
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

const fn line_start_range(row: u32) -> Range {
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

fn quickfix_action(
    title: String,
    diagnostic: &Diagnostic,
    edit: WorkspaceEdit,
) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(edit),
        is_preferred: Some(false),
        ..Default::default()
    })
}

fn command_code_action(
    title: &str,
    kind: CodeActionKind,
    command: &str,
    arguments: Vec<Value>,
) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title: title.to_owned(),
        kind: Some(kind),
        command: Some(Command {
            title: title.to_owned(),
            command: command.to_owned(),
            arguments: Some(arguments),
        }),
        ..Default::default()
    })
}

/// Build suppression quick-fix code actions.
///
/// `effective_row` is the row where the `disable-next-line` comment should be
/// inserted. For diagnostics inside multiline opening tags this is the tag's
/// start row (computed by `effective_suppression_row`), ensuring the comment
/// lands before the `<tag` line rather than inside the attribute list.
pub fn suppression_code_actions_for_diagnostic(
    uri: &Uri,
    source: &str,
    diagnostic: &Diagnostic,
    effective_row: u32,
) -> Vec<CodeActionOrCommand> {
    let Some(code) = suppression_code(diagnostic) else {
        return Vec::new();
    };

    let indentation = line_indentation(source, effective_row as usize);
    let line_comment = suppression_comment_text(code, true, &indentation);
    let file_comment = suppression_comment_text(code, false, "");
    let file_position = file_suppression_insert_position(source);

    vec![
        quickfix_action(
            format!("Suppress {code} on this line"),
            diagnostic,
            suppression_workspace_edit(uri, line_start_range(effective_row), line_comment),
        ),
        quickfix_action(
            format!("Suppress {code} in this file"),
            diagnostic,
            suppression_workspace_edit(uri, Range::new(file_position, file_position), file_comment),
        ),
    ]
}

pub fn copy_data_uri_code_action(uri: &Uri) -> CodeActionOrCommand {
    command_code_action(
        COPY_DATA_URI_ACTION_TITLE,
        CodeActionKind::SOURCE,
        COPY_DATA_URI_COMMAND,
        vec![Value::String(uri.as_str().to_owned())],
    )
}
