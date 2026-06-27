//! Browser-compat-data extraction for objective catalog facts.
//!
//! The fetched support statements remain the source of truth, but BCD's key
//! shapes do not line up 1:1 with the catalog. This module therefore owns an
//! explicit compatibility-normalization layer: mapping BCD spellings to
//! canonical catalog names and classifying non-catalog subfeatures. That policy
//! lives here instead of being mixed into the semantic catalog assembly path.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    sync::LazyLock,
};

use regex::{Captures, Regex};
use serde_json::Value;

use crate::{
    catalog::{
        CatalogBaselineStatus, CatalogBrowserFlag, CatalogBrowserSupport, CatalogBrowserVersion,
        CatalogCompatFacts, CatalogCompatProvenance, CatalogCompatSubfeature,
        CatalogCompatSubfeatureKind, CatalogPackageSource,
    },
    fetch,
    util::{boxed, decode_html_entities, normalize_ws},
};

const BCD_PACKAGE: &str = "@mdn/browser-compat-data";
const WEB_FEATURES_PACKAGE: &str = "web-features";

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// Objective compat facts collected from BCD and web-features.
pub struct CompatCatalog {
    /// Source package versions and URLs.
    pub provenance: CatalogCompatProvenance,
    /// Facts keyed by SVG element name.
    pub elements: BTreeMap<String, CatalogCompatFacts>,
    /// Attribute facts keyed by canonical SVG attribute name.
    pub attributes: BTreeMap<String, CompatAttribute>,
}

/// Objective compat facts plus BCD-derived applicability for one attribute.
pub struct CompatAttribute {
    /// Objective facts from `/svg/global_attributes`, when BCD has a global record.
    pub global_facts: Option<CatalogCompatFacts>,
    /// Objective facts from `/svg/elements/<element>/<attribute>`.
    pub element_facts: BTreeMap<String, CatalogCompatFacts>,
}

impl CompatAttribute {
    /// Element names where BCD defines this as an element-local attribute.
    pub fn bearers(&self) -> impl Iterator<Item = &String> {
        self.element_facts.keys()
    }

    /// Whether BCD defines this under `/svg/global_attributes`.
    pub const fn is_global(&self) -> bool {
        self.global_facts.is_some()
    }

    /// Facts that can be safely used for attribute-wide fallback.
    pub fn common_facts(&self) -> Option<&CatalogCompatFacts> {
        if let Some(facts) = self.global_facts.as_ref() {
            return Some(facts);
        }
        let mut facts = self.element_facts.values();
        let first = facts.next()?;
        facts.all(|facts| facts == first).then_some(first)
    }
}

/// Fetch and parse browser-compat-data and web-features.
pub fn fetch_compat_catalog() -> Fallible<CompatCatalog> {
    let bcd_source = package_source(BCD_PACKAGE, "data.json")?;
    let web_features_source = package_source(WEB_FEATURES_PACKAGE, "data.json")?;
    let bcd_json: Value =
        serde_json::from_str(&fetch::url_text(&bcd_source.url, "application/json")?)?;
    let web_features_json: Value = serde_json::from_str(&fetch::url_text(
        &web_features_source.url,
        "application/json",
    )?)?;

    let svg_elements = bcd_json
        .pointer("/svg/elements")
        .and_then(Value::as_object)
        .ok_or_else(|| boxed("browser-compat-data missing /svg/elements object"))?;
    let web_features = web_features_json.get("features");

    let mut elements = BTreeMap::new();
    let mut attributes = BTreeMap::new();
    let mut unmodeled_features = Vec::new();
    collect_element_facts(
        svg_elements,
        web_features,
        &mut elements,
        &mut attributes,
        &mut unmodeled_features,
    );
    collect_global_attribute_facts(&bcd_json, web_features, &mut attributes);

    Ok(CompatCatalog {
        provenance: CatalogCompatProvenance {
            browser_compat_data: bcd_source,
            web_features: web_features_source,
            unmodeled_features,
        },
        elements,
        attributes,
    })
}

fn package_source(package: &str, path: &str) -> Fallible<CatalogPackageSource> {
    let version = npm_latest_version(package)?;
    let url = format!("https://unpkg.com/{package}@{version}/{path}");
    Ok(CatalogPackageSource {
        name: package.to_owned(),
        version,
        url,
    })
}

fn npm_latest_version(package: &str) -> Fallible<String> {
    let registry_package = package.replace('/', "%2f");
    let url = format!("https://registry.npmjs.org/{registry_package}");
    let json: Value = serde_json::from_str(&fetch::url_text(&url, "application/json")?)?;
    let version = json
        .pointer("/dist-tags/latest")
        .and_then(Value::as_str)
        .ok_or_else(|| boxed("npm package metadata missing dist-tags.latest"))?;
    Ok(version.to_owned())
}

fn collect_element_facts(
    svg_elements: &serde_json::Map<String, Value>,
    web_features: Option<&Value>,
    elements: &mut BTreeMap<String, CatalogCompatFacts>,
    attributes: &mut BTreeMap<String, CompatAttribute>,
    unmodeled_features: &mut Vec<CatalogCompatSubfeature>,
) {
    for (element_name, element_data) in svg_elements {
        if let Some(compat) = element_data.pointer("/__compat") {
            elements.insert(
                element_name.clone(),
                facts_from_compat(
                    compat,
                    web_features,
                    &format!("svg.elements.{element_name}"),
                ),
            );
        }
        let Some(attribute_map) = element_data.as_object() else {
            continue;
        };
        for (attribute_name, attribute_data) in attribute_map {
            if attribute_name == "__compat" {
                continue;
            }
            let Some(compat) = attribute_data.pointer("/__compat") else {
                continue;
            };
            let compat_key = format!("svg.elements.{element_name}.{attribute_name}");
            let Some(canonical) = bcd_attribute_name(attribute_name) else {
                if let Some(kind) = unmodeled_feature_kind(attribute_name) {
                    unmodeled_features.push(CatalogCompatSubfeature {
                        compat_key: compat_key.clone(),
                        kind,
                        element: element_name.clone(),
                        name: attribute_name.clone(),
                        facts: facts_from_compat(compat, web_features, &compat_key),
                    });
                }
                continue;
            };
            let facts = facts_from_compat(compat, web_features, &compat_key);
            merge_element_compat_attribute(attributes.entry(canonical), element_name, facts);
        }
    }
}

fn collect_global_attribute_facts(
    bcd_json: &Value,
    web_features: Option<&Value>,
    attributes: &mut BTreeMap<String, CompatAttribute>,
) {
    let Some(global_attributes) = bcd_json
        .pointer("/svg/global_attributes")
        .and_then(Value::as_object)
    else {
        return;
    };
    for (attribute_name, attribute_data) in global_attributes {
        let Some(compat) = attribute_data.pointer("/__compat") else {
            continue;
        };
        let Some(canonical) = bcd_attribute_name(attribute_name) else {
            continue;
        };
        let facts = facts_from_compat(
            compat,
            web_features,
            &format!("svg.global_attributes.{attribute_name}"),
        );
        merge_global_compat_attribute(attributes.entry(canonical), facts);
    }
}

fn facts_from_compat(
    compat: &Value,
    web_features: Option<&Value>,
    compat_key: &str,
) -> CatalogCompatFacts {
    CatalogCompatFacts {
        mdn_url: compat
            .get("mdn_url")
            .and_then(Value::as_str)
            .map(str::to_owned),
        deprecated: compat
            .pointer("/status/deprecated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        experimental: compat
            .pointer("/status/experimental")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        standard_track: compat
            .pointer("/status/standard_track")
            .and_then(Value::as_bool),
        baseline: resolve_baseline(web_features, compat_key),
        browser_support: browser_support_from_compat(compat),
    }
}

fn browser_support_from_compat(compat: &Value) -> Option<CatalogBrowserSupport> {
    let support = compat.get("support")?.as_object()?;
    let support = CatalogBrowserSupport {
        chrome: support.get("chrome").and_then(browser_version_from_support),
        edge: support.get("edge").and_then(browser_version_from_support),
        firefox: support
            .get("firefox")
            .and_then(browser_version_from_support),
        safari: support.get("safari").and_then(browser_version_from_support),
    };
    (!support.is_empty()).then_some(support)
}

fn browser_version_from_support(value: &Value) -> Option<CatalogBrowserVersion> {
    if let Some(items) = value.as_array() {
        let mut unsupported = None;
        for item in items {
            let Some(version) = browser_version_from_support_statement(item) else {
                continue;
            };
            if version.supported == Some(false) {
                unsupported.get_or_insert(version);
            } else {
                return Some(version);
            }
        }
        return unsupported;
    }
    browser_version_from_support_statement(value)
}

fn browser_version_from_support_statement(value: &Value) -> Option<CatalogBrowserVersion> {
    let version_added_value = value.get("version_added");
    let supported = match version_added_value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::String(_) | Value::Null) => Some(true),
        _ => None,
    };
    let version_added = version_added_value
        .and_then(Value::as_str)
        .map(str::to_owned);
    let version_qualifier = version_added_value
        .and_then(Value::as_str)
        .and_then(parse_version_qualifier);
    let version_removed = value
        .get("version_removed")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let version_removed_qualifier = value
        .get("version_removed")
        .and_then(Value::as_str)
        .and_then(parse_version_qualifier);
    let partial_implementation = value
        .get("partial_implementation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let prefix = value
        .get("prefix")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let alternative_name = value
        .get("alternative_name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let flags = browser_flags_from_value(value.get("flags"));
    let notes = strings_from_value(value.get("notes"));

    if supported.is_none()
        && version_removed.is_none()
        && !partial_implementation
        && prefix.is_none()
        && alternative_name.is_none()
        && flags.is_empty()
        && notes.is_empty()
    {
        return None;
    }

    Some(CatalogBrowserVersion {
        supported,
        partial_implementation,
        notes,
        prefix,
        alternative_name,
        flags,
        version_added,
        version_qualifier,
        version_removed,
        version_removed_qualifier,
    })
}

fn parse_version_qualifier(version: &str) -> Option<crate::catalog::CatalogBaselineQualifier> {
    if version.starts_with('\u{2264}') || version.starts_with("<=") {
        Some(crate::catalog::CatalogBaselineQualifier::Before)
    } else if version.starts_with('\u{2265}') || version.starts_with(">=") {
        Some(crate::catalog::CatalogBaselineQualifier::After)
    } else if version.starts_with('~') {
        Some(crate::catalog::CatalogBaselineQualifier::Approximately)
    } else {
        None
    }
}

fn browser_flags_from_value(value: Option<&Value>) -> Vec<CatalogBrowserFlag> {
    let Some(flags) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut names: Vec<String> = flags
        .iter()
        .filter_map(|flag| flag.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|name| CatalogBrowserFlag { name })
        .collect()
}

fn strings_from_value(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![normalize_support_note(value)],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(normalize_support_note)
            .collect(),
        _ => Vec::new(),
    }
}

static CODE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex("(?is)<code>(.*?)</code>"));
static ANCHOR_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex("(?is)<a\\s+[^>]*href=(?:\"([^\"]*)\"|'([^']*)')[^>]*>(.*?)</a>")
});
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| compile_regex("(?is)<[^>]+>"));

fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid regex {pattern:?}: {error}"),
    }
}

fn normalize_support_note(note: &str) -> String {
    let note = CODE_TAG_RE.replace_all(note, |captures: &Captures<'_>| {
        format!("`{}`", captures.get(1).map_or("", |m| m.as_str()))
    });
    let note = ANCHOR_TAG_RE.replace_all(&note, |captures: &Captures<'_>| {
        let href = captures
            .get(1)
            .or_else(|| captures.get(2))
            .map_or("", |m| m.as_str());
        let label = captures.get(3).map_or("", |m| m.as_str());
        format!("[{label}]({href})")
    });
    let note = HTML_TAG_RE.replace_all(&note, "");
    normalize_ws(decode_html_entities(&note).as_ref())
}

fn resolve_baseline(
    web_features: Option<&Value>,
    compat_key: &str,
) -> Option<CatalogBaselineStatus> {
    let feature = web_feature_for_compat_key(web_features?, compat_key)?;
    let status = feature
        .get("status")?
        .get("by_compat_key")
        .and_then(Value::as_object)
        .and_then(|by_key| by_key.get(compat_key))
        .or_else(|| feature.get("status"))?;
    baseline_status_from_web_features(status)
}

fn web_feature_for_compat_key<'a>(web_features: &'a Value, compat_key: &str) -> Option<&'a Value> {
    web_features.as_object()?.values().find(|feature| {
        feature
            .get("compat_features")
            .and_then(Value::as_array)
            .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some(compat_key)))
    })
}

fn baseline_status_from_web_features(status: &Value) -> Option<CatalogBaselineStatus> {
    match status.get("baseline")? {
        Value::String(value) if value == "high" => {
            let date = status.get("baseline_high_date")?.as_str()?;
            year_from_date(date).map(|since| CatalogBaselineStatus::Widely {
                since,
                qualifier: parse_version_qualifier(date),
            })
        }
        Value::String(value) if value == "low" => {
            let date = status.get("baseline_low_date")?.as_str()?;
            year_from_date(date).map(|since| CatalogBaselineStatus::Newly {
                since,
                qualifier: parse_version_qualifier(date),
            })
        }
        Value::String(value) if value == "limited" => Some(CatalogBaselineStatus::Limited),
        Value::Bool(false) => Some(CatalogBaselineStatus::Limited),
        _ => None,
    }
}

fn merge_element_compat_attribute(
    entry: Entry<'_, String, CompatAttribute>,
    element_name: &str,
    new: CatalogCompatFacts,
) {
    match entry {
        Entry::Vacant(entry) => {
            let mut element_facts = BTreeMap::new();
            element_facts.insert(element_name.to_owned(), new);
            entry.insert(CompatAttribute {
                global_facts: None,
                element_facts,
            });
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            match existing.element_facts.entry(element_name.to_owned()) {
                Entry::Vacant(entry) => {
                    entry.insert(new);
                }
                Entry::Occupied(mut entry) => {
                    merge_compat_facts(entry.get_mut(), new);
                }
            }
        }
    }
}

fn merge_global_compat_attribute(
    entry: Entry<'_, String, CompatAttribute>,
    new: CatalogCompatFacts,
) {
    match entry {
        Entry::Vacant(entry) => {
            entry.insert(CompatAttribute {
                global_facts: Some(new),
                element_facts: BTreeMap::new(),
            });
        }
        Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            if let Some(global_facts) = existing.global_facts.as_mut() {
                merge_compat_facts(global_facts, new);
            } else {
                existing.global_facts = Some(new);
            }
        }
    }
}

fn merge_compat_facts(existing: &mut CatalogCompatFacts, new: CatalogCompatFacts) {
    if existing.mdn_url.is_none() {
        existing.mdn_url.clone_from(&new.mdn_url);
    }
    existing.deprecated |= new.deprecated;
    existing.experimental |= new.experimental;
    merge_baseline(&mut existing.baseline, new.baseline);
    if let Some(new_support) = new.browser_support {
        merge_browser_support(&mut existing.browser_support, new_support);
    }
}

const fn merge_baseline(
    existing: &mut Option<CatalogBaselineStatus>,
    new: Option<CatalogBaselineStatus>,
) {
    let Some(current) = *existing else {
        *existing = new;
        return;
    };
    let Some(new) = new else {
        return;
    };

    let current_rank = baseline_rank(current);
    let new_rank = baseline_rank(new);
    if new_rank < current_rank
        || (new_rank == current_rank && baseline_since(new) > baseline_since(current))
    {
        *existing = Some(new);
    }
}

fn merge_browser_support(existing: &mut Option<CatalogBrowserSupport>, new: CatalogBrowserSupport) {
    let Some(existing) = existing.as_mut() else {
        *existing = Some(new);
        return;
    };
    merge_browser_version(&mut existing.chrome, new.chrome);
    merge_browser_version(&mut existing.edge, new.edge);
    merge_browser_version(&mut existing.firefox, new.firefox);
    merge_browser_version(&mut existing.safari, new.safari);
}

fn merge_browser_version(
    existing: &mut Option<CatalogBrowserVersion>,
    new: Option<CatalogBrowserVersion>,
) {
    let Some(new) = new else {
        return;
    };
    let Some(existing) = existing.as_mut() else {
        *existing = Some(new);
        return;
    };
    if new.supported == Some(false) {
        *existing = new;
        return;
    }
    if existing.supported == Some(false) {
        return;
    }
    existing.partial_implementation |= new.partial_implementation;
    append_missing_strings(&mut existing.notes, new.notes);
    if existing.prefix.is_none() {
        existing.prefix = new.prefix;
    }
    if existing.alternative_name.is_none() {
        existing.alternative_name = new.alternative_name;
    }
    append_missing_flags(&mut existing.flags, new.flags);
    merge_later_version(&mut existing.version_added, new.version_added);
    merge_later_version(&mut existing.version_removed, new.version_removed);
}

fn append_missing_strings(existing: &mut Vec<String>, new: Vec<String>) {
    for value in new {
        if !existing.contains(&value) {
            existing.push(value);
        }
    }
}

fn append_missing_flags(existing: &mut Vec<CatalogBrowserFlag>, new: Vec<CatalogBrowserFlag>) {
    for flag in new {
        if !existing.iter().any(|existing| existing.name == flag.name) {
            existing.push(flag);
        }
    }
}

fn merge_later_version(existing: &mut Option<String>, new: Option<String>) {
    let Some(new) = new else {
        return;
    };
    let Some(current) = existing.as_ref() else {
        *existing = Some(new);
        return;
    };
    if compare_browser_versions(&new, current).is_gt() {
        *existing = Some(new);
    }
}

fn compare_browser_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let Some((left_upper_bound, left_parts)) = parse_browser_version(left) else {
        return std::cmp::Ordering::Equal;
    };
    let Some((right_upper_bound, right_parts)) = parse_browser_version(right) else {
        return std::cmp::Ordering::Equal;
    };

    let max_len = left_parts.len().max(right_parts.len());
    for idx in 0..max_len {
        let left_part = left_parts.get(idx).copied().unwrap_or(0);
        let right_part = right_parts.get(idx).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            non_eq => return non_eq,
        }
    }

    (!left_upper_bound).cmp(&!right_upper_bound)
}

fn parse_browser_version(version: &str) -> Option<(bool, Vec<u32>)> {
    let (upper_bound, version) = version
        .strip_prefix('\u{2264}')
        .or_else(|| version.strip_prefix("<="))
        .map_or((false, version), |version| (true, version));
    let parts = version
        .split('.')
        .map(str::parse)
        .collect::<Result<Vec<u32>, _>>()
        .ok()?;
    Some((upper_bound, parts))
}

const fn baseline_rank(baseline: CatalogBaselineStatus) -> u8 {
    match baseline {
        CatalogBaselineStatus::Limited => 0,
        CatalogBaselineStatus::Newly { .. } => 1,
        CatalogBaselineStatus::Widely { .. } => 2,
    }
}

const fn baseline_since(baseline: CatalogBaselineStatus) -> u16 {
    match baseline {
        CatalogBaselineStatus::Widely { since, .. }
        | CatalogBaselineStatus::Newly { since, .. } => since,
        CatalogBaselineStatus::Limited => 0,
    }
}

fn year_from_date(date: &str) -> Option<u16> {
    let date = date
        .strip_prefix('\u{2264}')
        .or_else(|| date.strip_prefix('\u{2265}'))
        .or_else(|| date.strip_prefix("<="))
        .or_else(|| date.strip_prefix(">="))
        .or_else(|| date.strip_prefix('~'))
        .unwrap_or(date);
    date.get(..4)?.parse().ok()
}

fn bcd_attribute_name(name: &str) -> Option<String> {
    if unmodeled_feature_kind(name).is_some() {
        return None;
    }
    canonical_attribute_name(name)
}

fn unmodeled_feature_kind(name: &str) -> Option<CatalogCompatSubfeatureKind> {
    // Compatibility-normalization policy: some BCD keys describe behaviors or
    // aliases that should be retained as compat records, not promoted to first-
    // class catalog attributes.
    match name {
        "data_uri" | "external_uri" | "omit_external_fragment" | "tooltip_display" => {
            Some(CatalogCompatSubfeatureKind::Behavior)
        }
        "xlink_href" => Some(CatalogCompatSubfeatureKind::LegacyXlinkAlias),
        _ => None,
    }
}

fn canonical_attribute_name(name: &str) -> Option<String> {
    // Compatibility-normalization policy: BCD key spellings are converted here
    // before they ever merge into catalog-facing attribute facts.
    match name {
        "data_attributes" => Some("data-*".to_owned()),
        "xlink_actuate" => Some("xlink:actuate".to_owned()),
        "xlink_href" => None,
        "xlink_show" => Some("xlink:show".to_owned()),
        "xlink_title" => Some("xlink:title".to_owned()),
        "xml_lang" => Some("xml:lang".to_owned()),
        "xml_space" => Some("xml:space".to_owned()),
        "referrerPolicy" => Some("referrerpolicy".to_owned()),
        other => Some(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_support_statement_objective_fields() {
        let support = serde_json::json!({
            "version_added": "120",
            "partial_implementation": true,
            "prefix": "-webkit-",
            "flags": [{ "name": "ExampleFlag" }],
            "notes": ["one", "two"]
        });

        let Some(version) = browser_version_from_support_statement(&support) else {
            panic!("support should parse");
        };

        assert_eq!(version.supported, Some(true));
        assert_eq!(version.version_added.as_deref(), Some("120"));
        assert!(version.partial_implementation);
        assert_eq!(version.prefix.as_deref(), Some("-webkit-"));
        assert_eq!(version.flags[0].name, "ExampleFlag");
        assert_eq!(version.notes, ["one", "two"]);
    }

    #[test]
    fn support_notes_are_normalized_from_html_to_markdown() {
        let note = r#"This property is exposed but has no effect if the <code>browser.send_pings</code> preference is not set to <code>true</code>. See <a href="https://bugzil.la/951104">bug 951104</a>."#;

        assert_eq!(
            normalize_support_note(note),
            "This property is exposed but has no effect if the `browser.send_pings` preference is not set to `true`. See [bug 951104](https://bugzil.la/951104)."
        );
    }

    #[test]
    fn support_arrays_prefer_supported_entries_over_false_entries() {
        let support = serde_json::json!([
            { "version_added": false },
            { "version_added": "80" }
        ]);

        let Some(version) = browser_version_from_support(&support) else {
            panic!("support should parse");
        };

        assert_eq!(version.supported, Some(true));
        assert_eq!(version.version_added.as_deref(), Some("80"));
    }

    #[test]
    fn resolves_baseline_by_compat_key_override() {
        let web_features = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.rect", "svg.elements.rect.width"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2022-01-01",
                    "by_compat_key": {
                        "svg.elements.rect.width": {
                            "baseline": "low",
                            "baseline_low_date": "2025-01-01"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(Some(&web_features), "svg.elements.rect.width"),
            Some(CatalogBaselineStatus::Newly {
                since: 2025,
                qualifier: None
            })
        );
    }

    #[test]
    fn resolves_qualified_baseline_dates() {
        let web_features = serde_json::json!({
            "feature": {
                "compat_features": ["svg.elements.feGaussianBlur"],
                "status": {
                    "baseline": "high",
                    "baseline_high_date": "2018-01-29",
                    "by_compat_key": {
                        "svg.elements.feGaussianBlur": {
                            "baseline": "high",
                            "baseline_high_date": "\u{2264}2021-04-02"
                        }
                    }
                }
            }
        });

        assert_eq!(
            resolve_baseline(Some(&web_features), "svg.elements.feGaussianBlur"),
            Some(CatalogBaselineStatus::Widely {
                since: 2021,
                qualifier: Some(crate::catalog::CatalogBaselineQualifier::Before)
            })
        );
    }

    #[test]
    fn bcd_attribute_names_are_normalized_and_subfeatures_skipped() {
        assert_eq!(bcd_attribute_name("xml_lang").as_deref(), Some("xml:lang"));
        assert_eq!(
            bcd_attribute_name("data_attributes").as_deref(),
            Some("data-*")
        );
        assert_eq!(bcd_attribute_name("path").as_deref(), Some("path"));
        assert_eq!(bcd_attribute_name("data_uri"), None);
        assert_eq!(bcd_attribute_name("xlink_href"), None);
    }

    #[test]
    fn compat_normalization_boundary_is_explicit() {
        assert_eq!(
            unmodeled_feature_kind("xlink_href"),
            Some(CatalogCompatSubfeatureKind::LegacyXlinkAlias)
        );
        assert_eq!(
            canonical_attribute_name("referrerPolicy").as_deref(),
            Some("referrerpolicy")
        );
        assert_eq!(bcd_attribute_name("xlink_href"), None);
    }
}
