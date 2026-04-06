use super::{
    LCH_ACHROMATIC_CHROMA_THRESHOLD, parse_hue_f64, parse_modern_alpha, space, split_modern_args,
};

pub(super) fn parse_lab(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let lightness = parse_lab_lightness(components[0])?;
    let axis_a = parse_lab_axis(components[1])?;
    let axis_b = parse_lab_axis(components[2])?;
    let alpha = parse_modern_alpha(alpha)?;
    space::lab_to_srgb(lightness, axis_a, axis_b, alpha)
}

pub(super) fn parse_lch(rest: &str) -> Option<(f32, f32, f32, f32)> {
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
    let hue_rad = hue.to_radians();
    let axis_a = chroma * hue_rad.cos();
    let axis_b = chroma * hue_rad.sin();
    space::lab_to_srgb(lightness, axis_a, axis_b, alpha)
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
