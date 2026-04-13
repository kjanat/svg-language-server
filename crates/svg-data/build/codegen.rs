use std::fmt::Write as _;

use super::{
    BaselineQualifierValue, BaselineValue, BrowserFlagValue, BrowserSupportValue,
    BrowserVersionValue, RawVersionAddedValue,
};

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
    let Some(v) = value else {
        return "None".to_string();
    };
    format!(
        concat!(
            "Some(BrowserVersion {{ ",
            "raw_value_added: {}, ",
            "version_added: {}, ",
            "version_qualifier: {}, ",
            "supported: {}, ",
            "version_removed: {}, ",
            "version_removed_qualifier: {}, ",
            "partial_implementation: {}, ",
            "prefix: {}, ",
            "alternative_name: {}, ",
            "flags: {}, ",
            "notes: {} ",
            "}})",
        ),
        format_raw_version_added(&v.raw_value_added),
        format_option_str(v.version_added.as_deref()),
        format_qualifier(v.version_qualifier),
        format_option_bool(v.supported),
        format_option_str(v.version_removed.as_deref()),
        format_qualifier(v.version_removed_qualifier),
        v.partial_implementation,
        format_option_str(v.prefix.as_deref()),
        format_option_str(v.alternative_name.as_deref()),
        format_browser_flags(&v.flags),
        format_static_str_slice(&v.notes),
    )
}

fn format_raw_version_added(raw: &RawVersionAddedValue) -> String {
    match raw {
        RawVersionAddedValue::Text(s) => format!("RawVersionAdded::Text(\"{}\")", escape(s)),
        RawVersionAddedValue::Flag(b) => format!("RawVersionAdded::Flag({b})"),
        RawVersionAddedValue::Null => "RawVersionAdded::Null".to_string(),
    }
}

const fn format_option_bool(value: Option<bool>) -> &'static str {
    match value {
        None => "None",
        Some(true) => "Some(true)",
        Some(false) => "Some(false)",
    }
}

fn format_browser_flags(flags: &[BrowserFlagValue]) -> String {
    if flags.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, flag) in flags.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(
            out,
            "BrowserFlag {{ flag_type: \"{}\", name: \"{}\", value_to_set: {} }}",
            escape(&flag.flag_type),
            escape(&flag.name),
            format_option_str(flag.value_to_set.as_deref()),
        );
    }
    out.push(']');
    out
}

fn format_static_str_slice(items: &[String]) -> String {
    if items.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "\"{}\"", escape(item));
    }
    out.push(']');
    out
}

pub fn format_option_str(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_string(),
        |s| format!("Some(\"{}\")", escape(s)),
    )
}
