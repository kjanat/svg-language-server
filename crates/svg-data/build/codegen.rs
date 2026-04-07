use std::fmt::Write as _;

use super::{BaselineValue, BrowserSupportValue, BrowserVersionValue};

pub fn escape(s: &str) -> String {
    s.chars().flat_map(char::escape_default).collect()
}

pub fn write_static_str_slice(out: &mut String, name: &str, items: &[String]) -> std::fmt::Result {
    write!(out, "static {name}: &[&str] = &[")?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "\"{}\"", escape(item))?;
    }
    writeln!(out, "];")
}

pub fn ident_from(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub fn format_baseline(baseline: Option<&BaselineValue>) -> String {
    match baseline {
        None => "None".to_string(),
        Some(BaselineValue::Widely { since }) => {
            format!("Some(BaselineStatus::Widely {{ since: {since} }})")
        }
        Some(BaselineValue::Newly { since }) => {
            format!("Some(BaselineStatus::Newly {{ since: {since} }})")
        }
        Some(BaselineValue::Limited) => "Some(BaselineStatus::Limited)".to_string(),
    }
}

pub fn format_browser_support(bs: Option<&BrowserSupportValue>) -> String {
    let Some(bs) = bs else {
        return "None".to_string();
    };
    format!(
        "Some(BrowserSupport {{ chrome: {}, edge: {}, firefox: {}, safari: {} }})",
        format_browser_version(bs.chrome.as_ref()),
        format_browser_version(bs.edge.as_ref()),
        format_browser_version(bs.firefox.as_ref()),
        format_browser_version(bs.safari.as_ref()),
    )
}

fn format_browser_version(value: Option<&BrowserVersionValue>) -> String {
    match value {
        None => "None".to_string(),
        Some(BrowserVersionValue::Unknown) => "Some(BrowserVersion::Unknown)".to_string(),
        Some(BrowserVersionValue::Version(version)) => {
            format!("Some(BrowserVersion::Version(\"{}\"))", escape(version))
        }
    }
}

pub fn format_option_str(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_string(),
        |s| format!("Some(\"{}\")", escape(s)),
    )
}
