const LCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 0.0015;
const OKLCH_ACHROMATIC_CHROMA_THRESHOLD: f64 = 4e-6;
const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

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
pub fn parse_functional(text: &str) -> Option<(f32, f32, f32, f32)> {
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

pub fn mix_colors(
    space: &str,
    left: (f32, f32, f32, f32),
    left_weight: f32,
    right: (f32, f32, f32, f32),
    right_weight: f32,
) -> Option<(f32, f32, f32, f32)> {
    let left_weight = left_weight as f64;
    let right_weight = right_weight as f64;
    if !left_weight.is_finite()
        || !right_weight.is_finite()
        || left_weight < 0.0
        || right_weight < 0.0
    {
        return None;
    }

    match space.trim().to_ascii_lowercase().as_str() {
        "srgb" => mix_srgb(left, left_weight, right, right_weight),
        "oklab" => mix_oklab(left, left_weight, right, right_weight),
        "oklch" => mix_oklch(left, left_weight, right, right_weight),
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

    let h = parse_hue(raw_args[0])?;
    let s = parse_legacy_percent(raw_args[1])?;
    let l = parse_legacy_percent(raw_args[2])?;
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
    let h = parse_hue(components[0])?;
    let s = parse_modern_percent_or_number_100(components[1])?;
    let l = parse_modern_percent_or_number_100(components[2])?;
    let a = clamp_channel(parse_modern_alpha(alpha)?);
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Some((r, g, b, a))
}

fn parse_hwb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let h = parse_hue(components[0])?;
    let w = parse_modern_percent_or_number_100(components[1])?;
    let b = parse_modern_percent_or_number_100(components[2])?;
    let a = parse_modern_alpha(alpha)?;
    Some(hwb_to_rgb(h, w, b, a))
}

fn parse_lab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_lab_lightness(components[0])?;
    let a = parse_lab_axis(components[1])?;
    let b = parse_lab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    lab_to_srgb(l, a, b, alpha)
}

fn parse_lch(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_lab_lightness(components[0])?;
    let c = parse_lch_chroma(components[1])?;
    let h = parse_lch_hue(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;

    let hue = if c <= LCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        h
    };
    let a = c * hue.to_radians().cos();
    let b = c * hue.to_radians().sin();
    lab_to_srgb(l, a, b, alpha)
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

fn hwb_to_rgb(h: f32, w: f32, b: f32, alpha: f64) -> (f32, f32, f32, f32) {
    let w = w.clamp(0.0, 1.0);
    let b = b.clamp(0.0, 1.0);

    if w + b >= 1.0 {
        let gray = if w + b == 0.0 { 0.0 } else { w / (w + b) };
        return (gray, gray, gray, clamp_channel(alpha));
    }

    let (hr, hg, hb) = hsl_to_rgb(h, 1.0, 0.5);
    let scale = 1.0 - w - b;
    (
        hr * scale + w,
        hg * scale + w,
        hb * scale + w,
        clamp_channel(alpha),
    )
}

fn lab_to_srgb(l: f64, a: f64, b: f64, alpha: f64) -> Option<(f32, f32, f32, f32)> {
    let xyz_d50 = lab_to_xyz([l, a, b]);
    let xyz_d65 = d50_to_d65(xyz_d50);
    let linear_rgb = xyz_to_linear_srgb(xyz_d65);
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

fn srgb_to_oklab(r: f32, g: f32, b: f32) -> Option<[f64; 3]> {
    let linear = [
        srgb_to_linear(r as f64),
        srgb_to_linear(g as f64),
        srgb_to_linear(b as f64),
    ];
    let xyz = linear_srgb_to_xyz(linear);
    let lms = [
        0.818_933_010_1 * xyz[0] + 0.361_866_742_4 * xyz[1] - 0.128_859_713_7 * xyz[2],
        0.032_984_543_6 * xyz[0] + 0.929_311_871_5 * xyz[1] + 0.036_145_638_7 * xyz[2],
        0.048_200_301_8 * xyz[0] + 0.264_366_269_1 * xyz[1] + 0.633_851_707 * xyz[2],
    ]
    .map(|value| value.cbrt());

    let oklab = [
        0.210_454_255_3 * lms[0] + 0.793_617_785 * lms[1] - 0.004_072_046_8 * lms[2],
        1.977_998_495_1 * lms[0] - 2.428_592_205 * lms[1] + 0.450_593_709_9 * lms[2],
        0.025_904_037_1 * lms[0] + 0.782_771_766_2 * lms[1] - 0.808_675_766 * lms[2],
    ];

    if oklab.iter().all(|value| value.is_finite()) {
        Some(oklab)
    } else {
        None
    }
}

fn srgb_to_oklch(r: f32, g: f32, b: f32) -> Option<[f64; 3]> {
    let [l, a, b] = srgb_to_oklab(r, g, b)?;
    let c = (a * a + b * b).sqrt();
    let h = if c <= OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        b.atan2(a).to_degrees().rem_euclid(360.0)
    };
    Some([l, c, h])
}

fn lab_to_xyz([l, a, b]: [f64; 3]) -> [f64; 3] {
    let kappa = 24_389.0 / 27.0;
    let epsilon = 216.0 / 24_389.0;

    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;

    let xyz = [
        if fx.powi(3) > epsilon {
            fx.powi(3)
        } else {
            (116.0 * fx - 16.0) / kappa
        },
        if l > kappa * epsilon {
            ((l + 16.0) / 116.0).powi(3)
        } else {
            l / kappa
        },
        if fz.powi(3) > epsilon {
            fz.powi(3)
        } else {
            (116.0 * fz - 16.0) / kappa
        },
    ];

    [xyz[0] * D50[0], xyz[1] * D50[1], xyz[2] * D50[2]]
}

fn d50_to_d65([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        0.955_473_421_488_075 * x - 0.023_098_454_948_764_71 * y + 0.063_259_243_200_570_72 * z,
        -0.028_369_709_333_863_7 * x + 1.009_995_398_081_304_1 * y + 0.021_041_441_191_917_323 * z,
        0.012_314_014_864_481_998 * x - 0.020_507_649_298_898_964 * y + 1.330_365_926_242_124 * z,
    ]
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

fn linear_srgb_to_xyz([r, g, b]: [f64; 3]) -> [f64; 3] {
    [
        0.412_390_799_265_959_34 * r + 0.357_584_339_383_877_96 * g + 0.180_480_788_401_834_3 * b,
        0.212_639_005_871_510_27 * r + 0.715_168_678_767_755_9 * g + 0.072_192_315_360_733_71 * b,
        0.019_330_818_715_591_85 * r + 0.119_194_779_794_625_99 * g + 0.950_532_152_249_660_7 * b,
    ]
}

fn xyz_to_linear_srgb([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        (12_831.0 / 3_959.0) * x + (-329.0 / 214.0) * y + (-1_974.0 / 3_959.0) * z,
        (-851_781.0 / 878_810.0) * x + (1_648_619.0 / 878_810.0) * y + (36_519.0 / 878_810.0) * z,
        (705.0 / 12_673.0) * x + (-2_585.0 / 12_673.0) * y + (705.0 / 667.0) * z,
    ]
}

fn srgb_to_linear(value: f64) -> f64 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let abs = value.abs();
    if abs <= 0.040_45 {
        value / 12.92
    } else {
        sign * ((abs + 0.055) / 1.055).powf(2.4)
    }
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

fn mix_srgb(
    left: (f32, f32, f32, f32),
    left_weight: f64,
    right: (f32, f32, f32, f32),
    right_weight: f64,
) -> Option<(f32, f32, f32, f32)> {
    let left = [left.0 as f64, left.1 as f64, left.2 as f64, left.3 as f64];
    let right = [
        right.0 as f64,
        right.1 as f64,
        right.2 as f64,
        right.3 as f64,
    ];
    let (coords, alpha) = mix_premultiplied(
        [left[0], left[1], left[2]],
        left[3],
        [right[0], right[1], right[2]],
        right[3],
        left_weight,
        right_weight,
    );
    Some((
        clamp_channel(coords[0]),
        clamp_channel(coords[1]),
        clamp_channel(coords[2]),
        clamp_channel(alpha),
    ))
}

fn mix_oklab(
    left: (f32, f32, f32, f32),
    left_weight: f64,
    right: (f32, f32, f32, f32),
    right_weight: f64,
) -> Option<(f32, f32, f32, f32)> {
    let left_lab = srgb_to_oklab(left.0, left.1, left.2)?;
    let right_lab = srgb_to_oklab(right.0, right.1, right.2)?;
    let (coords, alpha) = mix_premultiplied(
        left_lab,
        left.3 as f64,
        right_lab,
        right.3 as f64,
        left_weight,
        right_weight,
    );
    oklab_to_srgb(coords[0], coords[1], coords[2], alpha)
}

fn mix_oklch(
    left: (f32, f32, f32, f32),
    left_weight: f64,
    right: (f32, f32, f32, f32),
    right_weight: f64,
) -> Option<(f32, f32, f32, f32)> {
    let mut left_lch = srgb_to_oklch(left.0, left.1, left.2)?;
    let mut right_lch = srgb_to_oklch(right.0, right.1, right.2)?;
    let left_alpha = left.3 as f64;
    let right_alpha = right.3 as f64;

    if left_alpha <= 1e-12 && right_alpha > 1e-12 {
        left_lch = right_lch;
    } else if right_alpha <= 1e-12 && left_alpha > 1e-12 {
        right_lch = left_lch;
    }

    if left_lch[1] <= OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        left_lch[2] = right_lch[2];
    }
    if right_lch[1] <= OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        right_lch[2] = left_lch[2];
    }

    let alpha = left_alpha * left_weight + right_alpha * right_weight;
    if alpha <= 1e-12 {
        return Some((0.0, 0.0, 0.0, 0.0));
    }

    let l = left_lch[0] * left_weight + right_lch[0] * right_weight;
    let c = left_lch[1] * left_weight + right_lch[1] * right_weight;
    let h = mix_hue_shorter(left_lch[2], right_lch[2], left_weight, right_weight);
    let a = c * h.to_radians().cos();
    let b = c * h.to_radians().sin();
    oklab_to_srgb(l, a, b, alpha)
}

fn mix_premultiplied(
    left_coords: [f64; 3],
    left_alpha: f64,
    right_coords: [f64; 3],
    right_alpha: f64,
    left_weight: f64,
    right_weight: f64,
) -> ([f64; 3], f64) {
    let alpha = left_alpha * left_weight + right_alpha * right_weight;
    if alpha <= 1e-12 {
        return ([0.0, 0.0, 0.0], 0.0);
    }

    let coords = [
        (left_coords[0] * left_alpha * left_weight + right_coords[0] * right_alpha * right_weight)
            / alpha,
        (left_coords[1] * left_alpha * left_weight + right_coords[1] * right_alpha * right_weight)
            / alpha,
        (left_coords[2] * left_alpha * left_weight + right_coords[2] * right_alpha * right_weight)
            / alpha,
    ];
    (coords, alpha)
}

fn mix_hue_shorter(left: f64, right: f64, left_weight: f64, right_weight: f64) -> f64 {
    let delta = (right - left + 180.0).rem_euclid(360.0) - 180.0;
    (left + delta * right_weight / (left_weight + right_weight)).rem_euclid(360.0)
}

/// Convert HSL to RGB. All inputs/outputs are normalised to 0.0–1.0 except
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
