mod hex;
mod lab_lch;
mod mix;
mod oklab_oklch;
mod rgb_hsl;
mod space;

pub use hex::hex;

const LCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 0.0015;
const OKLCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 4e-6;

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
                rgb_hsl::parse_legacy_rgb(rest)
            } else {
                rgb_hsl::parse_modern_rgb(rest)
            }
        }
        "hsl" | "hsla" => {
            if rest.contains(',') {
                rgb_hsl::parse_legacy_hsl(rest)
            } else {
                rgb_hsl::parse_modern_hsl(rest)
            }
        }
        "hwb" => rgb_hsl::parse_hwb(rest),
        "lab" => lab_lch::parse_lab(rest),
        "lch" => lab_lch::parse_lch(rest),
        "oklab" => oklab_oklch::parse_oklab(rest),
        "oklch" => oklab_oklch::parse_oklch(rest),
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

// ── Shared parsing helpers used by submodules ───────────────────

fn split_legacy_args(rest: &str) -> Option<Vec<&str>> {
    let raw_args: Vec<&str> = rest.split(',').map(str::trim).collect();
    if raw_args.is_empty() || raw_args.iter().any(|arg| arg.is_empty()) {
        return None;
    }
    Some(raw_args)
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
        return Some(v.trim().parse::<f32>().ok()? * 0.9);
    }
    if let Some(v) = strip_suffix_ci(s, "rad") {
        return Some(v.trim().parse::<f32>().ok()? * (180.0 / std::f32::consts::PI));
    }
    if let Some(v) = strip_suffix_ci(s, "turn") {
        return Some(v.trim().parse::<f32>().ok()? * 360.0);
    }

    s.parse::<f32>().ok()
}

fn parse_hue_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }

    let hue = if let Some(v) = strip_suffix_ci(s, "deg") {
        v.trim().parse::<f64>().ok()?
    } else if let Some(v) = strip_suffix_ci(s, "grad") {
        v.trim().parse::<f64>().ok()? * 0.9
    } else if let Some(v) = strip_suffix_ci(s, "rad") {
        v.trim().parse::<f64>().ok()?.to_degrees()
    } else if let Some(v) = strip_suffix_ci(s, "turn") {
        v.trim().parse::<f64>().ok()? * 360.0
    } else {
        s.parse::<f64>().ok()?
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

pub fn clamp_channel(value: f64) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value.abs() < 1e-12 {
        0.0
    } else {
        f64_to_f32(value)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "color channel helpers intentionally narrow clamped f64 values to f32"
)]
const fn f64_to_f32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::{functional as parse_functional, hex::hex as parse_hex, mix_colors};

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
            Some((1.0, 0.0, 0.0, f32::from(0x88_u8) / 255.0))
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
    fn rgba_with_alpha() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("rgba(255, 0, 0, 50%)").ok_or("parse failed")?;
        assert!((result.0 - 1.0).abs() < 0.01);
        assert!((result.3 - 0.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn rgb_percentages() {
        assert_eq!(
            parse_functional("rgb(100%, 0%, 0%)"),
            Some((1.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn hsl_basic() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hsl(0, 100%, 50%)").ok_or("parse failed")?;
        assert!((result.0 - 1.0).abs() < 0.02);
        assert!(result.1 < 0.02);
        assert!(result.2 < 0.02);
        Ok(())
    }

    #[test]
    fn modern_hsl_space_separated() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hsl(120deg 100% 50% / 25%)").ok_or("parse failed")?;
        assert!(result.0 < 0.02);
        assert!((result.1 - 1.0).abs() < 0.02);
        assert!(result.2 < 0.02);
        assert!((result.3 - 0.25).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn hsl_green() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hsl(120, 100%, 50%)").ok_or("parse failed")?;
        assert!(result.0 < 0.02);
        assert!((result.1 - 1.0).abs() < 0.02);
        assert!(result.2 < 0.02);
        Ok(())
    }

    #[test]
    fn hsla_with_alpha() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hsla(0, 100%, 50%, 0.5)").ok_or("parse failed")?;
        assert!((result.3 - 0.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn hwb_green() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hwb(120 0% 0%)").ok_or("parse failed")?;
        assert_rgb_close(result, (0.0, 1.0, 0.0, 1.0), 0.02);
        Ok(())
    }

    #[test]
    fn hwb_achromatic() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("hwb(45 40% 80%)").ok_or("parse failed")?;
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.0 - (40.0 / 120.0)).abs() < 0.02);
        Ok(())
    }

    #[test]
    fn lab_and_lch_equivalent() -> Result<(), Box<dyn std::error::Error>> {
        let lab = parse_functional("lab(29.2345% 39.3825 20.0664)").ok_or("parse failed")?;
        let lch = parse_functional("lch(29.2345% 44.2 27)").ok_or("parse failed")?;
        assert_rgb_close(lab, lch, 0.02);
        Ok(())
    }

    #[test]
    fn lab_none_components_map_to_gray() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("lab(50% none none / 50%)").ok_or("parse failed")?;
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.3 - 0.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn lch_achromatic_none_hue_is_gray() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("lch(50% 0 none)").ok_or("parse failed")?;
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn oklch_red_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("oklch(0.627966 0.257704 29.2346)").ok_or("parse failed")?;
        assert_rgb_close(result, (1.0, 0.0, 0.0, 1.0), 0.02);
        Ok(())
    }

    #[test]
    fn oklab_red_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("oklab(62.7966% 0.22488 0.125859)").ok_or("parse failed")?;
        assert_rgb_close(result, (1.0, 0.0, 0.0, 1.0), 0.02);
        Ok(())
    }

    #[test]
    fn oklab_percentage_axes_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let result =
            parse_functional("oklab(62.7966% 56.22% 31.46475% / 50%)").ok_or("parse failed")?;
        assert_rgb_close(result, (1.0, 0.0, 0.0, 0.5), 0.02);
        Ok(())
    }

    #[test]
    fn oklch_achromatic_none_hue_is_gray() -> Result<(), Box<dyn std::error::Error>> {
        let result = parse_functional("oklch(59.99% 0 none)").ok_or("parse failed")?;
        assert!((result.0 - result.1).abs() < 0.001);
        assert!((result.1 - result.2).abs() < 0.001);
        assert!((result.0 - (128.0 / 255.0)).abs() < 0.02);
        Ok(())
    }

    #[test]
    fn oklab_none_components_map_to_zero() {
        assert_eq!(
            parse_functional("oklab(none none none / none)"),
            Some((0.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn oklch_hue_units_are_supported() -> Result<(), Box<dyn std::error::Error>> {
        let deg = parse_functional("oklch(0.627966 0.257704 29.2346)").ok_or("parse failed")?;
        let turn =
            parse_functional("oklch(0.627966 0.257704 0.0812072222turn)").ok_or("parse failed")?;
        let rad = parse_functional("oklch(0.627966 0.257704 0.510239rad)").ok_or("parse failed")?;
        assert_rgb_close(deg, turn, 0.01);
        assert_rgb_close(deg, rad, 0.02);
        Ok(())
    }

    #[test]
    fn mix_colors_oklch_lightens_toward_white() -> Result<(), Box<dyn std::error::Error>> {
        let base = parse_functional("oklch(22.84% 0.038 283)").ok_or("parse failed")?;
        let mixed =
            mix_colors("oklch", base, 0.92, (1.0, 1.0, 1.0, 1.0), 0.08).ok_or("mix failed")?;
        assert!(mixed.0 > base.0);
        assert!(mixed.1 > base.1);
        assert!(mixed.2 > base.2);
        Ok(())
    }

    #[test]
    fn mix_colors_oklch_with_transparent_reduces_alpha() -> Result<(), Box<dyn std::error::Error>> {
        let base = parse_functional("oklch(22.84% 0.038 283)").ok_or("parse failed")?;
        let mixed =
            mix_colors("oklch", base, 0.96, (0.0, 0.0, 0.0, 0.0), 0.04).ok_or("mix failed")?;
        assert_rgb_close(mixed, (base.0, base.1, base.2, 0.96), 0.04);
        Ok(())
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
