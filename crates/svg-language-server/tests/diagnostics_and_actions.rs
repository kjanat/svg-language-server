//! Integration tests for diagnostics and code actions.

mod support;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::TestServer;

fn wait_for_notification<F>(server: &mut TestServer, method: &str, mut predicate: F) -> Value
where
    F: FnMut(&Value) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for notification {method}");
        }
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
fn missing_reference_diagnostics_and_code_actions() {
    let mut server = TestServer::new();

    let missing_ref_svg = r#"<svg><rect clip-path="url(#myClip)" filter="url(#myFilter)"/></svg>"#;
    server.open("file:///missing-ref.svg", missing_ref_svg);

    let missing_ref_diags =
        wait_for_notification(&mut server, "textDocument/publishDiagnostics", |msg| {
            msg["params"]["uri"].as_str() == Some("file:///missing-ref.svg")
        });
    let missing_ref_list = missing_ref_diags["params"]["diagnostics"]
        .as_array()
        .expect("publishDiagnostics should include an array");
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
        json!({
            "textDocument": { "uri": "file:///missing-ref.svg" },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 }
            },
            "context": {
                "diagnostics": missing_ref_list
            }
        }),
    );
    let code_actions = code_action_resp["result"]
        .as_array()
        .expect("codeAction result should be an array");
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
        .expect("copy-as-data-uri source action should be offered");
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

    server.shutdown_and_exit();
}

#[test]
fn invalid_svg_publishes_diagnostics() {
    let mut server = TestServer::new();

    while server.rx.try_recv().is_ok() {}
    let invalid_svg = r##"<svg><rect><circle/></rect></svg>"##;
    server.open("file:///invalid.svg", invalid_svg);

    let msg = wait_for_notification(&mut server, "textDocument/publishDiagnostics", |msg| {
        msg["params"]["uri"].as_str() == Some("file:///invalid.svg")
    });
    let diags = msg["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics should be array");
    assert!(
        !diags.is_empty(),
        "invalid SVG should produce diagnostics: {diags:?}"
    );

    server.shutdown_and_exit();
}
