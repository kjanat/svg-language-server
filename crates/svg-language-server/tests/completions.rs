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
        "already-specified typed attributes should not be suggested again: \
         {typed_attribute_completion_resp}"
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
        "href value completions should include in-document fragment references: \
         {href_completion_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn typed_attribute_values_offer_context_aware_completions() -> TestResult {
    let mut server = TestServer::start()?;

    let dur_svg = r#"<svg><animate dur="" /></svg>"#;
    server.open("file:///dur-completion.svg", dur_svg)?;

    let dur_offset = dur_svg.find(r#"dur=""#).ok_or("dur attr")? + 5;
    let dur_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///dur-completion.svg" },
            "position": { "line": 0, "character": dur_offset }
        }),
    )?;
    let dur_items = dur_resp["result"]
        .as_array()
        .ok_or("duration completion result should be an array")?;
    let dur_labels: Vec<&str> = dur_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        dur_labels.contains(&"1s"),
        "duration value completions should include time values: {dur_labels:?}"
    );
    assert!(
        dur_labels.contains(&"indefinite"),
        "duration value completions should include indefinite: {dur_labels:?}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn color_paint_and_reference_value_completions_use_valid_syntax() -> TestResult {
    let mut server = TestServer::start()?;

    let svg = r#"<svg><symbol id="ss"/><rect color="" fill="" clip-path="" /></svg>"#;
    server.open("file:///paint-color-completion.svg", svg)?;

    let color_offset = svg.find(r#"color="""#).ok_or("color attr")? + 7;
    let color_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///paint-color-completion.svg" },
            "position": { "line": 0, "character": color_offset }
        }),
    )?;
    let color_items = color_resp["result"]
        .as_array()
        .ok_or("color completion result should be an array")?;
    let color_labels: Vec<&str> = color_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        [
            "currentColor",
            "rgb()",
            "hsl()",
            "oklch()",
            "color-mix()",
            "rebeccapurple"
        ]
        .into_iter()
        .all(|label| color_labels.contains(&label)),
        "color completions should include CSS color values: {color_labels:?}"
    );
    assert!(
        ["#ss", "url(#ss)", "none", "context-fill", "context-stroke"]
            .into_iter()
            .all(|label| !color_labels.contains(&label)),
        "color completions should not include paint/reference values: {color_labels:?}"
    );

    let fill_offset = svg.find(r#"fill="""#).ok_or("fill attr")? + 6;
    let fill_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///paint-color-completion.svg" },
            "position": { "line": 0, "character": fill_offset }
        }),
    )?;
    let fill_items = fill_resp["result"]
        .as_array()
        .ok_or("fill completion result should be an array")?;
    let fill_labels: Vec<&str> = fill_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        ["url(#ss)", "rgb()", "currentColor", "none", "context-fill"]
            .into_iter()
            .all(|label| fill_labels.contains(&label)),
        "paint server attrs should include paint values: {fill_labels:?}"
    );
    assert!(
        !fill_labels.contains(&"#ss"),
        "paint server attrs should not include bare fragment references: {fill_labels:?}"
    );

    let clip_path_offset = svg.find(r#"clip-path="""#).ok_or("clip-path attr")? + 11;
    let clip_path_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///paint-color-completion.svg" },
            "position": { "line": 0, "character": clip_path_offset }
        }),
    )?;
    let clip_path_items = clip_path_resp["result"]
        .as_array()
        .ok_or("clip-path completion result should be an array")?;
    let clip_path_labels: Vec<&str> = clip_path_items
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(
        clip_path_labels.contains(&"url(#ss)"),
        "functional IRI attrs should include url() fragment references: {clip_path_labels:?}"
    );
    assert!(
        !clip_path_labels.contains(&"#ss"),
        "functional IRI attrs should not include bare fragment references: {clip_path_labels:?}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn completions_follow_selected_profile() -> TestResult {
    let mut svg11_server = TestServer::start_with_initialize_options(&json!({
        "svg": {
            "profile": "svg11"
        }
    }))?;

    let attribute_completion_svg = r#"<svg><use height="32" /></svg>"#;
    svg11_server.open(
        "file:///profile-attribute-completion.svg",
        attribute_completion_svg,
    )?;

    let svg11_resp = svg11_server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///profile-attribute-completion.svg" },
            "position": { "line": 0, "character": 22 }
        }),
    )?;
    let svg11_items = svg11_resp["result"]
        .as_array()
        .ok_or("SVG 1.1 completion result should be an array")?;
    assert!(
        svg11_items
            .iter()
            .any(|item| item["label"].as_str() == Some("xlink:href")),
        "SVG 1.1 profile should include xlink:href completions: {svg11_resp}"
    );
    svg11_server.shutdown_and_exit()?;

    let mut svg2_server = TestServer::start_with_initialize_options(&json!({
        "svg": {
            "profile": "Svg2Draft"
        }
    }))?;
    svg2_server.open(
        "file:///profile-attribute-completion.svg",
        attribute_completion_svg,
    )?;

    let svg2_resp = svg2_server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///profile-attribute-completion.svg" },
            "position": { "line": 0, "character": 22 }
        }),
    )?;
    let svg2_items = svg2_resp["result"]
        .as_array()
        .ok_or("SVG 2 completion result should be an array")?;
    assert!(
        svg2_items
            .iter()
            .any(|item| item["label"].as_str() == Some("href")),
        "SVG 2 profile should include href completions: {svg2_resp}"
    );
    assert!(
        svg2_items
            .iter()
            .all(|item| item["label"].as_str() != Some("xlink:href")),
        "SVG 2 profile should hide unsupported xlink:href completions: {svg2_resp}"
    );
    svg2_server.shutdown_and_exit()?;
    Ok(())
}

/// Switching the active profile on a *single live server* (via
/// `workspace/didChangeConfiguration`) must change completion results for
/// an already-open document — the diagnostics side of this is covered by
/// `profile_config_applies_on_init_and_relints_open_documents`.
#[test]
fn completions_follow_live_profile_switch() -> TestResult {
    let mut server = TestServer::start_with_initialize_options(&json!({
        "svg": {
            "profile": "svg11"
        }
    }))?;

    let uri = "file:///profile-live-switch.svg";
    server.open(uri, r#"<svg><use height="32" /></svg>"#)?;

    let svg11_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 22 }
        }),
    )?;
    let svg11_items = svg11_resp["result"]
        .as_array()
        .ok_or("SVG 1.1 completion result should be an array")?;
    assert!(
        svg11_items
            .iter()
            .any(|item| item["label"].as_str() == Some("xlink:href")),
        "svg11 profile should offer xlink:href before the switch: {svg11_resp}"
    );

    server.change_configuration(&json!({
        "svg": {
            "profile": "svg2draft"
        }
    }))?;

    let svg2_resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 22 }
        }),
    )?;
    let svg2_items = svg2_resp["result"]
        .as_array()
        .ok_or("SVG 2 completion result should be an array")?;
    assert!(
        svg2_items
            .iter()
            .any(|item| item["label"].as_str() == Some("href")),
        "live switch to svg2draft should offer href: {svg2_resp}"
    );
    assert!(
        svg2_items
            .iter()
            .all(|item| item["label"].as_str() != Some("xlink:href")),
        "live switch to svg2draft should drop xlink:href: {svg2_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn value_completions_follow_profile_snapshot_overrides() -> TestResult {
    // SVG 1.1 keeps the CSS2 `display` keywords (`run-in`, `compact`,
    // `marker`) that the union default drops, so the per-snapshot value
    // override must surface for the active profile and disappear otherwise.
    let value_svg = r#"<svg display="inline"></svg>"#;
    let value_col = u32::try_from(value_svg.find("inline").ok_or("display value")? + 1)?;
    let uri = "file:///profile-value-completion.svg";
    let position = json!({ "line": 0, "character": value_col });

    let mut svg11_server = TestServer::start_with_initialize_options(&json!({
        "svg": { "profile": "svg11" }
    }))?;
    svg11_server.open(uri, value_svg)?;
    let svg11_resp = svg11_server.request(
        "textDocument/completion",
        &json!({ "textDocument": { "uri": uri }, "position": position }),
    )?;
    let svg11_items = svg11_resp["result"]
        .as_array()
        .ok_or("SVG 1.1 value completion result should be an array")?;
    assert!(
        svg11_items
            .iter()
            .any(|item| item["label"].as_str() == Some("run-in")),
        "SVG 1.1 profile should surface the `display` override value `run-in`: {svg11_resp}"
    );
    svg11_server.shutdown_and_exit()?;

    let mut svg2_server = TestServer::start_with_initialize_options(&json!({
        "svg": { "profile": "Svg2Draft" }
    }))?;
    svg2_server.open(uri, value_svg)?;
    let svg2_resp = svg2_server.request(
        "textDocument/completion",
        &json!({ "textDocument": { "uri": uri }, "position": position }),
    )?;
    let svg2_items = svg2_resp["result"]
        .as_array()
        .ok_or("SVG 2 value completion result should be an array")?;
    assert!(
        svg2_items
            .iter()
            .any(|item| item["label"].as_str() == Some("inline")),
        "SVG 2 profile should still offer the union `display` values: {svg2_resp}"
    );
    assert!(
        svg2_items
            .iter()
            .all(|item| item["label"].as_str() != Some("run-in")),
        "SVG 2 profile must not surface the SVG 1.1-only `display` value `run-in`: {svg2_resp}"
    );
    svg2_server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn completions_follow_document_version_attribute() -> TestResult {
    // Default server profile is SVG 2. A document declaring
    // `version="1.1"` must auto-swap, so completions for `<use>` show
    // `xlink:href` (SVG 1.1-only) instead of `href` (SVG 2-only).
    let mut server = TestServer::start()?;

    let doc = r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg"><use height="32" /></svg>"#;
    server.open("file:///doc-driven-completion.svg", doc)?;

    // Cursor positioned inside the `<use ...>` tag after `height="32" `.
    // Column 71 is just after the closing quote of height and the space,
    // before `/`. Completion should offer attribute names valid for the
    // active (now SVG 1.1) profile.
    let resp = server.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": "file:///doc-driven-completion.svg" },
            "position": { "line": 0, "character": 71 }
        }),
    )?;
    let items = resp["result"]
        .as_array()
        .ok_or("completion result should be an array")?;
    assert!(
        items
            .iter()
            .any(|item| item["label"].as_str() == Some("xlink:href")),
        "doc-driven SVG 1.1 profile should expose xlink:href in completions: {resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}
