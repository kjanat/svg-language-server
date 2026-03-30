/// Remove common leading whitespace from a block of text,
/// trimming leading/trailing blank lines.
pub fn dedent_block(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first_non_empty = lines.iter().position(|l| !l.trim().is_empty());
    let last_non_empty = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(start), Some(end)) = (first_non_empty, last_non_empty) else {
        return String::new();
    };

    let block = &lines[start..=end];
    let min_indent = block
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    block
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else {
                let skip: usize = l.chars().take(min_indent).map(char::len_utf8).sum();
                &l[skip..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collapse runs of whitespace into single spaces and trim.
pub fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_ws = true; // treat start as whitespace to trim leading
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }
    // trim trailing space
    if result.ends_with(' ') {
        result.pop();
    }
    result
}

/// SVG elements whose content is whitespace-sensitive inline text.
///
/// For these elements the formatter preserves raw content between the start
/// and end tags as a single text block instead of formatting each child node
/// on its own line (which would break entity references like `&lt;` apart
/// from surrounding text).
pub fn is_text_content_element(tag_name: &str) -> bool {
    matches!(tag_name, "text" | "tspan" | "textPath" | "title" | "desc")
}

#[derive(Clone, Copy)]
enum TextContentToken<'a> {
    Text(&'a str),
    Entity(&'a str),
    Whitespace(&'a str),
}

pub fn normalize_text_content_with_entities(text: &str) -> String {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        let rest = &text[offset..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch.is_whitespace() {
            let start = offset;
            offset += ch.len_utf8();
            while offset < text.len() {
                let Some(next) = text[offset..].chars().next() else {
                    break;
                };
                if !next.is_whitespace() {
                    break;
                }
                offset += next.len_utf8();
            }
            tokens.push(TextContentToken::Whitespace(&text[start..offset]));
            continue;
        }

        if ch == '&'
            && let Some(len) = entity_reference_len(rest)
        {
            tokens.push(TextContentToken::Entity(&text[offset..offset + len]));
            offset += len;
            continue;
        }

        let start = offset;
        offset += ch.len_utf8();
        while offset < text.len() {
            let Some(next) = text[offset..].chars().next() else {
                break;
            };
            if next.is_whitespace() {
                break;
            }
            if next == '&' && entity_reference_len(&text[offset..]).is_some() {
                break;
            }
            offset += next.len_utf8();
        }
        tokens.push(TextContentToken::Text(&text[start..offset]));
    }

    let mut normalized = String::new();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            TextContentToken::Text(text) | TextContentToken::Entity(text) => {
                normalized.push_str(text);
            }
            TextContentToken::Whitespace(space) => {
                let prev = tokens[..index]
                    .iter()
                    .rev()
                    .find(|token| !matches!(token, TextContentToken::Whitespace(_)));
                let next = tokens[index + 1..]
                    .iter()
                    .find(|token| !matches!(token, TextContentToken::Whitespace(_)));

                let (Some(prev), Some(next)) = (prev, next) else {
                    continue;
                };

                if should_strip_entity_boundary_space(*prev, *next, space) {
                    continue;
                }

                if !normalized.ends_with(' ') {
                    normalized.push(' ');
                }
            }
        }
    }

    normalized.trim().to_string()
}

fn should_strip_entity_boundary_space(
    prev: TextContentToken<'_>,
    next: TextContentToken<'_>,
    whitespace: &str,
) -> bool {
    if !whitespace.contains(['\n', '\r']) {
        return false;
    }

    matches!(prev, TextContentToken::Entity(entity) if is_open_angle_entity(entity))
        && matches!(next, TextContentToken::Text(_))
        || matches!(prev, TextContentToken::Text(_))
            && matches!(next, TextContentToken::Entity(entity) if is_close_angle_entity(entity))
}

fn entity_reference_len(text: &str) -> Option<usize> {
    let end = text.find(';')?;
    let candidate = &text[..=end];
    let body = &candidate[1..candidate.len() - 1];
    if body.is_empty() {
        return None;
    }

    let valid = body
        .strip_prefix("#x")
        .or_else(|| body.strip_prefix("#X"))
        .map_or_else(
            || {
                body.strip_prefix('#').map_or_else(
                    || body.chars().all(|ch| ch.is_ascii_alphanumeric()),
                    |decimal| !decimal.is_empty() && decimal.chars().all(|ch| ch.is_ascii_digit()),
                )
            },
            |hex| !hex.is_empty() && hex.chars().all(|ch| ch.is_ascii_hexdigit()),
        );

    valid.then_some(candidate.len())
}

fn is_open_angle_entity(entity: &str) -> bool {
    matches!(
        entity.to_ascii_lowercase().as_str(),
        "&lt;" | "&#60;" | "&#x3c;"
    )
}

fn is_close_angle_entity(entity: &str) -> bool {
    matches!(
        entity.to_ascii_lowercase().as_str(),
        "&gt;" | "&#62;" | "&#x3e;"
    )
}
