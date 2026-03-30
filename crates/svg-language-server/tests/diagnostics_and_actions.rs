//! Integration tests for diagnostics and code actions.

mod support;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn wait_for_notification<F>(server: &TestServer, method: &str, mut predicate: F) -> Value
where
    F: FnMut(&Value) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for notification {method}"
        );
        match server.rx.recv_timeout(remaining) {
            Ok(msg) => {
                if msg.get("method").and_then(Value::as_str) == Some(method) && predicate(&msg) {
                    return msg;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for notification {method}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("reader thread disconnected while waiting for {method}");
            }
        }
    }
}

#[test]
fn missing_reference_diagnostics_and_code_actions() -> TestResult {
    let mut server = TestServer::start()?;

    let missing_ref_svg = r#"<svg><rect clip-path="url(#myClip)" filter="url(#myFilter)"/></svg>"#;
    server.open("file:///missing-ref.svg", missing_ref_svg)?;

    let missing_ref_diags =
        wait_for_notification(&server, "textDocument/publishDiagnostics", |msg| {
            msg["params"]["uri"].as_str() == Some("file:///missing-ref.svg")
        });
    let missing_ref_list = missing_ref_diags["params"]["diagnostics"]
        .as_array()
        .ok_or("publishDiagnostics should include an array")?;
    assert!(
        missing_ref_list.iter().any(|diag| {
            diag["code"].as_str() == Some("MissingReferenceDefinition")
                && diag["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("clip-path"))
        }),
        "missing clip-path definition should produce a lint warning: {missing_ref_diags}"
    );

    let code_action_resp = server.request(
        "textDocument/codeAction",
        &json!({
            "textDocument": { "uri": "file:///missing-ref.svg" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 }
            },
            "context": {
                "diagnostics": missing_ref_list
            }
        }),
    )?;
    let code_actions = code_action_resp["result"]
        .as_array()
        .ok_or("codeAction result should be an array")?;
    assert!(
        code_actions.iter().any(|action| {
            action["title"].as_str() == Some("Suppress MissingReferenceDefinition on this line")
        }),
        "line suppression quick-fix should be offered: {code_action_resp}"
    );
    assert!(
        code_actions.iter().any(|action| {
            action["title"].as_str() == Some("Suppress MissingReferenceDefinition in this file")
        }),
        "file suppression quick-fix should be offered: {code_action_resp}"
    );

    let copy_as_data_uri_action = code_actions
        .iter()
        .find(|action| action["title"].as_str() == Some("Copy SVG as data URI"))
        .ok_or("copy-as-data-uri source action should be offered")?;
    assert_eq!(
        copy_as_data_uri_action["command"]["command"].as_str(),
        Some("svg.copyDataUri"),
        "copy-as-data-uri source action should use the server copy command: {code_action_resp}"
    );
    assert!(
        copy_as_data_uri_action["command"]["arguments"][0]
            .as_str()
            .is_some_and(|value| value == "file:///missing-ref.svg"),
        "copy-as-data-uri source action should pass the document uri: {code_action_resp}"
    );
    assert!(
        code_actions.iter().any(|action| {
            action["edit"]["changes"]["file:///missing-ref.svg"][0]["newText"].as_str()
                == Some("<!-- svg-lint-disable MissingReferenceDefinition -->\n")
        }),
        "file suppression quick-fix should insert a suppression comment: {code_action_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn multiline_tag_suppression_inserts_before_opening_tag() -> TestResult {
    let mut server = TestServer::start()?;

    // Multiline tag with deprecated attribute on a later line
    let svg =
        "<svg>\n<text x=\"10\" y=\"260\"\n\tclip=\"rect(0,100,100,0)\">deprecated</text>\n</svg>";
    server.open("file:///multiline.svg", svg)?;

    let diag_msg = wait_for_notification(&server, "textDocument/publishDiagnostics", |msg| {
        msg["params"]["uri"].as_str() == Some("file:///multiline.svg")
    });
    let diag_list = diag_msg["params"]["diagnostics"]
        .as_array()
        .ok_or("diagnostics should be array")?;

    // Verify there's a DeprecatedAttribute diagnostic on row 2 (the `clip` line)
    let deprecated_diag = diag_list
        .iter()
        .find(|d| d["code"].as_str() == Some("DeprecatedAttribute"))
        .ok_or("expected DeprecatedAttribute diagnostic")?;
    assert_eq!(
        deprecated_diag["range"]["start"]["line"].as_u64(),
        Some(2),
        "deprecated attr should be on line 2 (0-indexed): {deprecated_diag}"
    );

    let code_action_resp = server.request(
        "textDocument/codeAction",
        &json!({
            "textDocument": { "uri": "file:///multiline.svg" },
            "range": {
                "start": { "line": 2, "character": 0 },
                "end": { "line": 2, "character": 0 }
            },
            "context": {
                "diagnostics": [deprecated_diag]
            }
        }),
    )?;
    let code_actions = code_action_resp["result"]
        .as_array()
        .ok_or("codeAction result should be an array")?;

    // The line suppression should insert BEFORE the <text line (row 1), not on the clip line (row 2)
    let line_action = code_actions
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .is_some_and(|t| t.contains("Suppress") && t.contains("on this line"))
        })
        .ok_or("line suppression action should exist")?;

    let edit_range = &line_action["edit"]["changes"]["file:///multiline.svg"][0]["range"];
    assert_eq!(
        edit_range["start"]["line"].as_u64(),
        Some(1),
        "suppression comment should be inserted at the tag's start line (row 1), not the attr line (row 2): {line_action}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn invalid_svg_publishes_diagnostics() -> TestResult {
    let mut server = TestServer::start()?;

    while server.rx.try_recv().is_ok() {}
    let invalid_svg = r"<svg><rect><circle/></rect></svg>";
    server.open("file:///invalid.svg", invalid_svg)?;

    let msg = wait_for_notification(&server, "textDocument/publishDiagnostics", |msg| {
        msg["params"]["uri"].as_str() == Some("file:///invalid.svg")
    });
    let diags = msg["params"]["diagnostics"]
        .as_array()
        .ok_or("diagnostics should be array")?;
    assert!(
        !diags.is_empty(),
        "invalid SVG should produce diagnostics: {diags:?}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}
