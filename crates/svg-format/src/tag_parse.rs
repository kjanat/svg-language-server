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

fn canonical_attribute_sort_key(name: &str) -> (u8, u16, String) {
    let lowered = name.to_ascii_lowercase();

    if lowered == "xmlns" {
        return (0, 0, lowered);
    }
    if lowered.starts_with("xmlns:") {
        return (0, 1, lowered);
    }
    if lowered == "id" {
        return (1, 0, lowered);
    }
    if lowered == "class" {
        return (1, 1, lowered);
    }
    if let Some(order) = canonical_geometry_order(&lowered) {
        return (2, order, lowered);
    }
    (3, u16::MAX, lowered)
}

fn canonical_geometry_order(name: &str) -> Option<u16> {
    // Common SVG geometry/presentation progression before fallback alphabetical ordering.
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
        "href",
        "xlink:href",
        "d",
        "points",
        "transform",
        "fill",
        "stroke",
        "stroke-width",
        "style",
    ];
    order
        .iter()
        .position(|candidate| *candidate == name)
        .and_then(|i| u16::try_from(i).ok())
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
                while j < bytes.len() {
                    if bytes[j] == quote {
                        j += 1;
                        break;
                    }
                    j += 1;
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
            .and_then(|value| value.strip_suffix('"'))
            .map_or_else(
                || {
                    raw_value
                        .strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                        .map_or_else(
                            || ParsedAttributeValue {
                                raw: raw_value.to_string(),
                                original_quote: None,
                            },
                            |inner| ParsedAttributeValue {
                                raw: inner.to_string(),
                                original_quote: Some('\''),
                            },
                        )
                },
                |inner| ParsedAttributeValue {
                    raw: inner.to_string(),
                    original_quote: Some('"'),
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
