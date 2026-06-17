//! Per-edition inventory extraction from authoritative element/attribute indexes.

use std::collections::{BTreeMap, BTreeSet};

use tl::{HTMLTag, Parser, ParserOptions};

use crate::{
    catalog::{CatalogInventory, CatalogInventoryElement, CatalogSpecSnapshotId},
    extract::Definitions,
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;
type AttributeInventory = (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>);

/// Element/attribute index pages for one curated snapshot.
pub struct EditionIndexSource {
    /// Human-readable source label.
    pub name: &'static str,
    /// Profile represented by these indexes.
    pub profile: CatalogSpecSnapshotId,
    /// Exact W3C element-index URL.
    pub element_index_url: &'static str,
    /// Exact W3C attribute-index URL.
    pub attribute_index_url: &'static str,
}

/// Snapshot index pages used for profile-aware presence.
pub const SNAPSHOT_INDEX_SOURCES: &[EditionIndexSource] = &[
    EditionIndexSource {
        name: "SVG 1.1 First Edition indexes",
        profile: CatalogSpecSnapshotId::Svg11Rec20030114,
        element_index_url: "https://www.w3.org/TR/2003/REC-SVG11-20030114/eltindex.html",
        attribute_index_url: "https://www.w3.org/TR/2003/REC-SVG11-20030114/attindex.html",
    },
    EditionIndexSource {
        name: "SVG 1.1 Second Edition indexes",
        profile: CatalogSpecSnapshotId::Svg11Rec20110816,
        element_index_url: "https://www.w3.org/TR/SVG11/eltindex.html",
        attribute_index_url: "https://www.w3.org/TR/SVG11/attindex.html",
    },
    EditionIndexSource {
        name: "SVG 2 Candidate Recommendation 2018 indexes",
        profile: CatalogSpecSnapshotId::Svg2Cr20181004,
        element_index_url: "https://www.w3.org/TR/2018/CR-SVG2-20181004/eltindex.html",
        attribute_index_url: "https://www.w3.org/TR/2018/CR-SVG2-20181004/attindex.html",
    },
];

/// Extract one snapshot inventory from its element and attribute indexes.
///
/// # Errors
/// Returns an error if either page cannot be parsed or if the extracted
/// inventory is empty.
pub fn extract_index_inventory(
    source: &EditionIndexSource,
    element_html: &str,
    attribute_html: &str,
) -> Fallible<CatalogInventory> {
    let element_names = extract_element_index(element_html)?;
    if element_names.is_empty() {
        return Err(boxed("element index had no elements"));
    }

    let (attributes, mut element_attributes) = extract_attribute_index(attribute_html)?;
    if attributes.is_empty() {
        return Err(boxed("attribute index had no attributes"));
    }

    for element in &element_names {
        element_attributes.entry(element.clone()).or_default();
    }

    Ok(CatalogInventory {
        profile: source.profile,
        sources: vec![
            source.element_index_url.to_owned(),
            source.attribute_index_url.to_owned(),
        ],
        elements: inventory_elements(element_attributes),
        attributes: attributes.into_iter().collect(),
    })
}

/// Build the rolling SVG 2 editor's-draft inventory from fetched definitions.
pub fn inventory_from_definitions(
    profile: CatalogSpecSnapshotId,
    modules: &[Definitions],
) -> CatalogInventory {
    let mut category_attributes: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for module in modules {
        for category in &module.attribute_categories {
            let entry = category_attributes
                .entry(category.name.as_str())
                .or_default();
            entry.extend(
                category
                    .attributes
                    .iter()
                    .map(|attribute| canonical_inventory_attribute_name(&attribute.name)),
            );
            entry.extend(
                category
                    .presentation_attributes
                    .iter()
                    .map(|attribute| canonical_inventory_attribute_name(attribute)),
            );
        }
    }

    let mut element_attributes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut attributes = BTreeSet::new();
    for module in modules {
        for attribute in &module.global_attributes {
            attributes.insert(canonical_inventory_attribute_name(&attribute.name));
        }
        for element in &module.elements {
            let entry = element_attributes.entry(element.name.clone()).or_default();
            entry.extend(
                element
                    .attributes
                    .iter()
                    .map(|attribute| canonical_inventory_attribute_name(&attribute.name)),
            );
            entry.extend(
                element
                    .common_attributes
                    .iter()
                    .map(|attribute| canonical_inventory_attribute_name(attribute)),
            );
            entry.extend(
                element
                    .geometry_properties
                    .iter()
                    .map(|attribute| canonical_inventory_attribute_name(attribute)),
            );
            for category in &element.attribute_categories {
                if let Some(category_attrs) = category_attributes.get(category.as_str()) {
                    entry.extend(category_attrs.iter().cloned());
                }
            }
            attributes.extend(entry.iter().cloned());
        }
    }

    CatalogInventory {
        profile,
        sources: Vec::new(),
        elements: inventory_elements(element_attributes),
        attributes: attributes.into_iter().collect(),
    }
}

fn extract_element_index(html: &str) -> Fallible<BTreeSet<String>> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut names = BTreeSet::new();
    for tag in tags_with_class(&dom, parser, "element-name") {
        if let Some(name) = inventory_name_from_tag(tag, parser) {
            names.insert(name);
        }
    }
    if names.is_empty() {
        names = element_names_from_links(&dom, parser);
    }
    Ok(names)
}

fn extract_attribute_index(html: &str) -> Fallible<AttributeInventory> {
    let dom = tl::parse(html, ParserOptions::default())?;
    let parser = dom.parser();
    let mut attributes = BTreeSet::new();
    for tag in tags_with_class(&dom, parser, "attr-name") {
        if let Some(name) = inventory_name_from_tag(tag, parser) {
            attributes.insert(name);
        }
    }
    if attributes.is_empty() {
        return Ok(extract_legacy_attribute_index(&dom, parser));
    }

    let mut element_attributes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in dom.nodes() {
        let Some(row) = node.as_tag() else {
            continue;
        };
        if row.name().as_utf8_str() != "tr" {
            continue;
        }
        let Some((row_attributes, elements)) = row_attributes_and_elements(row, parser) else {
            continue;
        };
        for element in elements {
            let entry = element_attributes.entry(element).or_default();
            entry.extend(row_attributes.iter().cloned());
        }
    }

    Ok((attributes, element_attributes))
}

fn extract_legacy_attribute_index(
    dom: &tl::VDom<'_>,
    parser: &Parser,
) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
    let mut attributes = BTreeSet::new();
    let mut element_attributes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in dom.nodes() {
        let Some(row) = node.as_tag() else {
            continue;
        };
        if row.name().as_utf8_str() != "tr" {
            continue;
        }
        let cells: Vec<HTMLTag> = row
            .children()
            .top()
            .iter()
            .filter_map(|handle| handle.get(parser)?.as_tag().cloned())
            .filter(|tag| matches!(tag.name().as_utf8_str().as_ref(), "td" | "th"))
            .collect();
        let Some(name) = cells
            .first()
            .and_then(|cell| legacy_attribute_name(cell, parser))
        else {
            continue;
        };
        attributes.insert(name.clone());
        if let Some(element_cell) = cells.get(1) {
            for element in element_names_from_tag_links(element_cell, parser) {
                element_attributes
                    .entry(element)
                    .or_default()
                    .insert(name.clone());
            }
        }
    }
    (attributes, element_attributes)
}

fn row_attributes_and_elements(
    row: &HTMLTag,
    parser: &Parser,
) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let cells: Vec<HTMLTag> = row
        .children()
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser)?.as_tag().cloned())
        .filter(|tag| matches!(tag.name().as_utf8_str().as_ref(), "td" | "th"))
        .collect();
    let attr_cell = cells.first()?;
    let element_cell = cells.get(1)?;
    let attributes = descendant_names_with_class(attr_cell, parser, "attr-name");
    let elements = descendant_names_with_class(element_cell, parser, "element-name");
    (!attributes.is_empty() && !elements.is_empty()).then_some((attributes, elements))
}

fn descendant_names_with_class(
    tag: &HTMLTag,
    parser: &Parser,
    class_name: &str,
) -> BTreeSet<String> {
    tag.query_selector(parser, &format!(".{class_name}"))
        .into_iter()
        .flatten()
        .filter_map(|handle| handle.get(parser)?.as_tag())
        .filter_map(|tag| inventory_name_from_tag(tag, parser))
        .collect()
}

fn element_names_from_links(dom: &tl::VDom<'_>, parser: &Parser) -> BTreeSet<String> {
    dom.nodes()
        .iter()
        .filter_map(|node| node.as_tag())
        .filter(|tag| tag.name().as_utf8_str() == "a")
        .filter_map(|tag| element_name_from_link(tag, parser))
        .collect()
}

fn element_names_from_tag_links(tag: &HTMLTag, parser: &Parser) -> BTreeSet<String> {
    tag.query_selector(parser, "a")
        .into_iter()
        .flatten()
        .filter_map(|handle| handle.get(parser)?.as_tag())
        .filter_map(|tag| element_name_from_link(tag, parser))
        .collect()
}

fn element_name_from_link(tag: &HTMLTag, parser: &Parser) -> Option<String> {
    let href = tag.attributes().get("href")??.as_utf8_str();
    href.contains("Element")
        .then(|| normalize_inventory_name(&tag.inner_text(parser)))
        .filter(|name| !name.is_empty())
}

fn legacy_attribute_name(tag: &HTMLTag, parser: &Parser) -> Option<String> {
    let name = normalize_inventory_name(&tag.inner_text(parser));
    is_inventory_attribute_name(&name).then_some(name)
}

fn is_inventory_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('%')
        && !name.starts_with('#')
        && !name.contains(char::is_whitespace)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_'))
        && name.bytes().any(|byte| byte.is_ascii_lowercase())
}

fn tags_with_class<'a>(
    dom: &'a tl::VDom<'a>,
    parser: &'a Parser,
    class_name: &'a str,
) -> impl Iterator<Item = &'a HTMLTag<'a>> + 'a {
    dom.nodes()
        .iter()
        .filter_map(|node| node.as_tag())
        .filter(move |tag| tag_has_class(tag, class_name, parser))
}

fn tag_has_class(tag: &HTMLTag, class_name: &str, _parser: &Parser) -> bool {
    tag.attributes().class().is_some_and(|class| {
        class
            .as_utf8_str()
            .split_whitespace()
            .any(|item| item == class_name)
    })
}

fn inventory_name_from_tag(tag: &HTMLTag, parser: &Parser) -> Option<String> {
    let name = normalize_inventory_name(&tag.inner_text(parser));
    (!name.is_empty()).then_some(name)
}

fn normalize_inventory_name(text: &str) -> String {
    normalize_ws(text)
        .trim_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '\'' | '"' | '`' | '‘' | '’' | '“' | '”')
        })
        .to_owned()
}

fn inventory_elements(
    element_attributes: BTreeMap<String, BTreeSet<String>>,
) -> Vec<CatalogInventoryElement> {
    element_attributes
        .into_iter()
        .map(|(name, attributes)| CatalogInventoryElement {
            name,
            attributes: attributes.into_iter().collect(),
        })
        .collect()
}

fn canonical_inventory_attribute_name(name: &str) -> String {
    crate::catalog::canonical_attribute_name(name).into_owned()
}

fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn boxed(message: &str) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_elements_and_attribute_bearers_from_indexes() -> Fallible<()> {
        let elements = r#"
            <ul>
              <li><span class="element-name">‘<a><span>a</span></a>’</span></li>
              <li><a><span class="element-name">‘circle’</span></a></li>
            </ul>
        "#;
        let attributes = r#"
            <table>
              <tr>
                <th><span class="attr-name"><a><span>href</span></a></span></th>
                <td><span class="element-name"><a><span>a</span></a></span></td>
              </tr>
              <tr>
                <td><span class="attr-name">‘fill’</span>, <span class="attr-name">‘stroke’</span></td>
                <td><span class="element-name">‘a’</span>, <span class="element-name">‘circle’</span></td>
              </tr>
            </table>
        "#;
        let source = EditionIndexSource {
            name: "test",
            profile: CatalogSpecSnapshotId::Svg2Cr20181004,
            element_index_url: "https://example.test/eltindex.html",
            attribute_index_url: "https://example.test/attindex.html",
        };

        let inventory = extract_index_inventory(&source, elements, attributes)?;
        assert_eq!(inventory.attributes, ["fill", "href", "stroke"]);
        let circle = inventory
            .elements
            .iter()
            .find(|element| element.name == "circle")
            .ok_or("missing circle")?;
        assert_eq!(circle.attributes, ["fill", "stroke"]);
        Ok(())
    }

    #[test]
    fn extracts_legacy_xhtml_indexes_without_classed_names() -> Fallible<()> {
        let elements = r#"
            <ul>
              <li><a href="linking.html#AElement">a</a></li>
              <li><a href="shapes.html#CircleElement">circle</a></li>
            </ul>
        "#;
        let attributes = r#"
            <table>
              <tr>
                <td>%PresentationAttributes-All;</td>
                <td><a href="linking.html#AElement">a</a></td>
              </tr>
              <tr>
                <td>xlink:href</td>
                <td><a href="linking.html#AElement">a</a></td>
              </tr>
              <tr>
                <td>cx</td>
                <td><a href="shapes.html#CircleElement">circle</a></td>
              </tr>
            </table>
        "#;
        let source = EditionIndexSource {
            name: "test",
            profile: CatalogSpecSnapshotId::Svg11Rec20030114,
            element_index_url: "https://example.test/eltindex.html",
            attribute_index_url: "https://example.test/attindex.html",
        };

        let inventory = extract_index_inventory(&source, elements, attributes)?;
        assert_eq!(inventory.attributes, ["cx", "xlink:href"]);
        let link = inventory
            .elements
            .iter()
            .find(|element| element.name == "a")
            .ok_or("missing a")?;
        assert_eq!(link.attributes, ["xlink:href"]);
        Ok(())
    }
}
