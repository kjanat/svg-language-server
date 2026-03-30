//! Integration tests for color and formatting LSP flows.

mod support;

use serde_json::json;
use support::TestServer;

#[test]
fn initialize_document_color_and_color_presentation() {
    let mut server = TestServer::new();

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
    assert_eq!(
        caps["executeCommandProvider"]["commands"][0].as_str(),
        Some("svg.copyDataUri"),
        "executeCommandProvider should advertise the copy-data-uri command: {}",
        server.init_response
    );

    let svg_text = r##"<svg><rect fill="#ff0000" stroke="blue"/></svg>"##;
    server.open("file:///test.svg", svg_text);

    let color_resp = server.request(
        "textDocument/documentColor",
        json!({
            "textDocument": { "uri": "file:///test.svg" }
        }),
    );
    let colors = color_resp["result"]
        .as_array()
        .expect("documentColor result should be an array");
    assert_eq!(
        colors.len(),
        2,
        "expected 2 color entries (hex red + named blue), got {}: {colors:?}",
        colors.len()
    );

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

    let pres_resp = server.request(
        "textDocument/colorPresentation",
        json!({
            "textDocument": { "uri": "file:///test.svg" },
            "color": {
                "red": 1.0,
                "green": 0.0,
                "blue": 0.0,
                "alpha": 1.0
            },
            "range": red_entry["range"].clone()
        }),
    );
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

    server.shutdown_and_exit();
}
