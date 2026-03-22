use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

/// Frame and write a JSON-RPC message with Content-Length header.
fn send_message(stdin: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).expect("serialize JSON-RPC message");
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin
        .write_all(header.as_bytes())
        .expect("write header to stdin");
    stdin
        .write_all(body.as_bytes())
        .expect("write body to stdin");
    stdin.flush().expect("flush stdin");
}

/// Read a single JSON-RPC message from the stream: parse Content-Length header,
/// then read exactly that many bytes and deserialize.
fn read_message(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut content_length: Option<usize> = None;

    // Read headers until blank line.
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read header line");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
            content_length = Some(val.parse().expect("parse Content-Length"));
        }
    }

    let len = content_length.expect("Content-Length header missing");
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf).expect("read message body");
    serde_json::from_slice(&buf).expect("parse JSON body")
}

#[test]
fn lsp_end_to_end() {
    // Locate the binary. Build it first to ensure it is up-to-date.
    let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("resolve workspace root");

    let status = Command::new("cargo")
        .args(["build", "-p", "svg-language-server"])
        .current_dir(project_root)
        .status()
        .expect("run cargo build");
    assert!(status.success(), "cargo build failed");

    let binary = project_root.join("target/debug/svg-language-server");
    assert!(binary.exists(), "binary not found at {}", binary.display());

    // Spawn the server.
    let mut child = Command::new(&binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn svg-language-server");

    let mut stdin = child.stdin.take().expect("take stdin");
    let stdout = child.stdout.take().expect("take stdout");

    // Set a read timeout via a wrapper thread: if the test hangs we get a clear failure.
    // We communicate through a channel instead of raw blocking reads.
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    let reader_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let msg = read_message(&mut reader);
            if tx.send(msg).is_err() {
                break; // receiver dropped
            }
        }
    });

    /// Receive the next response matching `id`, with a timeout.
    /// Skips notifications.
    fn recv_response(
        rx: &std::sync::mpsc::Receiver<Value>,
        expected_id: u64,
        timeout: Duration,
    ) -> Value {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for response with id {}", expected_id);
            }
            match rx.recv_timeout(remaining) {
                Ok(msg) => {
                    if msg.get("id").and_then(Value::as_u64) == Some(expected_id) {
                        return msg;
                    }
                    // notification or wrong id — skip
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for response with id {}", expected_id);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!(
                        "reader thread disconnected while waiting for id {}",
                        expected_id
                    );
                }
            }
        }
    }

    fn recv_notification<F>(
        rx: &std::sync::mpsc::Receiver<Value>,
        method: &str,
        timeout: Duration,
        predicate: F,
    ) -> Value
    where
        F: Fn(&Value) -> bool,
    {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for notification {}", method);
            }
            match rx.recv_timeout(remaining) {
                Ok(msg) => {
                    if msg.get("method").and_then(Value::as_str) == Some(method) && predicate(&msg)
                    {
                        return msg;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for notification {}", method);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("reader thread disconnected while waiting for {}", method);
                }
            }
        }
    }

    let timeout = Duration::from_secs(10);

    // --- 1. initialize ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": null,
                "capabilities": {}
            }
        }),
    );

    let init_resp = recv_response(&rx, 1, timeout);
    let caps = &init_resp["result"]["capabilities"];
    assert!(
        caps.get("colorProvider").is_some(),
        "colorProvider capability missing from initialize response: {init_resp}"
    );
    assert!(
        caps.get("documentFormattingProvider").is_some(),
        "documentFormattingProvider capability missing from initialize response: {init_resp}"
    );

    // --- 2. initialized ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );

    // --- 3. didOpen ---
    let svg_text = r##"<svg><rect fill="#ff0000" stroke="blue"/></svg>"##;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///test.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": svg_text
                }
            }
        }),
    );

    // --- 4. documentColor ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentColor",
            "params": {
                "textDocument": { "uri": "file:///test.svg" }
            }
        }),
    );

    let color_resp = recv_response(&rx, 2, timeout);
    let colors = color_resp["result"]
        .as_array()
        .expect("documentColor result should be an array");
    assert_eq!(
        colors.len(),
        2,
        "expected 2 color entries (hex red + named blue), got {}: {colors:?}",
        colors.len()
    );

    // Verify #ff0000 → red(1,0,0)
    let red_entry = &colors[0];
    let red_color = &red_entry["color"];
    assert!(
        (red_color["red"].as_f64().unwrap() - 1.0).abs() < 0.01,
        "red channel mismatch: {red_color}"
    );
    assert!(
        (red_color["green"].as_f64().unwrap()).abs() < 0.01,
        "green channel mismatch: {red_color}"
    );
    assert!(
        (red_color["blue"].as_f64().unwrap()).abs() < 0.01,
        "blue channel mismatch: {red_color}"
    );

    // Verify blue → (0,0,1)
    let blue_entry = &colors[1];
    let blue_color = &blue_entry["color"];
    assert!(
        (blue_color["red"].as_f64().unwrap()).abs() < 0.01,
        "blue entry red channel: {blue_color}"
    );
    assert!(
        (blue_color["blue"].as_f64().unwrap() - 1.0).abs() < 0.01,
        "blue entry blue channel: {blue_color}"
    );

    // --- 5. didOpen second SVG for class go-to-definition ---
    let class_svg = r#"<svg><style>.uses-color{fill:red}</style><rect class="uses-color"/></svg>"#;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///class-test.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": class_svg
                }
            }
        }),
    );

    let class_ref = class_svg
        .rfind("uses-color")
        .expect("class reference present") as u32
        + 2;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": "file:///class-test.svg" },
                "position": { "line": 0, "character": class_ref }
            }
        }),
    );

    let definition_resp = recv_response(&rx, 3, timeout);
    let definition = &definition_resp["result"];
    assert_eq!(
        definition["uri"].as_str(),
        Some("file:///class-test.svg"),
        "definition should stay in the same SVG document: {definition_resp}"
    );
    let expected_class_start = class_svg.find(".uses-color").expect("class selector") as u64 + 1;
    assert_eq!(
        definition["range"]["start"]["line"].as_u64(),
        Some(0),
        "class definition should be on the first line: {definition_resp}"
    );
    assert_eq!(
        definition["range"]["start"]["character"].as_u64(),
        Some(expected_class_start),
        "definition should point at the CSS class token, not the attribute wrapper: {definition_resp}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///class-test.svg" },
                "position": { "line": 0, "character": class_ref }
            }
        }),
    );

    let hover_resp = recv_response(&rx, 4, timeout);
    let hover_text = hover_resp["result"]["contents"]["value"]
        .as_str()
        .expect("hover markdown");
    assert!(
        hover_text.contains(".uses-color"),
        "class hover should include the selector name: {hover_resp}"
    );
    assert!(
        hover_text.contains("```css"),
        "class hover should render the definition as CSS markdown: {hover_resp}"
    );
    assert!(
        hover_text.contains("fill:red"),
        "class hover should include the CSS definition snippet: {hover_resp}"
    );
    assert!(
        hover_text.contains("class-test.svg:1"),
        "class hover should show a short source label: {hover_resp}"
    );
    assert!(
        hover_text.contains("[class-test.svg:1](file:///class-test.svg#L1)"),
        "class hover should provide a clickable source link: {hover_resp}"
    );

    let vars_svg = r#"<svg><style>:root { --panel-bg: red; } .var-alpha { fill: var(--panel-bg); }</style></svg>"#;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///vars-test.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": vars_svg
                }
            }
        }),
    );

    let var_ref = vars_svg.find("var(--panel-bg)").expect("var reference") as u32 + 6;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": "file:///vars-test.svg" },
                "position": { "line": 0, "character": var_ref }
            }
        }),
    );

    let var_definition_resp = recv_response(&rx, 5, timeout);
    let var_definition = &var_definition_resp["result"];
    let expected_var_start = vars_svg
        .find("--panel-bg: red")
        .expect("property definition") as u64;
    assert_eq!(
        var_definition["uri"].as_str(),
        Some("file:///vars-test.svg"),
        "custom property definition should stay in the same SVG document: {var_definition_resp}"
    );
    assert_eq!(
        var_definition["range"]["start"]["character"].as_u64(),
        Some(expected_var_start),
        "definition should point at the custom property declaration: {var_definition_resp}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///vars-test.svg" },
                "position": { "line": 0, "character": var_ref }
            }
        }),
    );

    let var_hover_resp = recv_response(&rx, 6, timeout);
    let var_hover_text = var_hover_resp["result"]["contents"]["value"]
        .as_str()
        .expect("custom property hover markdown");
    assert!(
        var_hover_text.contains("--panel-bg: red"),
        "custom property hover should include the declaration snippet: {var_hover_resp}"
    );
    assert!(
        var_hover_text.contains("```css"),
        "custom property hover should render the declaration as CSS markdown: {var_hover_resp}"
    );
    assert!(
        var_hover_text.contains("vars-test.svg:1"),
        "custom property hover should show a short source label: {var_hover_resp}"
    );
    assert!(
        var_hover_text.contains("[vars-test.svg:1](file:///vars-test.svg#L1)"),
        "custom property hover should provide a clickable source link: {var_hover_resp}"
    );

    let missing_ref_svg = r#"<svg><rect clip-path="url(#myClip)" filter="url(#myFilter)"/></svg>"#;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///missing-ref.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": missing_ref_svg
                }
            }
        }),
    );

    let missing_ref_diags =
        recv_notification(&rx, "textDocument/publishDiagnostics", timeout, |msg| {
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

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": "file:///missing-ref.svg" },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 0 }
                },
                "context": {
                    "diagnostics": missing_ref_list
                }
            }
        }),
    );

    let code_action_resp = recv_response(&rx, 8, timeout);
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
    assert!(
        code_actions
            .iter()
            .any(|action| action["title"].as_str() == Some("Copy SVG as data URI")),
        "copy-as-data-uri source action should be offered: {code_action_resp}"
    );
    assert!(
        code_actions.iter().any(|action| {
            action["edit"]["changes"]["file:///missing-ref.svg"][0]["newText"].as_str()
                == Some("<!-- svg-lint-disable MissingReferenceDefinition -->\n")
        }),
        "file suppression quick-fix should insert a suppression comment: {code_action_resp}"
    );

    let style_completion_svg = "<svg><style>.a {\n  c\n}</style></svg>";
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///style-completion.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": style_completion_svg
                }
            }
        }),
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": "file:///style-completion.svg" },
                "position": { "line": 1, "character": 3 }
            }
        }),
    );

    let completion_resp = recv_response(&rx, 9, timeout);
    let completion_items = completion_resp["result"]
        .as_array()
        .expect("completion result should be an array");
    assert!(
        completion_items
            .iter()
            .any(|item| item["label"].as_str() == Some("clip-path")),
        "CSS property completions should be returned inside <style>: {completion_resp}"
    );
    assert!(
        !completion_items
            .iter()
            .any(|item| item["label"].as_str() == Some("circle")),
        "SVG element completions should not leak into <style> CSS context: {completion_resp}"
    );

    // --- 6. colorPresentation for the first color (red) ---
    let red_range = &red_entry["range"];
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/colorPresentation",
            "params": {
                "textDocument": { "uri": "file:///test.svg" },
                "color": {
                    "red": 1.0,
                    "green": 0.0,
                    "blue": 0.0,
                    "alpha": 1.0
                },
                "range": red_range
            }
        }),
    );

    let pres_resp = recv_response(&rx, 7, timeout);
    let presentations = pres_resp["result"]
        .as_array()
        .expect("colorPresentation result should be an array");
    assert!(
        presentations.len() >= 3,
        "expected at least 3 presentations (hex, rgb, hsl), got {}: {presentations:?}",
        presentations.len()
    );

    let labels: Vec<&str> = presentations
        .iter()
        .filter_map(|p| p["label"].as_str())
        .collect();

    assert!(
        labels.iter().any(|l| l.starts_with('#')),
        "expected a hex presentation: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.starts_with("rgb(")),
        "expected an rgb presentation: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.starts_with("hsl(")),
        "expected an hsl presentation: {labels:?}"
    );

    // --- 6. hover test: element name ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///test.svg" },
                "position": { "line": 0, "character": 7 }
            }
        }),
    );

    let hover_resp = recv_response(&rx, 10, timeout);
    let hover_result = &hover_resp["result"];
    assert!(
        hover_result.get("contents").is_some(),
        "hover should return contents: {hover_resp}"
    );
    let hover_value = hover_result["contents"]["value"].as_str().unwrap_or("");
    assert!(
        hover_value.contains("MDN Reference"),
        "hover should contain MDN link: {hover_value}"
    );

    // --- 6b. hover test: typed SVG attribute name (`d`) ---
    let hover_svg =
        r#"<svg xmlns="http://www.w3.org/2000/svg" xml:lang="en"><path d="M0 0"/></svg>"#;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///hover.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": hover_svg
                }
            }
        }),
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///hover.svg" },
                "position": { "line": 0, "character": 60 }
            }
        }),
    );

    let hover_d_resp = recv_response(&rx, 12, timeout);
    let hover_d_value = hover_d_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        hover_d_value.contains("Defines a path to be drawn."),
        "`d` hover should come from the local attribute catalog: {hover_d_resp}"
    );

    // --- 6c. hover test: XML infrastructure attribute (`xmlns`) ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///hover.svg" },
                "position": { "line": 0, "character": 7 }
            }
        }),
    );

    let hover_xmlns_resp = recv_response(&rx, 13, timeout);
    let hover_xmlns_value = hover_xmlns_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        hover_xmlns_value.contains("W3C Namespaces in XML"),
        "`xmlns` hover should use the external namespace reference: {hover_xmlns_resp}"
    );

    // --- 7. completion test ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": "file:///test.svg" },
                "position": { "line": 0, "character": 5 }
            }
        }),
    );

    let comp_resp = recv_response(&rx, 11, timeout);
    let comp_items = comp_resp["result"]
        .as_array()
        .expect("completion result should be an array");
    assert!(!comp_items.is_empty(), "should return completion items");
    let labels: Vec<&str> = comp_items
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"fill"),
        "completions should include fill: {labels:?}"
    );

    // --- 8. diagnostics test ---
    // Drain any buffered notifications
    while rx.try_recv().is_ok() {}

    // Open a file with invalid nesting
    let invalid_svg = r##"<svg><rect><circle/></rect></svg>"##;
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///invalid.svg",
                    "languageId": "svg",
                    "version": 1,
                    "text": invalid_svg
                }
            }
        }),
    );

    // Read messages until we find publishDiagnostics for invalid.svg
    let diag_deadline = std::time::Instant::now() + timeout;
    let mut found_diags = false;
    while std::time::Instant::now() < diag_deadline {
        let remaining = diag_deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(msg) => {
                if msg.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
                {
                    let params = &msg["params"];
                    if params["uri"].as_str() == Some("file:///invalid.svg") {
                        let diags = params["diagnostics"]
                            .as_array()
                            .expect("diagnostics should be array");
                        assert!(
                            !diags.is_empty(),
                            "invalid SVG should produce diagnostics: {diags:?}"
                        );
                        found_diags = true;
                        break;
                    }
                }
                // Skip other messages
            }
            Err(_) => break,
        }
    }
    assert!(
        found_diags,
        "should have received publishDiagnostics notification for invalid.svg"
    );

    // --- 9. shutdown ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "shutdown",
            "params": null
        }),
    );

    let shutdown_resp = recv_response(&rx, 4, timeout);
    assert!(
        shutdown_resp.get("result").is_some(),
        "shutdown should return a result: {shutdown_resp}"
    );

    // --- 10. exit ---
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }),
    );

    // Close stdin so the server can terminate.
    drop(stdin);

    let exit_status = child.wait().expect("wait for server process");
    assert!(
        exit_status.success(),
        "server exited with non-zero status: {exit_status}"
    );

    // Reader thread will terminate once stdout is closed.
    drop(rx);
    let _ = reader_thread.join();
}
