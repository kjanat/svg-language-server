use crate::AttributeSort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTag {
    pub name: String,
    pub attributes: Vec<ParsedAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAttribute {
    pub name: String,
    pub value: Option<ParsedAttributeValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAttributeValue {
    pub raw: String,
    pub original_quote: Option<char>,
}

pub fn reorder_attributes(attributes: &mut [ParsedAttribute], mode: AttributeSort) {
    match mode {
        AttributeSort::None => {}
        AttributeSort::Alphabetical => {
            attributes.sort_by_key(|attribute| attribute.name.to_ascii_lowercase());
        }
        AttributeSort::Canonical => {
            attributes.sort_by_key(|attribute| canonical_attribute_sort_key(&attribute.name));
        }
    }
}

/// Canonical attribute ordering groups. Members of the same group are a
/// contiguous run in the output; the multi-line wrap algorithm breaks at
/// boundaries between these groups. Order here is authoritative.
#[repr(u8)]
enum CanonicalGroup {
    Identity = 0,
    Geometry = 1,
    Drawing = 2,
    Reference = 3,
    Presentation = 4,
    Other = 5,
    Namespace = 6,
    Version = 7,
}

pub fn canonical_group_key(name: &str) -> u8 {
    let lowered = name.to_ascii_lowercase();
    canonical_attribute_sort_key(&lowered).0
}

fn canonical_attribute_sort_key(name: &str) -> (u8, u16, String) {
    let lowered = name.to_ascii_lowercase();

    // Group layout matches the W3 SVG reference samples' convention:
    //   id/class → geometry → drawing → refs → presentation → other → xmlns* → version
    // `xmlns` and `version` trail at the end because they describe the
    // document envelope, not per-element structure; the W3 spec examples
    // put them last on the root `<svg>` tag for readability.
    if lowered == "id" {
        return (CanonicalGroup::Identity as u8, 0, lowered);
    }
    if lowered == "class" {
        return (CanonicalGroup::Identity as u8, 1, lowered);
    }
    if let Some(order) = canonical_geometry_order(&lowered) {
        return (CanonicalGroup::Geometry as u8, order, lowered);
    }
    if let Some(order) = canonical_drawing_order(&lowered) {
        return (CanonicalGroup::Drawing as u8, order, lowered);
    }
    if let Some(order) = canonical_reference_order(&lowered) {
        return (CanonicalGroup::Reference as u8, order, lowered);
    }
    if let Some(order) = canonical_presentation_order(&lowered) {
        return (CanonicalGroup::Presentation as u8, order, lowered);
    }
    if lowered == "version" {
        return (CanonicalGroup::Version as u8, 0, lowered);
    }
    if lowered == "xmlns" {
        return (CanonicalGroup::Namespace as u8, 0, lowered);
    }
    if lowered.starts_with("xmlns:") {
        return (CanonicalGroup::Namespace as u8, 1, lowered);
    }
    (CanonicalGroup::Other as u8, u16::MAX, lowered)
}

fn canonical_geometry_order(name: &str) -> Option<u16> {
    let order = [
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "width",
        "height",
        "viewbox",
        "preserveaspectratio",
    ];
    order
        .iter()
        .position(|candidate| *candidate == name)
        .and_then(|i| u16::try_from(i).ok())
}

fn canonical_drawing_order(name: &str) -> Option<u16> {
    let order = ["d", "points", "transform"];
    order
        .iter()
        .position(|candidate| *candidate == name)
        .and_then(|i| u16::try_from(i).ok())
}

fn canonical_reference_order(name: &str) -> Option<u16> {
    let order = ["href", "xlink:href"];
    order
        .iter()
        .position(|candidate| *candidate == name)
        .and_then(|i| u16::try_from(i).ok())
}

fn canonical_presentation_order(name: &str) -> Option<u16> {
    // Fixed-order anchors for the most common SVG presentation attributes.
    // Anything else matching the `stroke-*` prefix or a known presentation
    // property falls through to an alphabetical slot after the anchors.
    let anchors = ["fill", "stroke", "stroke-width", "opacity", "style"];
    if let Some(i) = anchors.iter().position(|candidate| *candidate == name) {
        return u16::try_from(i).ok();
    }
    if name.starts_with("stroke-")
        || matches!(
            name,
            "fill-opacity"
                | "fill-rule"
                | "stroke-opacity"
                | "color"
                | "visibility"
                | "display"
                | "paint-order"
                | "vector-effect"
                | "shape-rendering"
                | "image-rendering"
                | "text-rendering"
                | "color-interpolation"
                | "color-interpolation-filters"
        )
    {
        return Some(u16::MAX - 1);
    }
    None
}

pub fn parse_tag(raw: &str, self_closing: bool) -> Option<ParsedTag> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('<') {
        return None;
    }

    let inner = if self_closing {
        if let Some(stripped) = trimmed.strip_suffix("/>") {
            stripped
        } else {
            trimmed.strip_suffix(" />")?
        }
    } else {
        trimmed.strip_suffix('>')?
    };
    let inner = inner.strip_prefix('<')?.trim();
    if inner.is_empty() {
        return None;
    }

    let mut i = 0usize;
    let bytes = inner.as_bytes();
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == 0 {
        return None;
    }

    let name = inner[..i].to_string();
    let mut attrs = Vec::new();
    let mut j = i;

    while j < bytes.len() {
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }

        let start = j;
        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'=' {
            j += 1;
        }
        if start == j {
            break;
        }

        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }

        if j < bytes.len() && bytes[j] == b'=' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                let found = loop {
                    if j >= bytes.len() {
                        break false;
                    }
                    if bytes[j] == quote {
                        j += 1;
                        break true;
                    }
                    j += 1;
                };
                if !found {
                    return None;
                }
            } else {
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }
        }

        let attribute = inner[start..j].trim();
        if !attribute.is_empty() {
            attrs.push(parse_attribute(attribute));
        }
    }

    Some(ParsedTag {
        name,
        attributes: attrs,
    })
}

pub fn parse_attribute(attribute: &str) -> ParsedAttribute {
    let trimmed = attribute.trim();
    if let Some((name, raw_value)) = trimmed.split_once('=') {
        let name = name.trim().to_string();
        let raw_value = raw_value.trim();
        let value = raw_value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .map(|inner| (inner, '"'))
            .or_else(|| {
                raw_value
                    .strip_prefix('\'')
                    .and_then(|v| v.strip_suffix('\''))
                    .map(|inner| (inner, '\''))
            })
            .map_or_else(
                || ParsedAttributeValue {
                    raw: raw_value.to_string(),
                    original_quote: None,
                },
                |(inner, quote)| ParsedAttributeValue {
                    raw: inner.to_string(),
                    original_quote: Some(quote),
                },
            );

        ParsedAttribute {
            name,
            value: Some(value),
        }
    } else {
        ParsedAttribute {
            name: trimmed.to_string(),
            value: None,
        }
    }
}
