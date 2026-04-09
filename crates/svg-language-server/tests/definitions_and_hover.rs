//! Integration tests for definition and hover behavior.

mod support;

use serde_json::json;
use support::TestServer;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn class_definition_and_hover() -> TestResult {
    let mut server = TestServer::start()?;

    // Build the CSS string without a literal `{fill:red}` in a single token
    // (avoids `literal_string_with_formatting_args` false positive).
    let css_body = "{fill:red}";
    let class_svg =
        format!(r#"<svg><style>.uses-color{css_body}</style><rect class="uses-color"/></svg>"#);
    server.open("file:///class-test.svg", &class_svg)?;

    let class_ref = u32::try_from(
        class_svg
            .rfind("uses-color")
            .ok_or("class reference present")?,
    )? + 2;
    let definition_resp = server.request(
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": "file:///class-test.svg" },
            "position": { "line": 0, "character": class_ref }
        }),
    )?;
    let definition = &definition_resp["result"];
    let expected_class_start =
        u64::try_from(class_svg.find(".uses-color").ok_or("class selector")?)? + 1;
    assert_eq!(
        definition["uri"].as_str(),
        Some("file:///class-test.svg"),
        "definition should stay in the same SVG document: {definition_resp}"
    );
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

    let hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///class-test.svg" },
            "position": { "line": 0, "character": class_ref }
        }),
    )?;
    let hover_text = hover_resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("hover markdown")?;
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
        hover_text.contains("[class-test.svg:1](file:///class-test.svg#L1)"),
        "class hover should provide a clickable source link: {hover_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn custom_property_definition_and_hover() -> TestResult {
    let mut server = TestServer::start()?;

    let vars_svg = [
        "<svg><style>:root { --panel-bg: red; } .var-alpha { fill: var(--panel-bg); }",
        "</style></svg>",
    ]
    .concat();
    server.open("file:///vars-test.svg", &vars_svg)?;

    let var_ref = u32::try_from(vars_svg.find("var(--panel-bg)").ok_or("var reference")?)? + 6;
    let var_definition_resp = server.request(
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": "file:///vars-test.svg" },
            "position": { "line": 0, "character": var_ref }
        }),
    )?;
    let var_definition = &var_definition_resp["result"];
    let expected_var_start = u64::try_from(
        vars_svg
            .find("--panel-bg: red")
            .ok_or("property definition")?,
    )?;
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

    let var_hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///vars-test.svg" },
            "position": { "line": 0, "character": var_ref }
        }),
    )?;
    let var_hover_text = var_hover_resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("custom property hover markdown")?;
    assert!(
        var_hover_text.contains("--panel-bg: red"),
        "custom property hover should include the declaration snippet: {var_hover_resp}"
    );
    assert!(
        var_hover_text.contains("```css"),
        "custom property hover should render the declaration as CSS markdown: {var_hover_resp}"
    );
    assert!(
        var_hover_text.contains("[vars-test.svg:1](file:///vars-test.svg#L1)"),
        "custom property hover should provide a clickable source link: {var_hover_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn element_hover_resolves_catalog_and_external_docs() -> TestResult {
    let mut server = TestServer::start()?;

    let test_svg = r##"<svg><rect fill="#ff0000" stroke="blue"/></svg>"##;
    server.open("file:///test.svg", test_svg)?;

    let element_hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///test.svg" },
            "position": { "line": 0, "character": 7 }
        }),
    )?;
    let element_hover_value = element_hover_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        element_hover_value.contains("MDN Reference"),
        "element hover should contain MDN link: {element_hover_resp}"
    );

    let hover_svg =
        r#"<svg xmlns="http://www.w3.org/2000/svg" xml:lang="en"><path d="M0 0"/></svg>"#;
    server.open("file:///hover.svg", hover_svg)?;

    let d_hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///hover.svg" },
            "position": { "line": 0, "character": 60 }
        }),
    )?;
    let d_hover_value = d_hover_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        d_hover_value.contains("Defines a path to be drawn."),
        "`d` hover should come from the local attribute catalog: {d_hover_resp}"
    );

    let xmlns_hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///hover.svg" },
            "position": { "line": 0, "character": 7 }
        }),
    )?;
    let xmlns_hover_value = xmlns_hover_resp["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("");
    assert!(
        xmlns_hover_value.contains("W3C Namespaces in XML"),
        "`xmlns` hover should use the external namespace reference: {xmlns_hover_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn typed_attribute_hover_resolves_catalog() -> TestResult {
    let mut server = TestServer::start()?;

    let typed_attribute_hover_svg = r#"<svg><defs><clipPath id="clip"><rect width="10" height="10" /></clipPath><linearGradient id="grad"><stop offset="50%" stop-color="red" /></linearGradient><filter id="blur"><feGaussianBlur stdDeviation="3" /></filter></defs><g clip-path="url(#clip)"><line stroke-dasharray="10 5" /></g><text><tspan dx="5">Hello</tspan></text><animate dur="2s" repeatCount="2" /></svg>"#;
    server.open(
        "file:///typed-attribute-hover.svg",
        typed_attribute_hover_svg,
    )?;

    for attribute_name in [
        "stroke-dasharray",
        "offset",
        "stdDeviation",
        "dx",
        "dur",
        "repeatCount",
    ] {
        let character = u32::try_from(
            typed_attribute_hover_svg
                .find(attribute_name)
                .ok_or("typed attribute name present")?,
        )? + 1;
        let hover_resp = server.request(
            "textDocument/hover",
            &json!({
                "textDocument": { "uri": "file:///typed-attribute-hover.svg" },
                "position": { "line": 0, "character": character }
            }),
        )?;

        assert!(
            hover_resp["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "typed attribute hover should resolve for {attribute_name}: {hover_resp}"
        );
    }

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn hover_shows_profile_lifecycle_separately_from_browser_support() -> TestResult {
    let hover_svg =
        r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><use xlink:href="#icon"/></svg>"##;
    let xlink_href_character =
        u32::try_from(hover_svg.find("xlink:href").ok_or("xlink:href present")?)? + 1;

    let mut svg11_server = TestServer::start_with_initialize_options(&json!({
        "svg": {
            "profile": "Svg11"
        }
    }))?;
    svg11_server.open("file:///profile-hover.svg", hover_svg)?;
    let svg11_hover = svg11_server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///profile-hover.svg" },
            "position": { "line": 0, "character": xlink_href_character }
        }),
    )?;
    let svg11_hover_value = svg11_hover["result"]["contents"]["value"]
        .as_str()
        .ok_or("SVG 1.1 hover markdown")?;
    assert!(
        svg11_hover_value.contains("**Obsolete in Svg11Rec20110816**"),
        "SVG 1.1 hover should show the selected profile lifecycle: {svg11_hover}"
    );
    assert!(
        svg11_hover_value.contains("Chrome"),
        "hover should keep the browser support section: {svg11_hover}"
    );
    svg11_server.shutdown_and_exit()?;

    let mut svg2_server = TestServer::start_with_initialize_options(&json!({
        "svg": {
            "profile": "Svg2Draft"
        }
    }))?;
    svg2_server.open("file:///profile-hover.svg", hover_svg)?;
    let svg2_hover = svg2_server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///profile-hover.svg" },
            "position": { "line": 0, "character": xlink_href_character }
        }),
    )?;
    let svg2_hover_value = svg2_hover["result"]["contents"]["value"]
        .as_str()
        .ok_or("SVG 2 hover markdown")?;
    assert!(
        svg2_hover_value.contains("**Obsolete after Svg11Rec20110816**"),
        "SVG 2 hover should show obsolete lifecycle separately from compat: {svg2_hover}"
    );
    assert!(
        svg2_hover_value.contains("Chrome"),
        "hover should still include browser support after lifecycle text: {svg2_hover}"
    );
    svg2_server.shutdown_and_exit()?;

    Ok(())
}
