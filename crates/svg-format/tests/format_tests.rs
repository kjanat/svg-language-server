//! Integration tests for the svg-format crate.

use svg_format::{
    AttributeLayout, AttributeSort, BlankLines, EmbeddedLanguage, FormatOptions, QuoteStyle,
    TextContentMode, WrappedAttributeIndent, format_with_host, format_with_options,
};

/// Pre-v0.4.0 defaults: tab indent + OneLevel wrapped-attribute indent.
/// Existing assertions were authored against these, so they continue to
/// use this helper while new tests exercise the canonical spaces /
/// AlignToTagName defaults.
fn legacy_tab_options() -> FormatOptions {
    FormatOptions {
        insert_spaces: false,
        wrapped_attribute_indent: WrappedAttributeIndent::OneLevel,
        ..FormatOptions::default()
    }
}

fn format(input: &str) -> String {
    format_with_options(input, legacy_tab_options())
}

#[test]
fn formats_nested_elements() {
    let input = r"<svg><g><rect/></g></svg>";
    let expected = "<svg>\n\t<g>\n\t\t<rect />\n\t</g>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn formats_multiline_attributes_consistently() {
    let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%" x2="0%" y2="100%"></linearGradient></svg>"#;
    let options = FormatOptions {
        max_inline_tag_width: 24,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<linearGradient\n\t\tid=\"sky\"\n\t\tx1=\"0%\"\n\t\ty1=\"0%\"\n\t\tx2=\"0%\"\n\t\ty2=\"100%\">\n\t</linearGradient>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn canonical_attribute_ordering() {
    let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
    let expected = "<svg>\n\t<rect id=\"x\" class=\"hero\" x=\"1\" y=\"2\" width=\"4\" height=\"5\" />\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn preserves_style_block_content_shape() {
    let input = "<svg><style>\n  .a { fill: red; }\n    .b { stroke: blue; }\n</style></svg>";
    let expected =
        "<svg>\n\t<style>\n\t\t.a { fill: red; }\n\t\t  .b { stroke: blue; }\n\t</style>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn text_content_collapse_applies_to_embedded_style_without_host() {
    let input = "<svg><style>\n  .a   {   fill: red; }\n</style></svg>";
    let options = FormatOptions {
        text_content: TextContentMode::Collapse,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<style>\n\t\t.a { fill: red; }\n\t</style>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn attribute_sort_none_preserves_input_order() {
    let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
    let options = FormatOptions {
        attribute_sort: AttributeSort::None,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect y=\"2\" width=\"4\" class=\"hero\" id=\"x\" x=\"1\" height=\"5\" />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn attribute_sort_alphabetical_orders_by_name() {
    let input = r#"<svg><rect y="2" width="4" class="hero" id="x" x="1" height="5"/></svg>"#;
    let options = FormatOptions {
        attribute_sort: AttributeSort::Alphabetical,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect class=\"hero\" height=\"5\" id=\"x\" width=\"4\" x=\"1\" y=\"2\" />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn quote_style_double_normalizes_quotes() {
    let input = r"<svg><rect class='hero' id='x'/></svg>";
    let options = FormatOptions {
        quote_style: QuoteStyle::Double,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect id=\"x\" class=\"hero\" />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn quote_style_single_normalizes_quotes() {
    let input = r#"<svg><rect class="hero" id="x"/></svg>"#;
    let options = FormatOptions {
        quote_style: QuoteStyle::Single,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect id='x' class='hero' />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn attribute_layout_single_line_ignores_width_trigger() {
    let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%" x2="0%" y2="100%"></linearGradient></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::SingleLine,
        max_inline_tag_width: 10,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<linearGradient id=\"sky\" x1=\"0%\" y1=\"0%\" x2=\"0%\" y2=\"100%\">\n\t</linearGradient>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn space_before_self_close_false_removes_spacing() {
    let input = r#"<svg><rect id="x"/></svg>"#;
    let options = FormatOptions {
        space_before_self_close: false,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect id=\"x\"/>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn wrapped_attribute_indent_align_to_tag_name() {
    // Under AlignToTagName with canonical sort, attributes partition
    // into canonical groups — Identity (`id`) rides the tag line, and
    // the Geometry group (`x1`, `y1`) wraps as one line aligned under
    // `<tag `.
    let input = r#"<svg><linearGradient id="sky" x1="0%" y1="0%"></linearGradient></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
        ..legacy_tab_options()
    };
    let aligned = format!("\t{}", " ".repeat("linearGradient".len() + 2));
    let expected = format!(
        "<svg>\n\t<linearGradient id=\"sky\"\n{aligned}x1=\"0%\" y1=\"0%\">\n\t</linearGradient>\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn canonical_sort_xmlns_trails_before_version() {
    // W3 SVG convention: geometry attrs, then xmlns, then version at the
    // very end of the root <svg> tag.
    let input =
        r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
    let expected = "<svg width=\"10\" height=\"10\" xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\">\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn canonical_sort_xmlns_xlink_sibling_keeps_plain_xmlns_first() {
    // Plain `xmlns` before `xmlns:*` within the namespace group, version
    // strictly last. Two-level group key (3, 0) vs (3, 1) orders them.
    let input = r#"<svg version="1.1" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns="http://www.w3.org/2000/svg"></svg>"#;
    let expected = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" version=\"1.1\">\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn align_to_tag_name_keeps_first_attribute_inline_with_tag() {
    // Input has 6 attributes across three canonical groups: Identity
    // (`id`), Geometry (`x`, `y`, `width`, `height`), Presentation
    // (`fill`). Each group wraps onto its own line; the Identity group
    // rides the tag line.
    let input = r#"<svg><rect id="box" x="10" y="20" width="100" height="50" fill="red"/></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
        ..legacy_tab_options()
    };
    let aligned = format!("\t{}", " ".repeat("rect".len() + 2));
    let expected = format!(
        "<svg>\n\t<rect id=\"box\"\n{aligned}x=\"10\" y=\"20\" width=\"100\" height=\"50\"\n{aligned}fill=\"red\" />\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn align_to_tag_name_with_multiline_value_continues_under_quote() {
    // The d attribute has an embedded newline. Under AlignToTagName, the
    // path stays inline with `<path`, its continuation aligns under the
    // opening quote (column of `d="` plus 1), and `fill` wraps under `d`.
    let input = "<svg><path d=\"M0 0\n     L1 1\" fill=\"red\" /></svg>";
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
        ..legacy_tab_options()
    };
    // wrapped_prefix = "\t      " (tab + 6 spaces for "<path ").
    // Continuation pad = wrapped_prefix + 3 spaces for `d="`.
    let aligned = format!("\t{}", " ".repeat("path".len() + 2));
    let cont = format!("{aligned}   "); // + 3 spaces for `d="`.
    let expected =
        format!("<svg>\n\t<path d=\"M0 0\n{cont}L1 1\"\n{aligned}fill=\"red\" />\n</svg>");
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn parse_error_returns_original_source() {
    let input = r#"<svg><path d="m0 0 l"/></svg>"#;
    assert_eq!(format(input), input);
}

#[test]
fn text_content_maintain_preserves_relative_indentation() {
    let input = "<svg><text>\n  hello\n    world\n</text></svg>";
    let options = FormatOptions {
        text_content: TextContentMode::Maintain,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<text>\n\t\thello\n\t\t  world\n\t</text>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn text_content_maintain_keeps_blank_lines_with_default_truncate() {
    let input = "<svg><text>\nalpha\n\n\nbeta\n</text></svg>";
    let expected = "<svg>\n\t<text>\n\t\talpha\n\t\t\n\t\t\n\t\tbeta\n\t</text>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn text_content_maintain_ignores_blank_line_remove_mode() {
    let input = "<svg><text>\nalpha\n\n\nbeta\n</text></svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Remove,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<text>\n\t\talpha\n\t\t\n\t\t\n\t\tbeta\n\t</text>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn text_content_collapse_collapses_whitespace() {
    let input = "<svg><text>\n  hello   world  \n    foo    bar  \n</text></svg>";
    let options = FormatOptions {
        text_content: TextContentMode::Collapse,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<text>\n\t\thello world\n\t\tfoo bar\n\t</text>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn text_content_prettify_trims_and_reindents() {
    let input = "<svg><text>\n  hello  \n    world  \n</text></svg>";
    let options = FormatOptions {
        text_content: TextContentMode::Prettify,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<text>\n\t\thello\n\t\tworld\n\t</text>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn text_content_default_is_maintain() {
    assert_eq!(legacy_tab_options().text_content, TextContentMode::Maintain);
}

#[test]
fn format_with_host_delegates_style_content() {
    let css_body = "{fill:red}";
    let input = format!("<svg><style>.a{css_body}</style></svg>");
    let input = input.as_str();
    let mut called_lang = None;
    let mut called_content = None;
    let result = format_with_host(input, legacy_tab_options(), &mut |req| {
        called_lang = Some(req.language);
        called_content = Some(req.content.to_string());
        Some(".a {\n  fill: red;\n}".to_string())
    });
    assert_eq!(called_lang, Some(EmbeddedLanguage::Css));
    let expected_css = [".a", css_body].concat();
    assert_eq!(called_content.as_deref(), Some(expected_css.as_str()));
    assert_eq!(
        result,
        "<svg>\n\t<style>\n\t\t.a {\n\t\t  fill: red;\n\t\t}\n\t</style>\n</svg>"
    );
}

#[test]
fn multiline_path_value_continuation_aligns_under_opening_quote() {
    // W3 SVG path samples break long `d="..."` values across lines to keep
    // logical path-command groups visible. Each continuation line aligns
    // to the column directly after `d="`, not to the attribute-wrap indent.
    // Before this fix, svg-format preserved the raw newlines but left each
    // continuation at its original source indentation, so the output
    // depended on how the author happened to pad the source.
    let input = "<svg><path d=\"M100,200 C100,100 250,100 250,200\n                              S400,300 400,200\" /></svg>";
    let result = format(input);
    let expected = concat!(
        "<svg>\n",
        "\t<path\n",
        "\t\td=\"M100,200 C100,100 250,100 250,200\n",
        "\t\t   S400,300 400,200\" />\n",
        "</svg>",
    );
    assert_eq!(result, expected);
}

#[test]
fn multiline_value_continuation_respects_wrapped_prefix_column() {
    // AlignToTagName wrap puts the attribute name at column = indent + "<tag ".
    // Continuation of a multi-line value must still align under the opening
    // quote — i.e. at (wrapped_prefix + name + 2), independent of which
    // wrap style is chosen.
    let input = r#"<svg><path d="M 0 0
         L 10 10" fill="red" /></svg>"#;
    let options = FormatOptions {
        wrapped_attribute_indent: WrappedAttributeIndent::AlignToTagName,
        ..legacy_tab_options()
    };
    let result = format_with_options(input, options);
    // Under AlignToTagName at depth=1, wrapped_prefix = "\t      " (1 tab +
    // 6 spaces for "<path "). Continuation pad = prefix + spaces for
    // `d=` + opening quote = 1 tab + 6 spaces + 3 spaces = 1 tab + 9 spaces.
    let lines: Vec<&str> = result.lines().collect();
    let Some(cont) = lines.iter().find(|l| l.contains("L 10 10")) else {
        panic!("no continuation line found in:\n{result}");
    };
    let leading: String = cont.chars().take_while(|c| c.is_whitespace()).collect();
    assert_eq!(
        leading, "\t         ",
        "continuation must mirror wrapped_prefix's indent style and extend to the opening-quote column, got: {cont:?}"
    );
}

#[test]
fn format_with_host_unwraps_cdata_style_before_delegating() {
    // W3 path samples wrap stylesheets in CDATA so CSS `>` / `&` can't
    // confuse the XML parser. The host CSS formatter rejects `<![CDATA[`
    // at column 0 as a syntax error — we must peel the markers before
    // handing content off, and re-wrap on the way out.
    let css = ".a{fill:red}";
    let input = format!("<svg><style><![CDATA[{css}]]></style></svg>");
    let mut received = None;
    let result = format_with_host(&input, legacy_tab_options(), &mut |req| {
        received = Some(req.content.to_string());
        Some(".a {\n  fill: red;\n}".to_string())
    });

    assert_eq!(
        received.as_deref(),
        Some(css),
        "host formatter must see CSS only — no CDATA markers",
    );
    assert_eq!(
        result,
        "<svg>\n\t<style>\n\t\t<![CDATA[\n\t\t\t.a {\n\t\t\t  fill: red;\n\t\t\t}\n\t\t]]>\n\t</style>\n</svg>",
        "CDATA wrapper must be preserved in the output",
    );
}

#[test]
fn format_with_host_preserves_ampersand_inside_cdata() {
    // Entities inside CDATA are literal, not escaped. Without CDATA, `&`
    // must be emitted as `&amp;`; inside CDATA, it stays raw. The fix
    // skips `decode_xml_entities`/`encode_xml_entities` when CDATA is
    // detected so we don't mangle content like `content: "a & b"`.
    let input = r#"<svg><style><![CDATA[.a::before{content:"a & b"}]]></style></svg>"#;
    let result = format_with_host(input, legacy_tab_options(), &mut |req| {
        Some(req.content.to_string())
    });
    assert!(
        result.contains("a & b"),
        "raw ampersand must survive round-trip inside CDATA, got: {result}"
    );
    assert!(
        !result.contains("&amp;"),
        "must not re-encode `&` inside CDATA: {result}"
    );
}

#[test]
fn format_with_host_falls_back_when_callback_returns_none() {
    let input = "<svg><style>.a { fill: red; }</style></svg>";
    let result = format_with_host(input, legacy_tab_options(), &mut |_| None);
    let fallback = format_with_options(input, legacy_tab_options());
    assert_eq!(result, fallback);
}

#[test]
fn format_with_host_delegates_script_content() {
    let input = "<svg><script>alert(1)</script></svg>";
    let mut called_lang = None;
    let _ = format_with_host(input, legacy_tab_options(), &mut |req| {
        called_lang = Some(req.language);
        None
    });
    assert_eq!(called_lang, Some(EmbeddedLanguage::JavaScript));
}

#[test]
fn format_with_host_decodes_entities_for_script() {
    let input = r"<svg><script>for (let i = 0; i &lt; n; i++) {}</script></svg>";
    let mut received = None;
    let result = format_with_host(input, legacy_tab_options(), &mut |req| {
        received = Some(req.content.to_string());
        Some(req.content.to_string())
    });
    // Callback receives decoded JS with bare <
    assert_eq!(received.as_deref(), Some("for (let i = 0; i < n; i++) {}"));
    // Output re-encodes back to XML entities
    assert!(result.contains("&lt;"), "output must re-encode < as &lt;");
}

#[test]
fn format_with_host_round_trips_multiple_entities() {
    let input = r"<svg><script>if (a &lt; b &amp;&amp; c &gt; d) {}</script></svg>";
    let mut received = None;
    let result = format_with_host(input, legacy_tab_options(), &mut |req| {
        received = Some(req.content.to_string());
        Some(req.content.to_string())
    });
    assert_eq!(received.as_deref(), Some("if (a < b && c > d) {}"));
    assert!(result.contains("&lt;"), "< must be re-encoded");
    assert!(result.contains("&gt;"), "> must be re-encoded");
    assert!(result.contains("&amp;"), "& must be re-encoded");
}

#[test]
fn format_with_host_delegates_foreign_object_content() {
    let input =
        r#"<svg><foreignObject width="200" height="200"><div>hello</div></foreignObject></svg>"#;
    let mut called_lang = None;
    let mut called_content = None;
    let _ = format_with_host(input, legacy_tab_options(), &mut |req| {
        called_lang = Some(req.language);
        called_content = Some(req.content.to_string());
        None
    });
    assert_eq!(called_lang, Some(EmbeddedLanguage::Html));
    assert!(
        called_content
            .as_deref()
            .is_some_and(|c| c.contains("<div>hello</div>"))
    );
}

#[test]
fn format_with_host_foreign_object_with_formatted_html() {
    let input =
        r#"<svg><foreignObject width="200" height="200"><div>hello</div></foreignObject></svg>"#;
    let result = format_with_host(input, legacy_tab_options(), &mut |req| {
        if req.language == EmbeddedLanguage::Html {
            Some("<div>\n  hello\n</div>".to_string())
        } else {
            None
        }
    });
    assert_eq!(
        result,
        "<svg>\n\t<foreignObject width=\"200\" height=\"200\">\n\t\t<div>\n\t\t  hello\n\t\t</div>\n\t</foreignObject>\n</svg>"
    );
}

#[test]
fn blank_lines_remove_strips_all_gaps() {
    let input = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Remove,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\t<!--legend-->\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_preserve_keeps_source_gaps() {
    let input = "<svg>\n\t<rect />\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Preserve,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_truncate_collapses_multiple() {
    let input = "<svg>\n\t<rect />\n\n\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Truncate,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_truncate_keeps_single() {
    let input = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Truncate,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_insert_adds_gaps() {
    let input = "<svg><rect/><circle/></svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_insert_comments_attach_downward() {
    let input = "<svg><rect/><!--legend--><circle/></svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\t<!--legend-->\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_insert_normalizes_multiple_to_one() {
    let input = "<svg>\n\t<rect />\n\n\n\n\t<circle />\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let expected = "<svg>\n\t<rect />\n\n\t<circle />\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_default_is_truncate() {
    assert_eq!(legacy_tab_options().blank_lines, BlankLines::Truncate);
}

#[test]
fn blank_lines_truncate_collapses_inside_script() {
    // Two blank lines between functions → collapsed to one.
    let input = "<svg>\n<script>\nfunction a() {}\n\n\nfunction b() {}\n</script>\n</svg>";
    let expected =
        "<svg>\n\t<script>\n\t\tfunction a() {}\n\n\t\tfunction b() {}\n\t</script>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn blank_lines_truncate_collapses_inside_style() {
    let input = "<svg>\n<style>\n.a { fill: red; }\n\n\n.b { stroke: blue; }\n</style>\n</svg>";
    let expected =
        "<svg>\n\t<style>\n\t\t.a { fill: red; }\n\n\t\t.b { stroke: blue; }\n\t</style>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn blank_lines_remove_strips_inside_script() {
    let input = "<svg>\n<script>\nfunction a() {}\n\n\nfunction b() {}\n</script>\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Remove,
        ..legacy_tab_options()
    };
    let expected =
        "<svg>\n\t<script>\n\t\tfunction a() {}\n\t\tfunction b() {}\n\t</script>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_preserve_keeps_inside_script() {
    let input = "<svg>\n<script>\nfunction a() {}\n\n\nfunction b() {}\n</script>\n</svg>";
    let options = FormatOptions {
        blank_lines: BlankLines::Preserve,
        ..legacy_tab_options()
    };
    let expected =
        "<svg>\n\t<script>\n\t\tfunction a() {}\n\n\n\t\tfunction b() {}\n\t</script>\n</svg>";
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn blank_lines_truncate_collapses_in_host_formatted_block() {
    let css_body = "{fill:red}";
    let input = format!("<svg><style>.a{css_body}</style></svg>");
    let input = input.as_str();
    let result = format_with_host(input, legacy_tab_options(), &mut |_| {
        Some(".a {\n  fill: red;\n}\n\n\n.b {\n  stroke: blue;\n}".to_string())
    });
    // Host returned 2 blank lines between rules → collapsed to 1.
    assert_eq!(
        result,
        "<svg>\n\t<style>\n\t\t.a {\n\t\t  fill: red;\n\t\t}\n\n\t\t.b {\n\t\t  stroke: blue;\n\t\t}\n\t</style>\n</svg>"
    );
}

#[test]
fn host_formatted_block_strips_leading_trailing_blanks() {
    let css_body = "{fill:red}";
    let input = format!("<svg><style>.a{css_body}</style></svg>");
    let input = input.as_str();
    // Host returns content with leading and trailing blank lines.
    let result = format_with_host(input, legacy_tab_options(), &mut |_| {
        Some("\n\n.a {\n  fill: red;\n}\n\n".to_string())
    });
    // Leading/trailing blanks must be stripped — no blank line between
    // <style> and the first rule, or between the last rule and </style>.
    assert_eq!(
        result,
        "<svg>\n\t<style>\n\t\t.a {\n\t\t  fill: red;\n\t\t}\n\t</style>\n</svg>"
    );
}

#[test]
fn ignore_file_skips_formatting() {
    let input = "<svg><rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-file -->\n</svg>";
    assert_eq!(format(input), input);
}

#[test]
fn ignore_next_skips_one_sibling() {
    let input = "<svg>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<circle cx=\"1\" cy=\"2\" r=\"3\"/>\n</svg>";
    let result = format(input);
    assert!(result.contains("y=\"2\" x=\"1\""));
    assert!(result.contains("<circle cx=\"1\" cy=\"2\" r=\"3\" />"));
}

#[test]
fn ignore_range_preserves_content() {
    let input = "<svg>\n<rect id=\"a\"/>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<!-- svg-format-ignore-end -->\n<rect y=\"2\" x=\"1\" id=\"b\"/>\n</svg>";
    let result = format(input);
    assert!(result.contains("<rect y=\"2\" x=\"1\"/>"));
    assert!(result.contains("<circle r=\"3\" cx=\"1\" cy=\"2\"/>"));
    assert!(result.contains("<rect id=\"b\" x=\"1\" y=\"2\" />"));
}

#[test]
fn custom_ignore_prefix_works() {
    let input = "<svg><!-- custom-ignore-file --><rect/></svg>";
    let options = FormatOptions {
        ignore_prefixes: vec!["custom".to_string()],
        ..legacy_tab_options()
    };
    assert_eq!(format_with_options(input, options), input);
}

#[test]
fn ignore_file_only_matches_comments_not_text() {
    let input = "<svg><text>svg-format-ignore-file</text></svg>";
    let result = format(input);
    assert_ne!(result, input);
}

#[test]
fn ignore_range_preserves_gaps_verbatim() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
    let result = format(input);
    assert!(result.contains("<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>"));
}

#[test]
fn ignore_next_preserves_inline_text() {
    let input = "<svg>\n<!-- svg-format-ignore -->\n<text>  spaced  </text>\n</svg>";
    let result = format(input);
    assert!(result.contains("<text>  spaced  </text>"));
}

#[test]
fn ignore_range_outside_svg_with_blank_lines_is_idempotent() {
    let input = "\
</svg>
<!-- dprint-ignore-start -->
<!-- comment A -->

<!-- comment B -->
<!-- comment C -->

<!-- comment D -->
<!-- comment E -->
<!-- comment F -->

<!-- comment G -->
<!-- comment H -->
<!-- comment I -->
<!-- comment J -->
<!-- dprint-ignore-end -->
";
    let opts = FormatOptions {
        ignore_prefixes: vec!["dprint".into()],
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let pass1 = format_with_options(input, opts.clone());
    let pass2 = format_with_options(&pass1, opts);
    assert_eq!(
        pass1, pass2,
        "not idempotent:\n--- pass1:\n{pass1}\n--- pass2:\n{pass2}"
    );
}

#[test]
fn ignore_range_inside_svg_with_blank_lines_is_idempotent() {
    let input = "\
<svg>
\t<rect />
\t<!-- dprint-ignore-start -->
\t<rect y=\"2\" x=\"1\"/>

\t<circle r=\"3\"/>
\t<!-- dprint-ignore-end -->
\t<rect />
</svg>
";
    let opts = FormatOptions {
        ignore_prefixes: vec!["dprint".into()],
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let pass1 = format_with_options(input, opts.clone());
    let pass2 = format_with_options(&pass1, opts);
    assert_eq!(
        pass1, pass2,
        "not idempotent:\n--- pass1:\n{pass1}\n--- pass2:\n{pass2}"
    );
}

#[test]
fn ignore_range_preserves_exact_source_bytes() {
    let input = "\
<svg>
<!-- dprint-ignore-start -->
<rect y=\"2\"
      x=\"1\"/>

<circle r=\"3\"/>
<!-- dprint-ignore-end -->
</svg>
";
    let opts = FormatOptions {
        ignore_prefixes: vec!["dprint".into()],
        ..legacy_tab_options()
    };
    let result = format_with_options(input, opts);
    assert!(
        result.contains("<rect y=\"2\"\n      x=\"1\"/>\n\n<circle r=\"3\"/>"),
        "source bytes not preserved:\n{result}"
    );
}

#[test]
fn ignore_range_with_insert_blank_lines_is_stable() {
    let input = "\
<svg>
\t<rect />
\t<!-- dprint-ignore-start -->
\t<rect y=\"2\" x=\"1\"/>
\t<circle r=\"3\"/>
\t<!-- dprint-ignore-end -->
\t<rect />
</svg>
";
    let opts = FormatOptions {
        ignore_prefixes: vec!["dprint".into()],
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let pass1 = format_with_options(input, opts);
    assert!(
        pass1.contains("<rect y=\"2\" x=\"1\"/>\n\t<circle r=\"3\"/>"),
        "insert mode modified ignored content:\n{pass1}"
    );
}

// ── Edge-case ignore directive tests ────────────────────────────

#[test]
fn two_consecutive_ignore_next_skip_two_siblings() {
    let input = "<svg>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "first ignored element was formatted:\n{result}"
    );
    assert!(
        result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
        "second ignored element was formatted:\n{result}"
    );
    assert!(
        result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
        "non-ignored element was not formatted:\n{result}"
    );
}

#[test]
fn ignore_end_without_start_is_harmless() {
    let input = "<svg>\n<!-- svg-format-ignore-end -->\n<rect y=\"2\" x=\"1\"/>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("<rect x=\"1\" y=\"2\" />"),
        "formatting was suppressed by stray ignore-end:\n{result}"
    );
}

#[test]
fn ignore_start_without_end_ignores_rest_of_siblings() {
    let input = "<svg>\n<rect id=\"a\"/>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("<rect id=\"a\" />"),
        "element before ignore-start was not formatted:\n{result}"
    );
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "element after unclosed ignore-start was formatted:\n{result}"
    );
    assert!(
        result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
        "element after unclosed ignore-start was formatted:\n{result}"
    );
}

#[test]
fn nested_ignore_start_is_harmless() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-start -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n<!-- svg-format-ignore-end -->\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "inner content was formatted:\n{result}"
    );
    assert!(
        result.contains("r=\"3\" cx=\"1\" cy=\"2\""),
        "inner content was formatted:\n{result}"
    );
    assert!(
        result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
        "element after ignore-end was not formatted:\n{result}"
    );
}

#[test]
fn ignore_next_inside_ignore_range_is_preserved_verbatim() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
    let result = format(input);
    assert!(
        result.contains("<!-- svg-format-ignore -->"),
        "inner directive was stripped:\n{result}"
    );
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "inner content was formatted:\n{result}"
    );
}

#[test]
fn ignore_next_inside_range_does_not_leak_after_end() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n<ellipse ry=\"1\" rx=\"2\"/>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("<ellipse rx=\"2\" ry=\"1\" />"),
        "ignore_next leaked past ignore-end:\n{result}"
    );
}

#[test]
fn ignore_directives_work_inside_nested_elements() {
    let input = "<svg>\n<g>\n<!-- svg-format-ignore -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</g>\n</svg>";
    let result = format(input);
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "ignored element inside <g> was formatted:\n{result}"
    );
    assert!(
        result.contains("<circle cx=\"1\" cy=\"2\" r=\"3\" />"),
        "non-ignored element inside <g> was not formatted:\n{result}"
    );
}

#[test]
fn ignore_next_with_insert_puts_blank_line_before_comment() {
    let input =
        "<svg><rect/>\n<!-- svg-format-ignore -->\n<circle r=\"3\" cx=\"1\" cy=\"2\"/>\n</svg>";
    let opts = FormatOptions {
        blank_lines: BlankLines::Insert,
        ..legacy_tab_options()
    };
    let result = format_with_options(input, opts);
    assert!(
        result.contains("<rect />\n\n\t<!-- svg-format-ignore -->"),
        "no blank line before ignore comment:\n{result}"
    );
    assert!(
        result.contains("<!-- svg-format-ignore -->\n<circle"),
        "blank line inserted between comment and ignored element:\n{result}"
    );
}

#[test]
fn ignore_file_inside_nested_element_still_skips_file() {
    let input =
        "<svg>\n<g>\n<!-- svg-format-ignore-file -->\n<rect y=\"2\" x=\"1\"/>\n</g>\n</svg>";
    assert_eq!(
        format(input),
        input,
        "ignore-file inside nested element did not skip formatting"
    );
}

#[test]
fn ignore_range_as_first_child_after_start_tag() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
    let result = format(input);
    assert!(
        result.contains("y=\"2\" x=\"1\""),
        "ignore range lost content when prev_end was None:\n{result}"
    );
}

#[test]
fn ignore_range_first_content_child_not_lost() {
    let input = "<svg>\n<!-- svg-format-ignore-start -->\n<rect y=\"2\" x=\"1\"/>\n<circle r=\"3\"/>\n<!-- svg-format-ignore-end -->\n</svg>";
    let result = format(input);
    assert!(
        result.contains("<rect y=\"2\" x=\"1\"/>"),
        "first element inside range was lost:\n{result}"
    );
    assert!(
        result.contains("<circle r=\"3\"/>"),
        "second element inside range was lost:\n{result}"
    );
}

// ── Text-content element whitespace sensitivity ──────────────

#[test]
fn text_element_entity_refs_stay_inline() {
    let input =
        r#"<svg><text class="label" x="36" y="58">Embedded &lt;style&gt; colors</text></svg>"#;
    let expected = "<svg>\n\t<text class=\"label\" x=\"36\" y=\"58\">Embedded &lt;style&gt; colors</text>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn text_element_entity_refs_idempotent() {
    let input =
        r#"<svg><text class="label" x="36" y="58">Embedded &lt;style&gt; colors</text></svg>"#;
    let once = format(input);
    let twice = format(&once);
    assert_eq!(once, twice, "not idempotent:\n{once}");
}

#[test]
fn text_element_broken_entity_refs_repaired() {
    let input = "<svg>\n<text class=\"label\" x=\"36\" y=\"58\">\n\tEmbedded\n\t&lt;\n\tstyle\n\t&gt;\n\tcolors\n</text>\n</svg>";
    let expected = "<svg>\n\t<text class=\"label\" x=\"36\" y=\"58\">Embedded &lt;style&gt; colors</text>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn text_element_comparison_entity_refs_keep_spaces() {
    let input = "<svg><text>a &lt; b &gt; c</text></svg>";
    let expected = "<svg>\n\t<text>a &lt; b &gt; c</text>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn desc_element_entity_ref_stays_inline() {
    let input = "<svg><desc>A &amp; B</desc></svg>";
    let expected = "<svg>\n\t<desc>A &amp; B</desc>\n</svg>";
    assert_eq!(format(input), expected);
}

#[test]
fn text_element_long_entity_ref_content_wraps_to_own_line() {
    let input = "<svg><text class=\"subtle\" x=\"36\" y=\"84\">hex, rgb(a), hsl(a), hwb, lab, lch, oklab, oklch, transparent, stop-color, CSS vars, and color-mix() &amp; more</text></svg>";
    let result = format(input);
    assert!(
        result.contains(">\n\t\thex, rgb(a)"),
        "long content not on own line:\n{result}"
    );
    assert_eq!(
        result.lines().filter(|l| l.contains("hex, rgb(a)")).count(),
        1,
        "content split across multiple lines:\n{result}"
    );
}

#[test]
fn tspan_entity_refs_stay_inline() {
    let input = "<svg><text><tspan>a &amp; b</tspan></text></svg>";
    let result = format(input);
    assert!(
        result.contains("<tspan>a &amp; b</tspan>"),
        "tspan content was split:\n{result}"
    );
}

#[test]
fn text_content_maintain_with_entities_still_normalizes() {
    let input = "<svg>\n<text>\n\tEmbedded\n\t&lt;\n\tstyle\n\t&gt;\n\tcolors\n</text>\n</svg>";
    let options = FormatOptions {
        text_content: TextContentMode::Maintain,
        ..legacy_tab_options()
    };
    let result = format_with_options(input, options);
    assert_eq!(
        result,
        "<svg>\n\t<text>Embedded &lt;style&gt; colors</text>\n</svg>"
    );
}

#[test]
fn output_is_pure_lf_when_parse_fails() {
    // Tree-sitter-svg cannot parse this malformed fragment, so the formatter
    // falls back to the passthrough path. The output must still be pure LF
    // (no \r) so downstream callers that translate newlines can't double-up
    // CRs that the source happened to contain.
    let crlf = "<svg>\r\n<unclosed \r\n</svg>\r\n";
    let out = format(crlf);
    assert!(!out.contains('\r'), "expected pure LF, got: {out:?}");
    assert_eq!(out, "<svg>\n<unclosed \n</svg>\n");
}

#[test]
fn output_is_pure_lf_on_ignore_file() {
    // With an ignore-file directive the formatter returns source verbatim.
    // Same invariant applies — normalize CRLF to LF before handing back.
    let crlf = "<!-- svg-format-ignore-file -->\r\n<svg><rect/></svg>\r\n";
    let out = format(crlf);
    assert!(!out.contains('\r'), "expected pure LF, got: {out:?}");
    assert_eq!(out, "<!-- svg-format-ignore-file -->\n<svg><rect/></svg>\n");
}

// --- group-based wrap regressions (v0.4.0) --------------------------------

#[test]
fn group_wrap_rect_splits_geometry_from_presentation() {
    // `<rect>` with Geometry + Presentation attrs wraps with each
    // canonical group on its own line under AlignToTagName.
    let input =
        r#"<svg><rect x="10" y="20" width="100" height="50" fill="red" stroke="black"/></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        ..FormatOptions::default()
    };
    let aligned = format!("  {}", " ".repeat("rect".len() + 2));
    let expected = format!(
        "<svg>\n  <rect x=\"10\" y=\"20\" width=\"100\" height=\"50\"\n{aligned}fill=\"red\" stroke=\"black\" />\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn group_wrap_path_drawing_then_presentation() {
    // `<path>` with Drawing (`d`) + Presentation attrs: Drawing group
    // rides the tag line; Presentation wraps on the next line.
    let input = r#"<svg><path d="M0 0 L1 1" fill="none" stroke="black" stroke-width="2"/></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        ..FormatOptions::default()
    };
    let aligned = format!("  {}", " ".repeat("path".len() + 2));
    let expected = format!(
        "<svg>\n  <path d=\"M0 0 L1 1\"\n{aligned}fill=\"none\" stroke=\"black\" stroke-width=\"2\" />\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn group_wrap_svg_geometry_then_namespace_then_version() {
    // Root `<svg>` with Geometry + Namespace + Version wraps as three
    // lines: geometry on tag line, xmlns next, version last.
    let input = r#"<svg width="800" height="600" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1"></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        ..FormatOptions::default()
    };
    let aligned = " ".repeat("svg".len() + 2);
    let expected = format!(
        "<svg width=\"800\" height=\"600\"\n{aligned}xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"\n{aligned}version=\"1.1\">\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn path_data_wraps_at_subpath_boundaries_when_over_width() {
    // Minified `d` value with two subpaths exceeds `max_inline_tag_width`,
    // so the formatter breaks at the second `M` (moveto) boundary. The
    // `/>` closer appends to the final continuation line.
    let input = r#"<svg><path d="M0 0 L10 10 M20 20 L30 30"/></svg>"#;
    let options = FormatOptions {
        max_inline_tag_width: 30,
        ..FormatOptions::default()
    };
    // wrapped_prefix = 2-space indent + "path " width (6) = 8 chars.
    // continuation pad = wrapped_prefix + `d="` width (3) = 11 chars.
    let pad = " ".repeat(8 + "d=\"".len());
    let expected = format!("<svg>\n  <path d=\"M0 0 L10 10\n{pad}M20 20 L30 30\" />\n</svg>");
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn path_data_packs_multiple_segments_per_line_until_budget() {
    // With a generous budget, several segments pack onto one line.
    // With a tighter budget, the same input forces more line breaks.
    let input = r#"<svg><path d="M0 0 L10 10 L20 20 L30 30 L40 40"/></svg>"#;
    let options = FormatOptions {
        max_inline_tag_width: 30,
        ..FormatOptions::default()
    };
    let pad = " ".repeat(8 + "d=\"".len());
    // Budget = 30 - 8 - 3 = 19 chars per value line.
    // Pack: "M0 0 L10 10" (11) + " L20 20" (7)=19 → ok.
    // Next: L30 30 would exceed → new line "L30 30" (6) + " L40 40" (7)=13 → ok.
    let expected =
        format!("<svg>\n  <path d=\"M0 0 L10 10 L20 20\n{pad}L30 30 L40 40\" />\n</svg>");
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn path_data_with_source_newlines_takes_preservation_path_not_wrap_path_data() {
    // Value already carries an embedded newline (W3-reference style).
    // The path-wrap pass skips it; existing continuation-alignment kicks
    // in unchanged.
    let input = "<svg><path d=\"M0 0\n     L100 100\" /></svg>";
    let pad = " ".repeat(8 + "d=\"".len());
    let expected = format!("<svg>\n  <path d=\"M0 0\n{pad}L100 100\" />\n</svg>");
    assert_eq!(
        format_with_options(input, FormatOptions::default()),
        expected
    );
}

#[test]
fn points_attribute_wraps_in_coordinate_pairs() {
    // Polyline's `points` attribute wraps at coordinate-pair boundaries
    // when the minified value exceeds the per-line budget.
    let input = r#"<svg><polyline points="0,0 10,10 20,20 30,30 40,40 50,50"/></svg>"#;
    let options = FormatOptions {
        max_inline_tag_width: 40,
        ..FormatOptions::default()
    };
    // wrapped_prefix = 2-indent + "polyline "=10 → 12 chars.
    // pad = 12 + `points="` (8) = 20. budget = 40 - 12 - 8 = 20.
    let pad = " ".repeat(12 + "points=\"".len());
    // Pairs: "0,0" "10,10" "20,20" "30,30" "40,40" "50,50" (widths 3,5,5,5,5,5).
    // Line 1: "0,0" + " 10,10" (9) + " 20,20" (15) + " 30,30" → 21 > 20, break.
    //   → "0,0 10,10 20,20" (15).
    // Line 2: "30,30" + " 40,40" (11) + " 50,50" (17) → fits.
    //   → "30,30 40,40 50,50" (17).
    let expected =
        format!("<svg>\n  <polyline points=\"0,0 10,10 20,20\n{pad}30,30 40,40 50,50\" />\n</svg>");
    assert_eq!(format_with_options(input, options), expected);
}

#[test]
fn group_wrap_falls_back_to_one_per_line_when_group_exceeds_width() {
    // A Geometry group wider than `max_inline_tag_width - prefix` falls
    // back to one-attribute-per-line within that group.
    let input = r#"<svg><rect x="1000" y="2000" width="3000" height="4000"/></svg>"#;
    let options = FormatOptions {
        attribute_layout: AttributeLayout::MultiLine,
        max_inline_tag_width: 30,
        ..FormatOptions::default()
    };
    let aligned = format!("  {}", " ".repeat("rect".len() + 2));
    let expected = format!(
        "<svg>\n  <rect x=\"1000\"\n{aligned}y=\"2000\"\n{aligned}width=\"3000\"\n{aligned}height=\"4000\" />\n</svg>"
    );
    assert_eq!(format_with_options(input, options), expected);
}
