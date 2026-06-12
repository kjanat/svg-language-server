//! `winnow` parser for the CSS value-definition syntax used in the SVG 1.1
//! property index `Values` column.
//!
//! The grammar handled here is the small subset that appears in `propidx.html`:
//!
//! ```text
//! expr      := combine ( "|"  combine )*     // single-bar alternation (lowest precedence)
//! combine   := atom    ( "||" atom    )*     // double-bar combinable group (binds tighter)
//! atom      := group | datatype | keyword
//! group     := "[" expr "]"
//! datatype  := "<" … ">"                     // e.g. <length>, <percentage>, <color>
//! keyword   := [A-Za-z0-9] [A-Za-z0-9-]*     // e.g. nonzero, list-item, 100, visiblePainted
//! ```
//!
//! Real-world example rows:
//!
//! - `nonzero | evenodd | inherit` → keyword enum
//! - `none | [ underline || overline || line-through || blink ] | inherit` →
//!   keyword enum (the `||` group is flattened into its ordered keywords)
//! - `<color> | inherit` → **not** a keyword enum (contains a datatype) → `None`
//!
//! The caller wants only the *pure keyword enums*. [`keyword_enum`] returns the
//! ordered keyword set for those (with a trailing `inherit` stripped, per the
//! Q-INHERIT "strip" decision) and `None` for any value definition that contains
//! a datatype/grammar-ref token — those are out of scope for the keyword-enum
//! extractor.

use winnow::{
    ModalResult, Parser,
    ascii::multispace0,
    combinator::{alt, delimited, not, separated},
    token::take_while,
};

/// Parsed value-definition expression.
///
/// Kept as a small AST (rather than collapsing to keywords during parsing) so
/// the datatype/group structure is available for classification: a value
/// definition is a keyword enum only if it contains *no* [`Expr::Datatype`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// `a | b | c` — ordered single-bar alternatives.
    Alt(Vec<Self>),
    /// `a || b || c` — ordered double-bar combinable terms.
    Combine(Vec<Self>),
    /// A single keyword token.
    Keyword(String),
    /// A `<…>` datatype / grammar-ref token (makes the expr a non-keyword enum).
    Datatype,
}

/// Parse a `Values`-column string (HTML entities still encoded) and, if it is a
/// pure keyword enumeration, return its ordered keyword set with any trailing
/// `inherit` alternative stripped.
///
/// `keyword_enum` parses with [`parse_expr`]/[`parse_group`], then flattens the
/// AST through [`collect_keywords`] into an ordered `Vec<String>` of keyword
/// leaves only. It intentionally does **not** preserve the semantic difference
/// between [`Expr::Combine`] (`||`, order-insensitive multi-keyword groups) and
/// [`Expr::Alt`] (`|`). Callers that need combinable semantics must inspect the
/// AST from `parse_expr` directly instead of using this lossy helper.
///
/// Returns `None` when:
/// - the syntax fails to parse, or
/// - any alternative contains a `<datatype>` token (not a keyword enum), or
/// - stripping `inherit` leaves no keywords.
pub fn keyword_enum(values: &str) -> Option<Vec<String>> {
    let decoded = decode_entities(values);
    let expr = parse_expr.parse(decoded.trim()).ok()?;
    if contains_datatype(&expr) {
        return None;
    }
    let mut keywords = Vec::new();
    collect_keywords(&expr, &mut keywords);
    keywords.retain(|kw| kw != "inherit");
    if keywords.is_empty() {
        None
    } else {
        Some(keywords)
    }
}

/// Decode the handful of HTML entities `tl`'s `inner_text` leaves encoded in the
/// `Values` cells. `&lt;`/`&gt;` are load-bearing (they delimit datatype
/// tokens); `&nbsp;`/`&amp;` appear as incidental text.
///
/// Decode `&amp;` first so one layer of encoding becomes visible to the normal
/// entity replacements (`&amp;lt;` → `&lt;` → `<`). This assumes at most one extra
/// encoding layer; triple/multi-encoded text is not recursively decoded.
fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

/// `true` if the expression tree contains any datatype token at any depth.
fn contains_datatype(expr: &Expr) -> bool {
    match expr {
        Expr::Datatype => true,
        Expr::Keyword(_) => false,
        Expr::Alt(items) | Expr::Combine(items) => items.iter().any(contains_datatype),
    }
}

/// Collect keyword leaves in source order (depth-first, left-to-right).
fn collect_keywords(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Keyword(value) => out.push(value.clone()),
        Expr::Datatype => {}
        Expr::Alt(items) | Expr::Combine(items) => {
            for item in items {
                collect_keywords(item, out);
            }
        }
    }
}

/// `expr := combine ( "|" combine )*` — single-bar alternation (lowest
/// precedence). A single term parses to itself (not wrapped in `Alt`).
fn parse_expr(input: &mut &str) -> ModalResult<Expr> {
    let mut terms: Vec<Expr> = separated(1.., parse_combine, single_bar).parse_next(input)?;
    Ok(if terms.len() == 1 {
        terms.remove(0)
    } else {
        Expr::Alt(terms)
    })
}

/// `combine := atom ( "||" atom )*` — double-bar combinable group (binds tighter
/// than `|`). A single atom parses to itself (not wrapped in `Combine`).
fn parse_combine(input: &mut &str) -> ModalResult<Expr> {
    let mut terms: Vec<Expr> = separated(1.., parse_atom, double_bar).parse_next(input)?;
    Ok(if terms.len() == 1 {
        terms.remove(0)
    } else {
        Expr::Combine(terms)
    })
}

/// `atom := group | datatype | keyword`.
fn parse_atom(input: &mut &str) -> ModalResult<Expr> {
    delimited(
        multispace0,
        alt((parse_group, parse_datatype, parse_keyword)),
        multispace0,
    )
    .parse_next(input)
}

/// `group := "[" expr "]"` — square-bracket grouping.
fn parse_group(input: &mut &str) -> ModalResult<Expr> {
    delimited(('[', multispace0), parse_expr, (multispace0, ']')).parse_next(input)
}

/// `datatype := "<" … ">"` — angle-bracket datatype / grammar-ref token. The
/// inner name is irrelevant to keyword extraction, so it is discarded.
fn parse_datatype(input: &mut &str) -> ModalResult<Expr> {
    delimited('<', take_while(0.., |ch: char| ch != '>'), '>')
        .map(|_| Expr::Datatype)
        .parse_next(input)
}

/// `keyword := [A-Za-z0-9] [A-Za-z0-9-]*`.
///
/// Accepts hyphenated keywords (`list-item`, `line-through`), camelCase
/// (`visiblePainted`), and the numeric `font-weight` keywords (`100`…`900`).
fn parse_keyword(input: &mut &str) -> ModalResult<Expr> {
    take_while(1.., |ch: char| ch.is_ascii_alphanumeric() || ch == '-')
        .map(|kw: &str| Expr::Keyword(kw.to_string()))
        .parse_next(input)
}

/// A single `|` that is **not** part of a `||`. The `||` separator must win, so
/// this parser rejects when a second bar follows.
fn single_bar(input: &mut &str) -> ModalResult<()> {
    delimited(multispace0, ('|', not('|')), multispace0)
        .map(|_| ())
        .parse_next(input)
}

/// A `||` combinable separator.
fn double_bar(input: &mut &str) -> ModalResult<()> {
    delimited(multispace0, "||", multispace0)
        .map(|_| ())
        .parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(values: &str) -> Option<Vec<String>> {
        keyword_enum(values)
    }

    #[test]
    fn simple_enum_strips_inherit() {
        assert_eq!(
            kw("nonzero | evenodd | inherit"),
            Some(vec!["nonzero".into(), "evenodd".into()])
        );
    }

    #[test]
    fn combinable_group_flattens_in_order() {
        assert_eq!(
            kw("none | [ underline || overline || line-through || blink ] | inherit"),
            Some(vec![
                "none".into(),
                "underline".into(),
                "overline".into(),
                "line-through".into(),
                "blink".into(),
            ])
        );
    }

    #[test]
    fn numeric_keywords_parse() {
        assert_eq!(
            kw("normal | bold | 100 | 900 | inherit"),
            Some(vec![
                "normal".into(),
                "bold".into(),
                "100".into(),
                "900".into()
            ])
        );
    }

    #[test]
    fn datatype_rejected() {
        assert_eq!(kw("&lt;color&gt; | inherit"), None);
        assert_eq!(
            kw("baseline | sub | super | &lt;percentage&gt; | &lt;length&gt; | inherit"),
            None
        );
    }

    #[test]
    fn pointer_events_order_preserved() {
        assert_eq!(
            kw(
                "visiblePainted | visibleFill | visibleStroke | visible | painted | fill | stroke | all | none | inherit"
            ),
            Some(vec![
                "visiblePainted".into(),
                "visibleFill".into(),
                "visibleStroke".into(),
                "visible".into(),
                "painted".into(),
                "fill".into(),
                "stroke".into(),
                "all".into(),
                "none".into(),
            ])
        );
    }
}
