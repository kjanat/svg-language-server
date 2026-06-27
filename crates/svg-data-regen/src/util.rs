//! Small shared helpers for the regeneration pipeline.

use std::borrow::Cow;

use regex::Regex;

/// Compile a regex pattern that is fixed at compile time.
///
/// # Panics
/// Panics when `pattern` is not a valid regex.
pub fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid regex {pattern:?}: {error}"),
    }
}

/// Wrap a message as a boxed error.
pub fn boxed(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(message.into())
}

/// Collapse whitespace runs into single ASCII spaces.
pub fn normalize_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode HTML entities, then collapse whitespace runs into single spaces.
pub fn normalize_html_ws(text: &str) -> String {
    normalize_ws(decode_html_entities(text).as_ref())
}

/// Decode the HTML entities the upstream spec and MDN prose use.
///
/// Named basics plus numeric references are decoded in a single pass, while
/// unrecognized `&...;` runs are left verbatim.
pub fn decode_html_entities(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';')
            && let Some(decoded) = decode_html_entity(&after[1..semi])
        {
            out.push(decoded);
            rest = &after[semi + 1..];
            continue;
        }
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{00A0}'),
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}

/// Whether a value grammar token is a bare keyword.
pub fn is_keyword_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_html_entities_handles_named_and_numeric() {
        assert_eq!(decode_html_entities("a&lt;b&gt;c").as_ref(), "a<b>c");
        assert_eq!(decode_html_entities("&amp;&quot;").as_ref(), "&\"");
        assert_eq!(decode_html_entities("x&#65;y").as_ref(), "xAy");
        assert_eq!(decode_html_entities("x&#x41;y").as_ref(), "xAy");
        assert_eq!(decode_html_entities("x&#39;y").as_ref(), "x'y");
        assert_eq!(decode_html_entities("plain text").as_ref(), "plain text");
        assert_eq!(decode_html_entities("a&bogus;b").as_ref(), "a&bogus;b");
    }

    #[test]
    fn normalize_html_ws_decodes_then_collapses() {
        assert_eq!(
            normalize_html_ws(" auto | &lt;length&gt;\n"),
            "auto | <length>"
        );
    }
}
