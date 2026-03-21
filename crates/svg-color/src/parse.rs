const OKLCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 4e-6;

/// Parse a hex color string like `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
pub fn parse_hex(text: &str) -> Option<(f32, f32, f32, f32)> {
    let hex = text.strip_prefix('#')?;

    let to_f32 = |b: u8| b as f32 / 255.0;

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

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse a CSS functional color string and return normalized sRGB + alpha.
///
/// Supported forms:
/// - `rgb()` / `rgba()` with comma-separated legacy syntax
/// - `hsl()` / `hsla()` with comma-separated legacy syntax
/// - `oklab()` with modern space-separated syntax
/// - `oklch()` with modern space-separated syntax
pub fn parse_functional(text: &str) -> Option<(f32, f32, f32, f32)> {
    let text = text.trim();

    let paren_open = text.find('(')?;
    let func = text[..paren_open].trim().to_ascii_lowercase();
    let rest = text[paren_open + 1..].strip_suffix(')')?.trim();

    match func.as_str() {
        "rgb" | "rgba" => parse_legacy_rgb(rest),
        "hsl" | "hsla" => parse_legacy_hsl(rest),
        "oklab" => parse_oklab(rest),
        "oklch" => parse_oklch(rest),
        _ => None,
    }
}

fn parse_legacy_rgb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let raw_args = split_legacy_args(rest)?;
    if raw_args.len() != 3 && raw_args.len() != 4 {
        return None;
    }

    let r = parse_rgb_component(raw_args[0])?;
    let g = parse_rgb_component(raw_args[1])?;
    let b = parse_rgb_component(raw_args[2])?;
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

    let h = parse_hue(raw_args[0])?;
    let s = parse_percent(raw_args[1])?;
    let l = parse_percent(raw_args[2])?;
    let a = if raw_args.len() == 4 {
        parse_alpha(raw_args[3])?
    } else {
        1.0
    };

    let (r, g, b) = hsl_to_rgb(h, s, l);
    Some((r, g, b, a))
}

fn split_legacy_args(rest: &str) -> Option<Vec<&str>> {
    let raw_args: Vec<&str> = rest.split(',').map(str::trim).collect();
    if raw_args.is_empty() || raw_args.iter().any(|arg| arg.is_empty()) {
        return None;
    }
    Some(raw_args)
}

fn parse_oklab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_oklab_lightness(components[0])?;
    let a = parse_oklab_axis(components[1])?;
    let b = parse_oklab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    oklab_to_srgb(l, a, b, alpha)
}

fn parse_oklch(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_oklab_lightness(components[0])?;
    let c = parse_oklch_chroma(components[1])?;
    let h = parse_oklch_hue(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;

    let hue = if c <= OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        h
    };
    let a = c * hue.to_radians().cos();
    let b = c * hue.to_radians().sin();
    oklab_to_srgb(l, a, b, alpha)
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

/// Parse a single RGB component: integer 0–255 or percentage 0%–100%.
/// Returns a normalised f32 in 0.0–1.0.
fn parse_rgb_component(s: &str) -> Option<f32> {
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

/// Parse a percentage value like `50%`, returning the value divided by 100.
fn parse_percent(s: &str) -> Option<f32> {
    let inner = s.strip_suffix('%')?;
    let v: f32 = inner.trim().parse().ok()?;
    Some(v / 100.0)
}

/// Parse a CSS hue angle, defaulting bare numbers to degrees.
fn parse_hue(s: &str) -> Option<f32> {
    parse_hue_f64(s).map(|h| h as f32)
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
        Some(value) => Some(f64::from(parse_alpha(value)?)),
        None => Some(1.0),
    }
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

fn oklab_to_srgb(l: f64, a: f64, b: f64, alpha: f64) -> Option<(f32, f32, f32, f32)> {
    let lms_nl = [
        l + 0.396_337_777_376_174_9 * a + 0.215_803_757_309_913_6 * b,
        l - 0.105_561_345_815_658_6 * a - 0.063_854_172_825_813_3 * b,
        l - 0.089_484_177_529_811_9 * a - 1.291_485_548_019_409_2 * b,
    ];

    let lms = [cube(lms_nl[0]), cube(lms_nl[1]), cube(lms_nl[2])];
    let xyz = ok_lab_lms_to_xyz(lms);
    let linear_rgb = xyz_to_linear_srgb(xyz);
    let srgb = linear_rgb.map(linear_to_srgb);

    if srgb.iter().any(|value| !value.is_finite()) || !alpha.is_finite() {
        return None;
    }

    Some((
        clamp_channel(srgb[0]),
        clamp_channel(srgb[1]),
        clamp_channel(srgb[2]),
        clamp_channel(alpha),
    ))
}

fn ok_lab_lms_to_xyz([l, m, s]: [f64; 3]) -> [f64; 3] {
    [
        1.226_879_875_845_924_3 * l - 0.557_814_994_460_217_1 * m + 0.281_391_045_665_964_7 * s,
        -0.040_575_745_214_800_8 * l + 1.112_286_803_280_317 * m - 0.071_711_058_065_516_4 * s,
        -0.076_372_936_674_660_1 * l - 0.421_493_332_402_243_2 * m + 1.586_924_019_836_781_6 * s,
    ]
}

fn xyz_to_linear_srgb([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        (12_831.0 / 3_959.0) * x + (-329.0 / 214.0) * y + (-1_974.0 / 3_959.0) * z,
        (-851_781.0 / 878_810.0) * x + (1_648_619.0 / 878_810.0) * y + (36_519.0 / 878_810.0) * z,
        (705.0 / 12_673.0) * x + (-2_585.0 / 12_673.0) * y + (705.0 / 667.0) * z,
    ]
}

fn linear_to_srgb(value: f64) -> f64 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let abs = value.abs();
    if abs > 0.003_130_8 {
        sign * (1.055 * abs.powf(1.0 / 2.4) - 0.055)
    } else {
        12.92 * value
    }
}

fn cube(value: f64) -> f64 {
    value * value * value
}

fn clamp_channel(value: f64) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value.abs() < 1e-12 {
        0.0
    } else {
        value as f32
    }
}

/// Convert HSL to RGB.  All inputs/outputs are normalised to 0.0–1.0 except
/// `h`, which is in degrees (0–360, wrapping).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());

    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ─── hex ────────────────────────────────────────────

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

    // ─── functional ─────────────────────────────────────

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
    fn rgba_with_alpha() {
        let result = parse_functional("rgba(255, 0, 0, 50%)");
        assert!(result.is_some());
        let (r, _, _, a) = result.unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!((a - 0.5).abs() < 0.01);
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
        let result = parse_functional("hsl(0, 100%, 50%)");
        assert!(result.is_some());
        let (r, g, b, _) = result.unwrap();
        assert!((r - 1.0).abs() < 0.02);
        assert!(g < 0.02);
        assert!(b < 0.02);
    }

    #[test]
    fn hsl_green() {
        let result = parse_functional("hsl(120, 100%, 50%)");
        assert!(result.is_some());
        let (r, g, b, _) = result.unwrap();
        assert!(r < 0.02);
        assert!((g - 1.0).abs() < 0.02);
        assert!(b < 0.02);
    }

    #[test]
    fn hsla_with_alpha() {
        let result = parse_functional("hsla(0, 100%, 50%, 0.5)");
        assert!(result.is_some());
        let (_, _, _, a) = result.unwrap();
        assert!((a - 0.5).abs() < 0.01);
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
    fn functional_whitespace_variations() {
        assert!(parse_functional("rgb( 255 , 0 , 0 )").is_some());
        assert!(parse_functional("rgb(255,0,0)").is_some());
        assert!(parse_functional("OKLCH(62.7966% 64.426% 29.2346)").is_some());
    }

    #[test]
    fn functional_invalid() {
        assert_eq!(parse_functional("rgb()"), None);
        assert_eq!(parse_functional("rgb(a, b, c)"), None);
        assert_eq!(parse_functional("notafunction(1,2,3)"), None);
        assert_eq!(parse_functional("oklch(0.6, 0.2, 20)"), None);
        assert_eq!(parse_functional("oklab(from red l a b)"), None);
        assert_eq!(parse_functional(""), None);
    }
}
