use std::fmt::Write as _;

use super::{BaselineQualifierValue, BaselineValue, BrowserSupportValue, BrowserVersionValue};

pub fn escape(s: &str) -> String {
    s.chars().flat_map(char::escape_default).collect()
}

#[allow(dead_code)]
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
        Some(BaselineValue::Widely { since, qualifier }) => {
            format!(
                "Some(BaselineStatus::Widely {{ since: {since}, qualifier: {} }})",
                format_qualifier(*qualifier),
            )
        }
        Some(BaselineValue::Newly { since, qualifier }) => {
            format!(
                "Some(BaselineStatus::Newly {{ since: {since}, qualifier: {} }})",
                format_qualifier(*qualifier),
            )
        }
        Some(BaselineValue::Limited) => "Some(BaselineStatus::Limited)".to_string(),
    }
}

const fn format_qualifier(qualifier: Option<BaselineQualifierValue>) -> &'static str {
    match qualifier {
        None => "None",
        Some(BaselineQualifierValue::Before) => "Some(BaselineQualifier::Before)",
        Some(BaselineQualifierValue::After) => "Some(BaselineQualifier::After)",
        Some(BaselineQualifierValue::Approximately) => "Some(BaselineQualifier::Approximately)",
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
