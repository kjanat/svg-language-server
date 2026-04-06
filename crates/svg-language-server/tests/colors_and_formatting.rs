//! Integration tests for color and formatting LSP flows.

mod support;

use serde_json::json;
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assert_init_capabilities(server: &TestServer) -> TestResult {
    let caps = &server.init_response["result"]["capabilities"];
    assert!(
        caps.get("colorProvider").is_some(),
        "colorProvider capability missing from initialize response: {}",
        server.init_response
    );
    assert!(
        caps.get("documentFormattingProvider").is_some(),
        "documentFormattingProvider capability missing from initialize response: {}",
        server.init_response
    );
    let commands = caps["executeCommandProvider"]["commands"]
        .as_array()
        .ok_or("executeCommandProvider.commands should be an array")?;
    assert!(
        commands
            .iter()
            .any(|command| command.as_str() == Some("svg.copyDataUri")),
        "executeCommandProvider should advertise the copy-data-uri command: {}",
        server.init_response
    );
    Ok(())
}

#[test]
fn initialize_document_color_and_color_presentation() -> TestResult {
    let mut server = TestServer::start()?;

    assert_init_capabilities(&server)?;

    let svg_text = r##"<svg><rect fill="#ff0000" stroke="blue"/></svg>"##;
    server.open("file:///test.svg", svg_text)?;

    let color_resp = server.request(
        "textDocument/documentColor",
        &json!({
            "textDocument": { "uri": "file:///test.svg" }
        }),
    )?;
    let colors = color_resp["result"]
        .as_array()
        .ok_or("documentColor result should be an array")?;
    assert_eq!(
        colors.len(),
        2,
        "expected 2 color entries (hex red + named blue), got {}: {colors:?}",
        colors.len()
    );

    let red_entry = &colors[0];
    let red_color = &red_entry["color"];
    let red_r = red_color["red"].as_f64().ok_or("red channel missing")?;
    let red_g = red_color["green"].as_f64().ok_or("green channel missing")?;
    let red_b = red_color["blue"].as_f64().ok_or("blue channel missing")?;
    assert!(
        (red_r - 1.0).abs() < 0.01,
        "red channel mismatch: {red_color}"
    );
    assert!(red_g.abs() < 0.01, "green channel mismatch: {red_color}");
    assert!(red_b.abs() < 0.01, "blue channel mismatch: {red_color}");

    let blue_entry = &colors[1];
    let blue_color = &blue_entry["color"];
    let blue_r = blue_color["red"]
        .as_f64()
        .ok_or("blue entry red channel missing")?;
    let blue_b = blue_color["blue"]
        .as_f64()
        .ok_or("blue entry blue channel missing")?;
    assert!(blue_r.abs() < 0.01, "blue entry red channel: {blue_color}");
    assert!(
        (blue_b - 1.0).abs() < 0.01,
        "blue entry blue channel: {blue_color}"
    );

    let pres_resp = server.request(
        "textDocument/colorPresentation",
        &json!({
            "textDocument": { "uri": "file:///test.svg" },
            "color": {
                "red": 1.0,
                "green": 0.0,
                "blue": 0.0,
                "alpha": 1.0
            },
            "range": red_entry["range"].clone()
        }),
    )?;
    let presentations = pres_resp["result"]
        .as_array()
        .ok_or("colorPresentation result should be an array")?;
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
        labels.iter().any(|label| label.starts_with('#')),
        "expected a hex presentation: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.starts_with("rgb(")),
        "expected an rgb presentation: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.starts_with("hsl(")),
        "expected an hsl presentation: {labels:?}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}
