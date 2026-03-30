mod mix;
mod space;

const LCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 0.0015;
const OKLCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 4e-6;

/// Parse a hex color string like `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
#[must_use]
pub fn hex(text: &str) -> Option<(f32, f32, f32, f32)> {
    let hex = text.strip_prefix('#')?;

    let to_f32 = |b: u8| f32::from(b) / 255.0;

    match hex.len() {
        3 => {
            let r = hex_nibble_pair(hex, 0)?;
            let g = hex_nibble_pair(hex, 1)?;
            let b = hex_nibble_pair(hex, 2)?;
            Some((to_f32(r), to_f32(g), to_f32(b), 1.0))
        }
        4 => {
            let r = hex_nibble_pair(hex, 0)?;
            let g = hex_nibble_pair(hex, 1)?;
            let b = hex_nibble_pair(hex, 2)?;
            let a = hex_nibble_pair(hex, 3)?;
            Some((to_f32(r), to_f32(g), to_f32(b), to_f32(a)))
        }
        6 => {
            let r = hex_byte(hex, 0)?;
            let g = hex_byte(hex, 2)?;
            let b = hex_byte(hex, 4)?;
            Some((to_f32(r), to_f32(g), to_f32(b), 1.0))
        }
        8 => {
            let r = hex_byte(hex, 0)?;
            let g = hex_byte(hex, 2)?;
            let b = hex_byte(hex, 4)?;
            let a = hex_byte(hex, 6)?;
            Some((to_f32(r), to_f32(g), to_f32(b), to_f32(a)))
        }
        _ => None,
    }
}

/// Parse two consecutive hex digits at `start` as a byte (00–FF).
fn hex_byte(s: &str, start: usize) -> Option<u8> {
    let hi = hex_digit(s.as_bytes().get(start).copied()?)?;
    let lo = hex_digit(s.as_bytes().get(start + 1).copied()?)?;
    Some((hi << 4) | lo)
}

/// Expand a single hex nibble at `pos` to a full byte (e.g. `f` → `0xff`).
fn hex_nibble_pair(s: &str, pos: usize) -> Option<u8> {
    let n = hex_digit(s.as_bytes().get(pos).copied()?)?;
    Some((n << 4) | n)
}

const fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse a CSS functional color string and return normalized sRGB + alpha.
#[must_use]
pub fn functional(text: &str) -> Option<(f32, f32, f32, f32)> {
    let text = text.trim();

    let paren_open = text.find('(')?;
    let func = text[..paren_open].trim().to_ascii_lowercase();
    let rest = text[paren_open + 1..].strip_suffix(')')?.trim();

    match func.as_str() {
        "rgb" | "rgba" => {
            if rest.contains(',') {
                parse_legacy_rgb(rest)
            } else {
                parse_modern_rgb(rest)
            }
        }
        "hsl" | "hsla" => {
            if rest.contains(',') {
                parse_legacy_hsl(rest)
            } else {
                parse_modern_hsl(rest)
            }
        }
        "hwb" => parse_hwb(rest),
        "lab" => parse_lab(rest),
        "lch" => parse_lch(rest),
        "oklab" => parse_oklab(rest),
        "oklch" => parse_oklch(rest),
        _ => None,
    }
}

#[must_use]
/// Mix two colors in the requested interpolation space.
///
/// Returns normalized RGBA output in the same `[0.0, 1.0]` channel range used
/// throughout this crate, or `None` when the space or weights are invalid.
pub fn mix_colors(
    space: &str,
    left: (f32, f32, f32, f32),
    left_weight: f64,
    right: (f32, f32, f32, f32),
    right_weight: f64,
) -> Option<(f32, f32, f32, f32)> {
    if !left_weight.is_finite()
        || !right_weight.is_finite()
        || left_weight < 0.0
        || right_weight < 0.0
    {
        return None;
    }

    match space.trim().to_ascii_lowercase().as_str() {
        "srgb" => Some(mix::mix_srgb(left, left_weight, right, right_weight)),
        "oklab" => mix::mix_oklab(left, left_weight, right, right_weight),
        "oklch" => mix::mix_oklch(left, left_weight, right, right_weight),
        _ => None,
    }
}

fn parse_legacy_rgb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let raw_args = split_legacy_args(rest)?;
    if raw_args.len() != 3 && raw_args.len() != 4 {
        return None;
    }

    let r = parse_legacy_rgb_component(raw_args[0])?;
    let g = parse_legacy_rgb_component(raw_args[1])?;
    let b = parse_legacy_rgb_component(raw_args[2])?;
    let a = if raw_args.len() == 4 {
        parse_alpha(raw_args[3])?
    } else {
        1.0
    };

    Some((r, g, b, a))
}

fn parse_legacy_hsl(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let raw_args = split_legacy_args(rest)?;
    if raw_args.len() != 3 && raw_args.len() != 4 {
        return None;
    }

    let hue = parse_hue(raw_args[0])?;
    let saturation = parse_legacy_percent(raw_args[1])?;
    let lightness = parse_legacy_percent(raw_args[2])?;
    let alpha = if raw_args.len() == 4 {
        parse_alpha(raw_args[3])?
    } else {
        1.0
    };

    let (red, green, blue) = space::hsl_to_rgb(hue, saturation, lightness);
    Some((red, green, blue, alpha))
}

fn split_legacy_args(rest: &str) -> Option<Vec<&str>> {
    let raw_args: Vec<&str> = rest.split(',').map(str::trim).collect();
    if raw_args.is_empty() || raw_args.iter().any(|arg| arg.is_empty()) {
        return None;
    }
    Some(raw_args)
}

fn parse_modern_rgb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let r = parse_modern_rgb_component(components[0])?;
    let g = parse_modern_rgb_component(components[1])?;
    let b = parse_modern_rgb_component(components[2])?;
    let a = clamp_channel(parse_modern_alpha(alpha)?);
    Some((r, g, b, a))
}

fn parse_modern_hsl(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let hue = parse_hue(components[0])?;
    let saturation = parse_modern_percent_or_number_100(components[1])?;
    let lightness = parse_modern_percent_or_number_100(components[2])?;
    let alpha = clamp_channel(parse_modern_alpha(alpha)?);
    let (red, green, blue) = space::hsl_to_rgb(hue, saturation, lightness);
    Some((red, green, blue, alpha))
}

fn parse_hwb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let h = parse_hue(components[0])?;
    let w = parse_modern_percent_or_number_100(components[1])?;
    let b = parse_modern_percent_or_number_100(components[2])?;
    let a = parse_modern_alpha(alpha)?;
    Some(space::hwb_to_rgb(h, w, b, a))
}

fn parse_lab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let lightness = parse_lab_lightness(components[0])?;
    let axis_a = parse_lab_axis(components[1])?;
    let axis_b = parse_lab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    space::lab_to_srgb(lightness, axis_a, axis_b, alpha)
}

fn parse_lch(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let lightness = parse_lab_lightness(components[0])?;
    let chroma = parse_lch_chroma(components[1])?;
    let hue_angle = parse_lch_hue(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;

    let hue = if chroma <= LCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        hue_angle
    };
    let axis_a = chroma * hue.to_radians().cos();
    let axis_b = chroma * hue.to_radians().sin();
    space::lab_to_srgb(lightness, axis_a, axis_b, alpha)
}

fn parse_oklab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_oklab_lightness(components[0])?;
    let a = parse_oklab_axis(components[1])?;
    let b = parse_oklab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    space::oklab_to_srgb(l, a, b, alpha)
}

fn parse_oklch(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let lightness = parse_oklab_lightness(components[0])?;
    let chroma = parse_oklch_chroma(components[1])?;
    let hue_angle = parse_oklch_hue(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;

    let hue = if chroma <= OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        hue_angle
    };
    let axis_a = chroma * hue.to_radians().cos();
    let axis_b = chroma * hue.to_radians().sin();
    space::oklab_to_srgb(lightness, axis_a, axis_b, alpha)
}

fn split_modern_args(rest: &str) -> Option<([&str; 3], Option<&str>)> {
    if rest.contains(',') {
        return None;
    }

    let mut parts = rest.split('/');
    let main = parts.next()?.trim();
    let alpha = parts.next().map(str::trim);
    if parts.next().is_some() || main.is_empty() || main.starts_with("from ") {
        return None;
    }

    let components: Vec<&str> = main.split_whitespace().collect();
    let [c1, c2, c3]: [&str; 3] = components.try_into().ok()?;
    if alpha.is_some_and(str::is_empty) {
        return None;
    }

    Some(([c1, c2, c3], alpha))
}

fn parse_legacy_rgb_component(s: &str) -> Option<f32> {
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        if !(0.0..=100.0).contains(&v) {
            return None;
        }
        Some(v / 100.0)
    } else {
        let v: f32 = s.parse().ok()?;
        if !(0.0..=255.0).contains(&v) {
            return None;
        }
        Some(v / 255.0)
    }
}

fn parse_modern_rgb_component(s: &str) -> Option<f32> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        return Some((v / 100.0).clamp(0.0, 1.0));
    }

    let v: f32 = s.parse().ok()?;
    Some((v / 255.0).clamp(0.0, 1.0))
}

fn parse_legacy_percent(s: &str) -> Option<f32> {
    let inner = s.strip_suffix('%')?;
    let v: f32 = inner.trim().parse().ok()?;
    Some(v / 100.0)
}

fn parse_modern_percent_or_number_100(s: &str) -> Option<f32> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    let value = if let Some(percent) = s.strip_suffix('%') {
        percent.trim().parse::<f32>().ok()?
    } else {
        s.trim().parse::<f32>().ok()?
    };
    Some((value / 100.0).clamp(0.0, 1.0))
}

/// Parse a CSS hue angle, defaulting bare numbers to degrees.
fn parse_hue(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }

    if let Some(v) = strip_suffix_ci(s, "deg") {
        return v.trim().parse::<f32>().ok();
    }
    if let Some(v) = strip_suffix_ci(s, "grad") {
        return Some(v.trim().parse::<f32>().ok()?.mul_add(0.9, 0.0));
    }
    if let Some(v) = strip_suffix_ci(s, "rad") {
        return Some(v.trim().parse::<f32>().ok()? * (180.0 / std::f32::consts::PI));
    }
    if let Some(v) = strip_suffix_ci(s, "turn") {
        return Some(v.trim().parse::<f32>().ok()?.mul_add(360.0, 0.0));
    }

    s.parse::<f32>().ok()
}

fn parse_hue_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }

    let lower = s.to_ascii_lowercase();
    let hue = if let Some(value) = lower.strip_suffix("deg") {
        value.trim().parse::<f64>().ok()?
    } else if let Some(value) = lower.strip_suffix("grad") {
        value.trim().parse::<f64>().ok()? * 0.9
    } else if let Some(value) = lower.strip_suffix("rad") {
        value.trim().parse::<f64>().ok()?.to_degrees()
    } else if let Some(value) = lower.strip_suffix("turn") {
        value.trim().parse::<f64>().ok()? * 360.0
    } else {
        lower.parse::<f64>().ok()?
    };

    Some(hue.rem_euclid(360.0))
}

fn strip_suffix_ci<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let prefix_len = text.len().checked_sub(suffix.len())?;
    text[prefix_len..]
        .eq_ignore_ascii_case(suffix)
        .then_some(&text[..prefix_len])
}

/// Parse an alpha value in 0.0–1.0 or 0%–100%.
fn parse_alpha(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(1.0);
    }
    if let Some(percent) = s.strip_suffix('%') {
        let v: f32 = percent.trim().parse().ok()?;
        if !(0.0..=100.0).contains(&v) {
            return None;
        }
        return Some(v / 100.0);
    }

    let v: f32 = s.parse().ok()?;
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some(v)
}

fn parse_modern_alpha(alpha: Option<&str>) -> Option<f64> {
    match alpha {
        Some(value) => {
            let value = value.trim();
            if value.eq_ignore_ascii_case("none") {
                return Some(1.0);
            }
            if let Some(percent) = value.strip_suffix('%') {
                let v = percent.trim().parse::<f64>().ok()?;
                return Some((v / 100.0).clamp(0.0, 1.0));
            }

            let v = value.parse::<f64>().ok()?;
            Some(v.clamp(0.0, 1.0))
        }
        None => Some(1.0),
    }
}

fn parse_lab_lightness(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    let value = if let Some(percent) = s.strip_suffix('%') {
        percent.trim().parse::<f64>().ok()?
    } else {
        s.trim().parse::<f64>().ok()?
    };
    Some(value.clamp(0.0, 100.0))
}

fn parse_lab_axis(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(percent) = s.strip_suffix('%') {
        return Some(percent.trim().parse::<f64>().ok()? * 125.0 / 100.0);
    }
    s.trim().parse::<f64>().ok()
}

fn parse_lch_chroma(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    let value = if let Some(percent) = s.strip_suffix('%') {
        percent.trim().parse::<f64>().ok()? * 150.0 / 100.0
    } else {
        s.trim().parse::<f64>().ok()?
    };
    Some(value.max(0.0))
}

fn parse_lch_hue(s: &str) -> Option<f64> {
    parse_hue_f64(s)
}

fn parse_oklab_lightness(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    let value = if let Some(percent) = s.strip_suffix('%') {
        percent.trim().parse::<f64>().ok()? / 100.0
    } else {
        s.trim().parse::<f64>().ok()?
    };
    Some(value.clamp(0.0, 1.0))
}

fn parse_oklab_axis(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(percent) = s.strip_suffix('%') {
        return Some(percent.trim().parse::<f64>().ok()? * 0.4 / 100.0);
    }
    s.trim().parse::<f64>().ok()
}

fn parse_oklch_chroma(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    let value = if let Some(percent) = s.strip_suffix('%') {
        percent.trim().parse::<f64>().ok()? * 0.4 / 100.0
    } else {
        s.trim().parse::<f64>().ok()?
    };
    Some(value.max(0.0))
}

fn parse_oklch_hue(s: &str) -> Option<f64> {
    parse_hue_f64(s)
}

pub(crate) fn clamp_channel(value: f64) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value.abs() < 1e-12 {
        0.0
    } else {
        f64_to_f32(value)
    }
}

fn f64_to_f32(value: f64) -> f32 {
    value.to_string().parse::<f32>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::{functional as parse_functional, hex as parse_hex, *};

    fn assert_rgb_close(
        actual: (f32, f32, f32, f32),
        expected: (f32, f32, f32, f32),
        epsilon: f32,
    ) {
        assert!(
            (actual.0 - expected.0).abs() <= epsilon,
            "r: {actual:?} vs {expected:?}"
        );
        assert!(
            (actual.1 - expected.1).abs() <= epsilon,
            "g: {actual:?} vs {expected:?}"
        );
        assert!(
            (actual.2 - expected.2).abs() <= epsilon,
            "b: {actual:?} vs {expected:?}"
        );
        assert!(
            (actual.3 - expected.3).abs() <= epsilon,
            "a: {actual:?} vs {expected:?}"
        );
    }

    #[test]
    fn hex_6_digit() {
        assert_eq!(parse_hex("#ff0000"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#00ff00"), Some((0.0, 1.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#0000ff"), Some((0.0, 0.0, 1.0, 1.0)));
    }

    #[test]
    fn hex_3_digit() {
        assert_eq!(parse_hex("#f00"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#fff"), Some((1.0, 1.0, 1.0, 1.0)));
    }

    #[test]
    fn hex_8_digit_alpha() {
        assert_eq!(parse_hex("#ff000080"), Some((1.0, 0.0, 0.0, 128.0 / 255.0)));
        assert_eq!(parse_hex("#ff0000ff"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hex_4_digit_alpha() {
        assert_eq!(
            parse_hex("#f008"),
            Some((1.0, 0.0, 0.0, 0x88 as f32 / 255.0))
        );
    }

    #[test]
    fn hex_case_insensitive() {
        assert_eq!(parse_hex("#FF0000"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#Ff0000"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hex_invalid() {
        assert_eq!(parse_hex("#gg0000"), None);
        assert_eq!(parse_hex("#ff"), None);
        assert_eq!(parse_hex("ff0000"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn rgb_integers() {
        assert_eq!(
            parse_functional("rgb(255, 0, 0)"),
            Some((1.0, 0.0, 0.0, 1.0))
        );
        assert_eq!(
            parse_functional("rgb(0,128,255)"),
            Some((0.0, 128.0 / 255.0, 1.0, 1.0))
        );
    }

    #[test]
    fn modern_rgb_space_separated() {
        assert_eq!(
            parse_functional("rgb(255 0 0 / 50%)"),
            Some((1.0, 0.0, 0.0, 0.5))
        );
        assert_eq!(
            parse_functional("rgba(100% 0% 0% / none)"),
            Some((1.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn modern_rgb_none_and_clamping() {
        assert_eq!(
            parse_functional("rgb(none 300 -10 / 2)"),
            Some((0.0, 1.0, 0.0, 1.0))
        );
    }

    #[test]
    fn rgba_with_alpha() {
        let result = parse_functional("rgba(255, 0, 0, 50%)").unwrap();
        assert!((result.0 - 1.0).abs() < 0.01);
        assert!((result.3 - 0.5).abs() < 0.01);
    }

    #[test]
    fn rgb_percentages() {
        assert_eq!(
            parse_functional("rgb(100%, 0%, 0%)"),
            Some((1.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn hsl_basic() {
        let result = parse_functional("hsl(0, 100%, 50%)").unwrap();
        assert!((result.0 - 1.0).abs() < 0.02);
        assert!(result.1 < 0.02);
        assert!(result.2 < 0.02);
    }

    #[test]
    fn modern_hsl_space_separated() {
        let result = parse_functional("hsl(120deg 100% 50% / 25%)").unwrap();
        assert!(result.0 < 0.02);
        assert!((result.1 - 1.0).abs() < 0.02);
        assert!(result.2 < 0.02);
        assert!((result.3 - 0.25).abs() < 0.01);
    }

    #[test]
    fn hsl_green() {
        let result = parse_functional("hsl(120, 100%, 50%)").unwrap();
        assert!(result.0 < 0.02);
        assert!((result.1 - 1.0).abs() < 0.02);
        assert!(result.2 < 0.02);
    }

    #[test]
    fn hsla_with_alpha() {
        let result = parse_functional("hsla(0, 100%, 50%, 0.5)").unwrap();
        assert!((result.3 - 0.5).abs() < 0.01);
    }

    #[test]
    fn hwb_green() {
        let result = parse_functional("hwb(120 0% 0%)").unwrap();
        assert_rgb_close(result, (0.0, 1.0, 0.0, 1.0), 0.02);
    }

    #[test]
    fn hwb_achromatic() {
        let result = parse_functional("hwb(45 40% 80%)").unwrap();
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.0 - (40.0 / 120.0)).abs() < 0.02);
    }

    #[test]
    fn lab_and_lch_equivalent() {
        let lab = parse_functional("lab(29.2345% 39.3825 20.0664)").unwrap();
        let lch = parse_functional("lch(29.2345% 44.2 27)").unwrap();
        assert_rgb_close(lab, lch, 0.02);
    }

    #[test]
    fn lab_none_components_map_to_gray() {
        let result = parse_functional("lab(50% none none / 50%)").unwrap();
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.3 - 0.5).abs() < 0.01);
    }

    #[test]
    fn lch_achromatic_none_hue_is_gray() {
        let result = parse_functional("lch(50% 0 none)").unwrap();
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
    }

    #[test]
    fn oklch_red_round_trip() {
        let result = parse_functional("oklch(0.627966 0.257704 29.2346)").unwrap();
        assert_rgb_close(result, (1.0, 0.0, 0.0, 1.0), 0.02);
    }

    #[test]
    fn oklab_red_round_trip() {
        let result = parse_functional("oklab(62.7966% 0.22488 0.125859)").unwrap();
        assert_rgb_close(result, (1.0, 0.0, 0.0, 1.0), 0.02);
    }

    #[test]
    fn oklab_percentage_axes_round_trip() {
        let result = parse_functional("oklab(62.7966% 56.22% 31.46475% / 50%)").unwrap();
        assert_rgb_close(result, (1.0, 0.0, 0.0, 0.5), 0.02);
    }

    #[test]
    fn oklch_achromatic_none_hue_is_gray() {
        let result = parse_functional("oklch(59.99% 0 none)").unwrap();
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.0 - (128.0 / 255.0)).abs() < 0.02);
    }

    #[test]
    fn oklab_none_components_map_to_zero() {
        assert_eq!(
            parse_functional("oklab(none none none / none)"),
            Some((0.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn oklch_hue_units_are_supported() {
        let deg = parse_functional("oklch(0.627966 0.257704 29.2346)").unwrap();
        let turn = parse_functional("oklch(0.627966 0.257704 0.0812072222turn)").unwrap();
        let rad = parse_functional("oklch(0.627966 0.257704 0.510239rad)").unwrap();
        assert_rgb_close(deg, turn, 0.01);
        assert_rgb_close(deg, rad, 0.02);
    }

    #[test]
    fn mix_colors_oklch_lightens_toward_white() {
        let base = parse_functional("oklch(22.84% 0.038 283)").unwrap();
        let mixed = mix_colors("oklch", base, 0.92, (1.0, 1.0, 1.0, 1.0), 0.08).unwrap();
        assert!(mixed.0 > base.0);
        assert!(mixed.1 > base.1);
        assert!(mixed.2 > base.2);
    }

    #[test]
    fn mix_colors_oklch_with_transparent_reduces_alpha() {
        let base = parse_functional("oklch(22.84% 0.038 283)").unwrap();
        let mixed = mix_colors("oklch", base, 0.96, (0.0, 0.0, 0.0, 0.0), 0.04).unwrap();
        assert_rgb_close(mixed, (base.0, base.1, base.2, 0.96), 0.04);
    }

    #[test]
    fn functional_whitespace_variations() {
        assert!(parse_functional("rgb( 255 , 0 , 0 )").is_some());
        assert!(parse_functional("rgb(255,0,0)").is_some());
        assert!(parse_functional("rgb(255 0 0 / 0.5)").is_some());
        assert!(parse_functional("hsl(120deg 100% 50%)").is_some());
        assert!(parse_functional("OKLCH(62.7966% 64.426% 29.2346)").is_some());
        assert!(parse_functional("lab(29.2345% 39.3825 20.0664)").is_some());
        assert!(parse_functional("hwb(120 0% 0%)").is_some());
    }

    #[test]
    fn functional_invalid() {
        assert_eq!(parse_functional("rgb()"), None);
        assert_eq!(parse_functional("rgb(a, b, c)"), None);
        assert_eq!(parse_functional("notafunction(1,2,3)"), None);
        assert_eq!(parse_functional("oklch(0.6, 0.2, 20)"), None);
        assert_eq!(parse_functional("lab(10, 20, 30)"), None);
        assert_eq!(parse_functional("hwb(0, 10%, 10%)"), None);
        assert_eq!(parse_functional("oklab(from red l a b)"), None);
        assert_eq!(parse_functional(""), None);
    }
}
