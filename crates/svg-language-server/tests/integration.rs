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

    // --- 5. colorPresentation for the first color (red) ---
    let red_range = &red_entry["range"];
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
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

    let pres_resp = recv_response(&rx, 3, timeout);
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

    // --- 6. shutdown ---
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

    // --- 7. exit ---
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
