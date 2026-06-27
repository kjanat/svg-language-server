//! Path-data facts scraped from SVGWG `paths.html`.

use std::{collections::BTreeSet, sync::LazyLock};

use regex::Regex;

use crate::{fetch, util::boxed};

const PATHS_HTML_PATH: &str = "master/paths.html";
const PATH_DATA_BNF_HEADING: &str = "id=\"PathDataBNF\"";

/// The production whose alternatives enumerate the path commands. The command
/// set is read from the grammar's own command list rather than a hand-written
/// roster, so a command added upstream is discovered automatically.
const DRAWTO_COMMAND_PRODUCTION: &str = "drawto_command";

/// Exact set of path-command letters the derivation must yield (both cases of
/// every `drawto_command` alternative). Extraction discovers these from
/// `#PathDataBNF`; the validation gate asserts the scraped set matches this
/// exactly, so a command added, dropped, or renamed upstream fails the build.
const EXPECTED_PATH_COMMAND_LETTERS: &[&str] = &[
    "A", "C", "H", "L", "M", "Q", "S", "T", "V", "Z", "a", "c", "h", "l", "m", "q", "s", "t", "v",
    "z",
];

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

static QUOTED_LETTER_RE: LazyLock<Regex> =
    LazyLock::new(|| crate::util::compile_regex(r#"["']([A-Za-z])["']"#));
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| crate::util::compile_regex("(?is)<[^>]+>"));

/// Path-data grammar facts from the pinned SVGWG `paths.html` chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathsGrammarFacts {
    /// Raw GitHub URL for the pinned `paths.html` source.
    pub url: String,
    /// The `d` property name from `#DProperty`.
    pub path_data_property: String,
    /// Single-letter moveto/lineto/curveto/arc commands from `#PathDataBNF`.
    pub path_command_letters: Vec<String>,
}

/// Fetch and parse path-data grammar facts from SVGWG `paths.html`.
///
/// # Errors
/// Returns an error when the chapter is missing required anchors or the scraped
/// command letters do not satisfy the validation gate.
pub fn fetch_paths_grammar_facts(repo_slug: &str, commit_sha: &str) -> Fallible<PathsGrammarFacts> {
    let html = fetch::raw_file(repo_slug, commit_sha, PATHS_HTML_PATH)?;
    let url =
        format!("https://raw.githubusercontent.com/{repo_slug}/{commit_sha}/{PATHS_HTML_PATH}");
    let path_data_property = extract_path_data_property(&html)?;
    let path_command_letters = extract_path_command_letters(&html)?;
    validate_path_command_letters(&path_command_letters)?;
    Ok(PathsGrammarFacts {
        url,
        path_data_property,
        path_command_letters,
    })
}

fn extract_path_data_property(html: &str) -> Fallible<String> {
    let marker = r#"<dfn id="DProperty""#;
    let dfn_start = html
        .find(marker)
        .ok_or_else(|| boxed("paths.html missing #DProperty <dfn>"))?;
    let dfn_open_end = tag_open_end(html, dfn_start)
        .ok_or_else(|| boxed("paths.html #DProperty <dfn> tag is malformed"))?;
    let dfn_close = html[dfn_open_end..]
        .find("</dfn>")
        .map(|offset| dfn_open_end + offset)
        .ok_or_else(|| boxed("paths.html #DProperty <dfn> is unclosed"))?;
    let name = strip_tags(&html[dfn_open_end..dfn_close]);
    if name.is_empty() {
        return Err(boxed("paths.html #DProperty <dfn> has no property name"));
    }
    Ok(name)
}

fn extract_path_command_letters(html: &str) -> Fallible<Vec<String>> {
    let grammar = path_data_bnf_grammar(html)?;
    let mut letters = BTreeSet::new();
    for production in command_production_names(grammar)? {
        let Some(block) = production_block(grammar, &production) else {
            return Err(boxed(format!(
                "paths.html svg-path grammar missing `{production}` production listed by \
                 `{DRAWTO_COMMAND_PRODUCTION}`"
            )));
        };
        for capture in QUOTED_LETTER_RE.captures_iter(block) {
            letters.insert(capture[1].to_owned());
        }
    }
    if letters.is_empty() {
        return Err(boxed(
            "paths.html svg-path grammar contained no path command letters",
        ));
    }
    Ok(letters.into_iter().collect())
}

/// Command productions = the bare-identifier alternatives of the grammar's own
/// `drawto_command` production, so the command set is derived from the spec
/// rather than enumerated by hand.
fn command_production_names(grammar: &str) -> Fallible<Vec<String>> {
    let block = production_block(grammar, DRAWTO_COMMAND_PRODUCTION).ok_or_else(|| {
        boxed(format!(
            "paths.html svg-path grammar missing `{DRAWTO_COMMAND_PRODUCTION}` production"
        ))
    })?;
    let names: Vec<String> = block
        .split('|')
        .filter_map(|alternative| {
            let name = alternative.trim();
            (!name.is_empty() && name.chars().all(is_production_name_char)).then(|| name.to_owned())
        })
        .collect();
    if names.is_empty() {
        return Err(boxed(format!(
            "paths.html `{DRAWTO_COMMAND_PRODUCTION}` production listed no command alternatives"
        )));
    }
    Ok(names)
}

fn path_data_bnf_grammar(html: &str) -> Fallible<&str> {
    let heading = html
        .find(PATH_DATA_BNF_HEADING)
        .ok_or_else(|| boxed("paths.html missing #PathDataBNF anchor"))?;
    let after_heading = &html[heading..];
    let pre_start = after_heading
        .find("<pre")
        .ok_or_else(|| boxed("paths.html #PathDataBNF missing <pre> grammar block"))?;
    let pre_open = after_heading[pre_start..]
        .find('>')
        .map(|offset| pre_start + offset + 1)
        .ok_or_else(|| boxed("paths.html path grammar <pre> has no opening tag"))?;
    let pre_end = after_heading[pre_open..]
        .find("</pre>")
        .map(|offset| pre_open + offset)
        .ok_or_else(|| boxed("paths.html path grammar <pre> has no closing tag"))?;
    Ok(&after_heading[pre_open..pre_end])
}

fn production_block<'a>(grammar: &'a str, production: &str) -> Option<&'a str> {
    let body_start = production_body_start(grammar, production)?;
    let body = &grammar[body_start..];
    let end = next_production_start(body).unwrap_or(body.len());
    Some(&body[..end])
}

/// Byte offset just past the `::=` of the line-anchored production header whose
/// name equals `production`. Line-anchored rather than a bare substring search,
/// so a shorter name like `lineto` is never matched inside `horizontal_lineto`
/// and the result does not depend on the order productions are declared in.
fn production_body_start(grammar: &str, production: &str) -> Option<usize> {
    let mut offset = 0;
    for line in grammar.split_inclusive('\n') {
        let leading = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        if production_header_name(trimmed) == Some(production) {
            let separator = trimmed.find("::=")?;
            return Some(offset + leading + separator + "::=".len());
        }
        offset += line.len();
    }
    None
}

/// The production name when `line` (already left-trimmed) is a `name::=` header.
fn production_header_name(line: &str) -> Option<&str> {
    let (name, _) = line.split_once("::=")?;
    let name = name.trim();
    (!name.is_empty() && name.chars().all(is_production_name_char)).then_some(name)
}

/// Characters allowed in an EBNF production name. The SVG grammar uses both `_`
/// (`horizontal_lineto`) and `-` (`fractional-constant`), so both must count or
/// a hyphenated production is not recognised as a boundary.
const fn is_production_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn validate_path_command_letters(letters: &[String]) -> Fallible<()> {
    let extracted: BTreeSet<&str> = letters.iter().map(String::as_str).collect();
    let expected: BTreeSet<&str> = EXPECTED_PATH_COMMAND_LETTERS.iter().copied().collect();
    let missing: Vec<&str> = expected.difference(&extracted).copied().collect();
    let unexpected: Vec<&str> = extracted.difference(&expected).copied().collect();
    if missing.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!("missing: {}", missing.join(", ")));
    }
    if !unexpected.is_empty() {
        problems.push(format!("unexpected: {}", unexpected.join(", ")));
    }
    Err(boxed(format!(
        "paths.html path command letters do not match the expected set ({})",
        problems.join("; ")
    )))
}

fn next_production_start(body: &str) -> Option<usize> {
    let mut offset = 0;
    for (index, line) in body.split_inclusive('\n').enumerate() {
        if index == 0 {
            offset += line.len();
            continue;
        }
        let trimmed = line.trim_start();
        if is_production_header(trimmed) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

fn is_production_header(line: &str) -> bool {
    production_header_name(line).is_some()
}

fn tag_open_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, ch) in html[start..].char_indices() {
        match (quote, ch) {
            (Some(current), found) if found == current => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(start + offset + ch.len_utf8()),
            _ => {}
        }
    }
    None
}

fn strip_tags(html: &str) -> String {
    TAG_RE.replace_all(html, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PATHS_HTML: &str = r#"
<h3 id="PathDataBNF">The grammar for path data</h3>
<pre class="grammar">
drawto_command::=
    moveto | closepath | lineto | horizontal_lineto | vertical_lineto
    | curveto | smooth_curveto | quadratic_bezier_curveto
    | smooth_quadratic_bezier_curveto | elliptical_arc
moveto::=
    ( "M" | "m" ) wsp* coordinate_pair_sequence
closepath::=
    ("Z" | "z")
lineto::=
    ("L"|"l") wsp* coordinate_pair_sequence
horizontal_lineto::=
    ("H"|"h") wsp* coordinate_sequence
vertical_lineto::=
    ("V"|"v") wsp* coordinate_sequence
curveto::=
    ("C"|"c") wsp* curveto_coordinate_sequence
smooth_curveto::=
    ("S"|"s") wsp* smooth_curveto_coordinate_sequence
quadratic_bezier_curveto::=
    ("Q"|"q") wsp* quadratic_bezier_curveto_coordinate_sequence
smooth_quadratic_bezier_curveto::=
    ("T"|"t") wsp* coordinate_pair_sequence
elliptical_arc::=
    ( "A" | "a" ) wsp* elliptical_arc_argument_sequence
exponent::= ("e" | "E") sign? digit+
</pre>
<table class="propdef def">
  <tr><th>Name:</th><td><dfn id="DProperty" data-dfn-type="property">d</dfn></td></tr>
</table>
"#;

    fn panic_letters(result: Fallible<Vec<String>>) -> Vec<String> {
        match result {
            Ok(letters) => letters,
            Err(error) => panic!("letters: {error}"),
        }
    }

    fn panic_property(result: Fallible<String>) -> String {
        match result {
            Ok(property) => property,
            Err(error) => panic!("property: {error}"),
        }
    }

    /// Wrap an EBNF body in the minimal `#PathDataBNF` `<pre>` the extractor looks for.
    fn bnf(grammar_body: &str) -> String {
        format!("<h3 id=\"PathDataBNF\">g</h3>\n<pre class=\"grammar\">\n{grammar_body}\n</pre>\n")
    }

    #[test]
    fn derives_path_command_letters_from_drawto_command_alternatives() {
        // The command set comes from `drawto_command`; quoted letters in
        // non-command productions (the sample's `exponent`) contribute nothing.
        let letters = panic_letters(extract_path_command_letters(SAMPLE_PATHS_HTML));
        assert_eq!(
            letters,
            [
                "A", "C", "H", "L", "M", "Q", "S", "T", "V", "Z", "a", "c", "h", "l", "m", "q",
                "s", "t", "v", "z"
            ]
        );
    }

    #[test]
    fn validation_gate_rejects_letters_outside_the_expected_set() {
        let with_extra: Vec<String> = EXPECTED_PATH_COMMAND_LETTERS
            .iter()
            .map(|letter| (*letter).to_owned())
            .chain(std::iter::once("B".to_owned()))
            .collect();
        let Err(err) = validate_path_command_letters(&with_extra) else {
            panic!("expected an unexpected-letter to fail the gate")
        };
        assert!(err.to_string().contains("unexpected: B"));
    }

    #[test]
    fn extracts_d_property_name_from_dproperty_anchor() {
        assert_eq!(
            panic_property(extract_path_data_property(SAMPLE_PATHS_HTML)),
            "d"
        );
    }

    #[test]
    fn rejects_missing_path_data_bnf_anchor() {
        let Err(err) = extract_path_command_letters("<html></html>") else {
            panic!("expected missing PathDataBNF to fail")
        };
        assert!(err.to_string().contains("PathDataBNF"));
    }

    #[test]
    fn extraction_is_independent_of_production_declaration_order() {
        // Longer-named commands declared BEFORE their shorter-name prefixes:
        // a substring search for `lineto::=` would hit `horizontal_lineto::=`
        // first. Line-anchored lookup must still resolve each command exactly.
        let grammar = bnf(
            "drawto_command::= horizontal_lineto | smooth_curveto | lineto | \
             curveto\nhorizontal_lineto::= ( \"H\" | \"h\" ) wsp* \
             coordinate_sequence\nsmooth_curveto::= ( \"S\" | \"s\" ) wsp* seq\nlineto::= ( \"L\" \
             | \"l\" ) wsp* coordinate_pair_sequence\ncurveto::= ( \"C\" | \"c\" ) wsp* \
             curveto_coordinate_sequence",
        );
        let letters = panic_letters(extract_path_command_letters(&grammar));
        assert_eq!(letters, ["C", "H", "L", "S", "c", "h", "l", "s"]);
    }

    #[test]
    fn hyphenated_neighbor_production_does_not_leak_into_a_command_block() {
        // A hyphenated production (like the spec's `fractional-constant`) right
        // after a command must be a recognised boundary, so its stray quoted
        // letters are not absorbed into the command's block.
        let grammar = bnf(
            "drawto_command::= lineto\nlineto::= ( \"L\" | \"l\" ) wsp* \
             coordinate_pair_sequence\nfractional-constant::= ( \"X\" | \"x\" )",
        );
        let letters = panic_letters(extract_path_command_letters(&grammar));
        assert_eq!(letters, ["L", "l"]);
    }

    #[test]
    fn new_drawto_command_alternative_is_included_automatically() {
        // A command added to `drawto_command` upstream is picked up with no code
        // change. (Calls the extractor directly so the exact gate, which would
        // reject the unknown letters, does not mask the sensitivity.)
        let grammar = bnf(
            "drawto_command::= moveto | pizzazz\nmoveto::= ( \"M\" | \"m\" ) wsp* \
             coordinate_pair_sequence\npizzazz::= ( \"F\" | \"f\" ) wsp* coordinate_pair_sequence",
        );
        let letters = panic_letters(extract_path_command_letters(&grammar));
        assert_eq!(letters, ["F", "M", "f", "m"]);
    }

    #[test]
    fn single_quoted_command_terminals_are_captured() {
        let grammar =
            bnf("drawto_command::= moveto\nmoveto::= ( 'M' | 'm' ) wsp* coordinate_pair_sequence");
        let letters = panic_letters(extract_path_command_letters(&grammar));
        assert_eq!(letters, ["M", "m"]);
    }
}
