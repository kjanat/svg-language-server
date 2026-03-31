//! Integration tests for SVG completion behavior.

mod support;

use serde_json::json;
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn style_and_comment_completion_respect_context() -> TestResult {
    let mut server = TestServer::start()?;

    let style_completion_svg = "<svg><style>.a {\n  c\n}</style></svg>";
    server.open("file:///style-completion.svg", style_completion_svg)?;

    let completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///style-completion.svg" },
            "position": { "line": 1, "character": 3 }
        }),
    )?;
    let completion_items = completion_resp["result"]
        .as_array()
        .ok_or("completion result should be an array")?;
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

    let comment_completion_svg = r#"<svg>
    <filter id="f1">
        <!-- Place cursor after < here -->
    </filter>
</svg>"#;
    server.open("file:///comment-completion.svg", comment_completion_svg)?;

    let comment_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///comment-completion.svg" },
            "position": { "line": 2, "character": 33 }
        }),
    )?;
    assert!(
        comment_completion_resp["result"].is_null(),
        "completion should be disabled inside XML comments: {comment_completion_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn attribute_and_element_completion_filters_invalid_suggestions() -> TestResult {
    let mut server = TestServer::start()?;

    let test_svg = r##"<svg><rect fill="#ff0000" stroke="blue"/></svg>"##;
    server.open("file:///test.svg", test_svg)?;

    let root_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///test.svg" },
            "position": { "line": 0, "character": 5 }
        }),
    )?;
    let root_items = root_completion_resp["result"]
        .as_array()
        .ok_or("completion result should be an array")?;
    let root_labels: Vec<&str> = root_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        root_labels.contains(&"stroke-width"),
        "completions should include other applicable attributes: {root_labels:?}"
    );

    let attribute_completion_svg = r#"<svg><use height="32" /></svg>"#;
    server.open("file:///attribute-completion.svg", attribute_completion_svg)?;

    let attribute_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///attribute-completion.svg" },
            "position": { "line": 0, "character": 22 }
        }),
    )?;
    let attribute_completion_items = attribute_completion_resp["result"]
        .as_array()
        .ok_or("attribute completion result should be an array")?;
    let attribute_labels: Vec<&str> = attribute_completion_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        attribute_labels.contains(&"width"),
        "applicable attribute completions should still be returned: {attribute_completion_resp}"
    );
    assert!(
        !attribute_labels.contains(&"height"),
        "already-specified attributes should not be suggested again: {attribute_completion_resp}"
    );
    assert!(
        !attribute_labels.contains(&"xlink:href"),
        "deprecated attributes should not be suggested: {attribute_completion_resp}"
    );

    let typed_attribute_completion_svg = r#"<svg><animate dur="2s" /></svg>"#;
    server.open(
        "file:///typed-attribute-completion.svg",
        typed_attribute_completion_svg,
    )?;

    let self_close_offset = typed_attribute_completion_svg
        .find("/>")
        .ok_or("self-closing tag")?;
    let self_close_col = u32::try_from(self_close_offset)?;
    let typed_attribute_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///typed-attribute-completion.svg" },
            "position": {
                "line": 0,
                "character": self_close_col - 1
            }
        }),
    )?;
    let typed_attribute_completion_items = typed_attribute_completion_resp["result"]
        .as_array()
        .ok_or("typed attribute completion result should be an array")?;
    let typed_attribute_labels: Vec<&str> = typed_attribute_completion_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        !typed_attribute_labels.contains(&"dur"),
        "already-specified typed attributes should not be suggested again: {typed_attribute_completion_resp}"
    );
    assert!(
        typed_attribute_labels.contains(&"attributeName"),
        "other animate attributes should still be suggested: {typed_attribute_completion_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn script_and_href_completion_respect_svg_boundaries() -> TestResult {
    let mut server = TestServer::start()?;

    let script_completion_svg = r"<svg><script>con</script></svg>";
    server.open("file:///script-completion.svg", script_completion_svg)?;

    let script_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///script-completion.svg" },
            "position": { "line": 0, "character": 15 }
        }),
    )?;
    assert!(
        script_completion_resp["result"].is_null(),
        "SVG completions should not leak into <script> text content: {script_completion_resp}"
    );

    let href_completion_svg =
        r#"<svg><defs><linearGradient id="g1" /></defs><use href="" /></svg>"#;
    server.open("file:///href-completion.svg", href_completion_svg)?;

    let href_completion_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///href-completion.svg" },
            "position": { "line": 0, "character": 55 }
        }),
    )?;
    let href_completion_items = href_completion_resp["result"]
        .as_array()
        .ok_or("href completion result should be an array")?;
    assert!(
        href_completion_items
            .iter()
            .any(|item| item["label"].as_str() == Some("#g1")),
        "href value completions should include in-document fragment references: {href_completion_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}
