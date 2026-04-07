use super::{
    OKLCH_ACHROMATIC_CHROMA_THRESHOLD, parse_hue_f64, parse_modern_alpha, space, split_modern_args,
};

pub(super) fn parse_oklab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let l = parse_oklab_lightness(components[0])?;
    let a = parse_oklab_axis(components[1])?;
    let b = parse_oklab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    space::oklab_to_srgb(l, a, b, alpha)
}

pub(super) fn parse_oklch(rest: &str) -> Option<(f32, f32, f32, f32)> {
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
    let hue_rad = hue.to_radians();
    let axis_a = chroma * hue_rad.cos();
    let axis_b = chroma * hue_rad.sin();
    space::oklab_to_srgb(lightness, axis_a, axis_b, alpha)
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
    if !value.is_finite() {
        return None;
    }
    Some(value.clamp(0.0, 1.0))
}

fn parse_oklab_axis(s: &str) -> Option<f64> {
    if s.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    if let Some(percent) = s.strip_suffix('%') {
        let value = percent.trim().parse::<f64>().ok()?;
        if !value.is_finite() {
            return None;
        }
        return Some(value * 0.4 / 100.0);
    }
    let value = s.trim().parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(value)
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
    if !value.is_finite() {
        return None;
    }
    Some(value.max(0.0))
}

fn parse_oklch_hue(s: &str) -> Option<f64> {
    parse_hue_f64(s)
}
