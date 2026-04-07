use std::collections::HashSet;

use super::{ColorStop, CustomProperties, ResolvedColor};
use crate::{named_colors, parse, types::ColorKind};

pub(super) fn resolve_css_color(
    text: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<ResolvedColor> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    if let Some(color) = parse_literal_css_color(text) {
        return Some(color);
    }

    let (function, args) = parse_css_function_call(text)?;
    match function.as_str() {
        "var" => resolve_var_color(args, custom_properties, seen),
        "color-mix" => resolve_color_mix(args, custom_properties, seen),
        _ => None,
    }
}

pub(super) fn parse_named_color(text: &str) -> Option<(f32, f32, f32, f32, ColorKind)> {
    if text.eq_ignore_ascii_case("transparent") {
        return Some((0.0, 0.0, 0.0, 0.0, ColorKind::Named));
    }

    let (r, g, b) = named_colors::lookup(text)?;
    Some((r, g, b, 1.0, ColorKind::Named))
}

fn parse_literal_css_color(text: &str) -> Option<ResolvedColor> {
    if let Some((r, g, b, a)) = parse::hex(text) {
        return Some((r, g, b, a, ColorKind::Hex));
    }
    if let Some((r, g, b, a)) = parse::functional(text) {
        return Some((r, g, b, a, ColorKind::Functional));
    }
    parse_named_color(text)
}

fn resolve_var_color(
    args: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<ResolvedColor> {
    let parts = split_top_level(args, ',');
    let name = parts.first()?.trim();
    if !name.starts_with("--") {
        return None;
    }

    if let Some(value) = custom_properties.get(name) {
        if !seen.insert(name.to_owned()) {
            return None;
        }
        let resolved = resolve_css_color(value, custom_properties, seen);
        seen.remove(name);
        return resolved;
    }

    parts
        .get(1)
        .and_then(|fallback| resolve_css_color(fallback.trim(), custom_properties, seen))
}

fn resolve_color_mix(
    args: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<ResolvedColor> {
    let parts = split_top_level(args, ',');
    let [space_part, left_stop, right_stop]: [&str; 3] = parts.try_into().ok()?;
    let mut pieces = space_part.split_whitespace();
    if !pieces.next()?.eq_ignore_ascii_case("in") {
        return None;
    }
    let space = pieces.collect::<Vec<_>>().join(" ");
    if space.is_empty() {
        return None;
    }

    let (left, left_pct) = parse_color_mix_stop(left_stop, custom_properties, seen)?;
    let (right, right_pct) = parse_color_mix_stop(right_stop, custom_properties, seen)?;
    let (left_weight, right_weight, alpha_scale) = resolve_mix_weights(left_pct, right_pct)?;

    let mut mixed = parse::mix_colors(&space, left, left_weight, right, right_weight)?;
    mixed.3 = parse::clamp_channel(f64::from(mixed.3) * alpha_scale);
    Some((mixed.0, mixed.1, mixed.2, mixed.3, ColorKind::Functional))
}

fn parse_color_mix_stop(
    stop: &str,
    custom_properties: &CustomProperties,
    seen: &mut HashSet<String>,
) -> Option<ColorStop> {
    let (color_text, percentage) = split_color_stop_percentage(stop.trim());
    let (r, g, b, a, _) = resolve_css_color(color_text, custom_properties, seen)?;
    Some(((r, g, b, a), percentage))
}

/// `resolve_mix_weights` follows CSS Color 4 `color-mix()` defaults: explicit
/// percentages stay explicit, one percentage implies `100 - other`, omitted
/// percentages default to `50/50`, and negative percentages are invalid.
/// Returns `(left_fraction, right_fraction, alpha_scale)`, where the fractions
/// sum to `1.0` and `alpha_scale` is `(total / 100.0).min(1.0)`.
fn resolve_mix_weights(left_pct: Option<f64>, right_pct: Option<f64>) -> Option<(f64, f64, f64)> {
    let mut left = left_pct;
    let mut right = right_pct;

    match (left, right) {
        (Some(l), Some(r)) => {
            if l < 0.0 || r < 0.0 {
                return None;
            }
        }
        (Some(l), None) => {
            if !(0.0..=100.0).contains(&l) {
                return None;
            }
            right = Some(100.0 - l);
        }
        (None, Some(r)) => {
            if !(0.0..=100.0).contains(&r) {
                return None;
            }
            left = Some(100.0 - r);
        }
        (None, None) => {
            left = Some(50.0);
            right = Some(50.0);
        }
    }

    let left = left?;
    let right = right?;
    let total = left + right;
    if total <= 0.0 {
        return None;
    }

    Some((left / total, right / total, (total / 100.0).min(1.0)))
}

fn split_color_stop_percentage(stop: &str) -> (&str, Option<f64>) {
    let mut depth = 0usize;
    let mut last_space = None;

    for (idx, ch) in stop.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => last_space = Some(idx),
            _ => {}
        }
    }

    let Some(space_idx) = last_space else {
        return (stop, None);
    };
    let color = stop[..space_idx].trim();
    let candidate = stop[space_idx..].trim();
    if color.is_empty() {
        return (stop, None);
    }

    parse_mix_percentage(candidate).map_or((stop, None), |percentage| (color, Some(percentage)))
}

fn parse_mix_percentage(text: &str) -> Option<f64> {
    let pct = text.strip_suffix('%')?.trim();
    let value: f64 = pct.parse().ok()?;
    if value.is_finite() { Some(value) } else { None }
}

fn parse_css_function_call(text: &str) -> Option<(String, &str)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close != text.len() - 1 {
        return None;
    }
    let function = text[..open].trim().to_ascii_lowercase();
    if function.is_empty() {
        return None;
    }
    Some((function, text[open + 1..close].trim()))
}

fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == separator && depth == 0 => {
                parts.push(text[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(text[start..].trim());
    parts
}
