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
fn hover_renders_per_browser_partial_note_for_color_interpolation() -> TestResult {
    // Regression guard for the per-browser sub-bullet rendering path.
    // `color-interpolation` is chosen because live BCD data has:
    //
    // - Chrome `≤80` + partial_implementation + note "Only the default sRGB"
    // - Edge `≤80`   + partial_implementation + same note
    // - Firefox `≤72`
    // - Safari `≤13.1` + partial_implementation + same note
    //
    // This exercises version qualifier rendering, the partial-implementation
    // sub-bullet path, and the notes pipe from BCD through the catalog.
    let mut server = TestServer::start()?;
    let svg = r#"<svg><rect color-interpolation="sRGB" /></svg>"#;
    server.open("file:///ci.svg", svg)?;

    let col = u32::try_from(svg.find("color-interpolation").ok_or("attr present")?)? + 1;
    let resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///ci.svg" },
            "position": { "line": 0, "character": col }
        }),
    )?;
    let value = resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("color-interpolation hover markdown")?;

    // Qualifier glyph must appear in the chip row — NOT a bare "Chrome 80".
    assert!(
        value.contains("Chrome ≤80"),
        "chip row must render ≤ qualifier: {value}"
    );
    assert!(
        value.contains("Safari ≤13.1"),
        "chip row must render ≤ qualifier on Safari: {value}"
    );
    // Per-browser sub-bullet surfaces the partial-implementation note.
    assert!(
        value.contains("- Chrome: partial"),
        "expected per-browser partial sub-bullet for Chrome: {value}"
    );
    assert!(
        value.contains("sRGB"),
        "partial note text must flow through from BCD notes: {value}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn hover_baseprofile_verdict_is_forbid_in_svg2_profile() -> TestResult {
    // Flagship regression guard for the reconciled-verdict hover path:
    // baseProfile was removed from SVG 2. The old hover contradicted itself
    // by showing both `**Deprecated**` (BCD) and `**Stable in
    // Svg2EditorsDraft20250914**` (snapshot data). After the fix:
    //
    // - data audit removed baseProfile from the SVG 2 snapshot files;
    // - union lifecycle now returns Obsolete;
    // - verdict layer emits Forbid + ProfileObsolete + BcdDeprecated;
    // - hover renders a single coherent `✗ removed from the current SVG
    //   profile` blockquote with a consolidated Status line.
    //
    // This test fails loudly if ANY of those layers regress.
    let mut server = TestServer::start()?;
    let svg = r#"<svg baseProfile="full"></svg>"#;
    server.open("file:///bp.svg", svg)?;

    let col = u32::try_from(svg.find("baseProfile").ok_or("baseProfile present")?)? + 1;
    let resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///bp.svg" },
            "position": { "line": 0, "character": col }
        }),
    )?;
    let value = resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("baseProfile hover markdown")?;

    // Verdict headline must be Forbid (✗) with "removed from the current SVG profile".
    assert!(
        value.contains("\u{2717}") && value.contains("baseProfile"),
        "expected ✗ verdict headline for baseProfile: {value}"
    );
    assert!(
        value.contains("removed from the current SVG profile"),
        "expected 'removed from the current SVG profile' template: {value}"
    );
    // Status line must consolidate all three reasons.
    assert!(
        value.contains("**Status:**") && value.contains("removed after"),
        "expected Status line with 'removed after': {value}"
    );
    assert!(
        value.contains("deprecated"),
        "Status line must include `deprecated` reason: {value}"
    );
    // The old contradictory lines must NOT appear.
    assert!(
        !value.contains("**Stable in"),
        "hover must not show 'Stable in ...' for a forbid-verdict attribute: {value}"
    );
    assert!(
        !value.contains("~~The baseProfile"),
        "hover no longer uses strikethrough description — verdict headline replaces it: {value}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn hover_renders_baseline_qualifier_for_fegaussianblur() -> TestResult {
    // Regression guard for the `≤` qualifier propagation pipeline:
    // BCD → web-features (`baseline_high_date: "≤2021-04-02"`) → worker
    // `/data.json` → svg-data build script → static catalog →
    // svg-language-server hover markdown. If any layer drops the
    // qualifier the hover will render `since 2021` instead of `since ≤2021`.
    let mut server = TestServer::start()?;

    let svg =
        r#"<svg><defs><filter id="blur"><feGaussianBlur stdDeviation="3" /></filter></defs></svg>"#;
    server.open("file:///baseline-qualifier.svg", svg)?;

    let tag_character =
        u32::try_from(svg.find("feGaussianBlur").ok_or("feGaussianBlur present")?)? + 1;
    let hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///baseline-qualifier.svg" },
            "position": { "line": 0, "character": tag_character }
        }),
    )?;

    let hover_value = hover_resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("feGaussianBlur hover markdown")?;
    assert!(
        hover_value.contains("Baseline since ≤"),
        "hover should surface the ≤ qualifier on feGaussianBlur: {hover_resp}"
    );

    server.shutdown_and_exit()?;
    Ok(())
}

#[test]
fn hover_marks_glyph_orientation_horizontal_unsupported_across_chromium_firefox() -> TestResult {
    // Regression guard for the browser-support preservation pipeline:
    // BCD records `version_added: false` for chrome/edge/firefox on the
    // `glyph-orientation-horizontal` attribute. Previously the entire
    // browser_support block was silently dropped by the worker, so the
    // hover line read "Chrome supported | ..." even though BCD said the
    // opposite. After the fix, hover must render `✗` for unsupported
    // engines.
    let mut server = TestServer::start()?;
    let svg = r#"<svg><text glyph-orientation-horizontal="0">x</text></svg>"#;
    server.open("file:///goh.svg", svg)?;

    let attr_character = u32::try_from(
        svg.find("glyph-orientation-horizontal")
            .ok_or("attr present")?,
    )? + 1;
    let hover_resp = server.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": "file:///goh.svg" },
            "position": { "line": 0, "character": attr_character }
        }),
    )?;

    let hover_value = hover_resp["result"]["contents"]["value"]
        .as_str()
        .ok_or("glyph-orientation-horizontal hover markdown")?;
    // At least chrome/firefox/edge must render as unsupported.
    assert!(
        hover_value.contains("Chrome \u{2717}"),
        "hover should mark chrome as unsupported: {hover_value}"
    );
    assert!(
        hover_value.contains("Firefox \u{2717}"),
        "hover should mark firefox as unsupported: {hover_value}"
    );

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
    // In SVG 1.1 profile, xlink:href is defined but BCD-deprecated. The
    // verdict-driven hover shows ⊘ deprecated (not Forbid/removed) because
    // the attribute is still present in the active profile.
    assert!(
        svg11_hover_value.contains("\u{2298}")
            && svg11_hover_value.contains("xlink:href")
            && svg11_hover_value.contains("deprecated"),
        "SVG 1.1 hover for xlink:href should display ⊘ deprecated verdict: {svg11_hover_value}"
    );
    assert!(
        !svg11_hover_value.contains("removed after"),
        "SVG 1.1 verdict must not report xlink:href as removed — it's still defined there: {svg11_hover_value}"
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
    // In SVG 2 profile, xlink:href is no longer defined — verdict escalates
    // to ✗ Forbid with "removed after `Svg11Rec20110816`" in the Status line.
    assert!(
        svg2_hover_value.contains("\u{2717}") && svg2_hover_value.contains("xlink:href"),
        "SVG 2 hover for xlink:href should display ✗ Forbid verdict: {svg2_hover_value}"
    );
    assert!(
        svg2_hover_value.contains("removed after `Svg11Rec20110816`"),
        "SVG 2 hover should surface the removal point in the Status line: {svg2_hover_value}"
    );
    assert!(
        svg2_hover_value.contains("Chrome"),
        "hover should still include browser support after lifecycle text: {svg2_hover}"
    );
    svg2_server.shutdown_and_exit()?;

    Ok(())
}
