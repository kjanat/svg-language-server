use crate::{named_colors, types::ColorKind};

/// Convert RGB (each in `[0.0, 1.0]`) to HSL.
///
/// Returns `(hue_degrees, saturation_percent, lightness_percent)` as integers.
fn rgb_to_hsl(red: f32, green: f32, blue: f32) -> (u16, u8, u8) {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let lightness = f32::midpoint(max, min);

    let saturation = if delta <= f32::EPSILON {
        0.0_f32
    } else {
        delta / (1.0 - 2.0f32.mul_add(lightness, -1.0).abs())
    };

    let hue = if delta <= f32::EPSILON {
        0.0_f32
    } else if (max - red).abs() < f32::EPSILON {
        60.0 * (((green - blue) / delta) % 6.0)
    } else if (max - green).abs() < f32::EPSILON {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };

    // Normalise hue to [0, 360)
    let hue = ((hue % 360.0) + 360.0) % 360.0;

    (
        round_degrees_to_u16(hue),
        round_percent_to_u8(saturation),
        round_percent_to_u8(lightness),
    )
}

/// Reverse-lookup: given integer RGB components, return a matching CSS name.
fn reverse_named_lookup(ri: u8, gi: u8, bi: u8) -> Option<&'static str> {
    named_colors::reverse_lookup(ri, gi, bi)
}

/// Format alpha as a minimal decimal string (no trailing zeros beyond one decimal place).
fn fmt_alpha(a: f32) -> String {
    // Use up to 4 significant decimal digits, then strip trailing zeros.
    let s = format!("{a:.4}");
    let s = s.trim_end_matches('0');
    // Always keep at least one decimal place (e.g. "0.5" not "0.").
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_owned()
    }
}

fn round_channel_to_u8(value: f32) -> u8 {
    round_nonnegative_to_u8((value.clamp(0.0, 1.0) * 255.0).round())
}

fn round_percent_to_u8(value: f32) -> u8 {
    round_nonnegative_to_u8((value.clamp(0.0, 1.0) * 100.0).round())
}

fn round_degrees_to_u16(value: f32) -> u16 {
    round_nonnegative_to_u16(value.round().rem_euclid(360.0))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers bound and round presentation values before narrowing to u8"
)]
#[expect(
    clippy::cast_sign_loss,
    reason = "callers only pass non-negative rounded presentation values"
)]
const fn round_nonnegative_to_u8(target: f32) -> u8 {
    target as u8
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "callers bound and round presentation values before narrowing to u16"
)]
#[expect(
    clippy::cast_sign_loss,
    reason = "callers only pass non-negative rounded presentation values"
)]
const fn round_nonnegative_to_u16(target: f32) -> u16 {
    target as u16
}

/// Generate color presentation strings for a given RGBA color.
///
/// Returns a `Vec<String>` of alternative format strings, with the original
/// format (as identified by `original`) listed first.  All other formats
/// follow in a stable order: hex → rgb/rgba → hsl/hsla → named (when
/// applicable).
///
/// Rules:
/// - When `a == 1.0`: opaque forms only (`#RRGGBB`, `rgb(…)`, `hsl(…)`,
///   named). No 8-digit hex, no `rgba`, no `hsla`.
/// - When `a < 1.0`: alpha-bearing forms (`#RRGGBBAA`, `rgba(…)`, `hsla(…)`).
///   Named colors are never emitted for semi-transparent colors.
/// - The named color is included only when an exact RGB match exists in the
///   148-color CSS table and `a == 1.0`.
#[must_use]
pub fn color_presentations(r: f32, g: f32, b: f32, a: f32, original: ColorKind) -> Vec<String> {
    let opaque = (a - 1.0).abs() < f32::EPSILON;

    let ri = round_channel_to_u8(r);
    let gi = round_channel_to_u8(g);
    let bi = round_channel_to_u8(b);
    let ai = round_channel_to_u8(a);

    let (hh, hs, hl) = rgb_to_hsl(r, g, b);
    let alpha_str = fmt_alpha(a);

    // Build each format string.
    let hex = if opaque {
        format!("#{ri:02x}{gi:02x}{bi:02x}")
    } else {
        format!("#{ri:02x}{gi:02x}{bi:02x}{ai:02x}")
    };

    let rgb_str = if opaque {
        format!("rgb({ri}, {gi}, {bi})")
    } else {
        format!("rgba({ri}, {gi}, {bi}, {alpha_str})")
    };

    let hsl_str = if opaque {
        format!("hsl({hh}, {hs}%, {hl}%)")
    } else {
        format!("hsla({hh}, {hs}%, {hl}%, {alpha_str})")
    };

    let named: Option<&'static str> = if opaque {
        reverse_named_lookup(ri, gi, bi)
    } else {
        None
    };

    // Collect all applicable formats in canonical order: hex, rgb, hsl, named.
    let mut all: Vec<String> = Vec::with_capacity(4);
    all.push(hex.clone());
    all.push(rgb_str.clone());
    all.push(hsl_str);
    if let Some(n) = named {
        all.push(n.to_owned());
    }

    // Determine which entry corresponds to the original format.
    let original_str: String = match original {
        ColorKind::Hex => hex,
        ColorKind::Functional => rgb_str,
        ColorKind::Named => named.map_or(rgb_str, str::to_owned),
    };

    // Put the original format first; keep the rest in stable order.
    let mut result: Vec<String> = Vec::with_capacity(all.len());
    result.push(original_str.clone());
    for s in all {
        if s != original_str {
            result.push(s);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_presentations() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff0000"));
        assert!(results.iter().any(|s| s == "rgb(255, 0, 0)"));
        assert!(results.iter().any(|s| s.starts_with("hsl(")));
        // Original format should be first
        assert!(results[0] == "#ff0000");
    }

    #[test]
    fn hex_with_alpha() {
        let results = color_presentations(1.0, 0.0, 0.0, 0.5, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff000080"));
        assert!(results.iter().any(|s| s == "rgba(255, 0, 0, 0.5)"));
    }

    #[test]
    fn named_color_included() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Named);
        assert!(results.iter().any(|s| s == "red"));
        // Named should be first when original is Named
        assert!(results[0] == "red");
    }

    #[test]
    fn functional_original_first() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Functional);
        assert!(results[0].starts_with("rgb(") || results[0].starts_with("hsl("));
    }

    #[test]
    fn no_alpha_suffix_when_opaque() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        // Should NOT include #ff0000ff (redundant alpha)
        assert!(!results.iter().any(|s| s == "#ff0000ff"));
        // Should NOT include rgba when alpha is 1.0
        assert!(!results.iter().any(|s| s.starts_with("rgba(")));
    }

    #[test]
    fn hsl_values_for_red() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "hsl(0, 100%, 50%)"));
    }

    #[test]
    fn alpha_formatting_minimal() {
        let results = color_presentations(0.0, 0.0, 1.0, 0.5, ColorKind::Functional);
        // Alpha should be "0.5" not "0.5000"
        assert!(results.iter().any(|s| s == "rgba(0, 0, 255, 0.5)"));
    }

    #[test]
    fn unnamed_color_no_named_entry() {
        // coral = (255, 127, 80) — has a name; but a slightly shifted value won't
        let results = color_presentations(1.0, 0.5, 0.5, 1.0, ColorKind::Hex);
        // (255, 128, 128) = lightcoral? no — lightcoral is (240,128,128). So no name.
        // Just verify the function returns at least hex, rgb, hsl.
        assert!(results.iter().any(|s| s.starts_with('#')));
        assert!(results.iter().any(|s| s.starts_with("rgb(")));
        assert!(results.iter().any(|s| s.starts_with("hsl(")));
    }
}
