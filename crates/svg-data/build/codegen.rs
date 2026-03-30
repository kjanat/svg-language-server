use std::fmt::Write as _;

use super::{BaselineValue, BrowserSupportValue};

pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
    name.replace('-', "_").to_uppercase()
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
        format_option_str(bs.chrome.as_deref()),
        format_option_str(bs.edge.as_deref()),
        format_option_str(bs.firefox.as_deref()),
        format_option_str(bs.safari.as_deref()),
    )
}

pub fn format_option_str(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_string(),
        |s| format!("Some(\"{}\")", escape(s)),
    )
}
