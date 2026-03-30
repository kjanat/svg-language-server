use super::{
    clamp_channel, parse_alpha, parse_hue, parse_modern_alpha, parse_modern_percent_or_number_100,
    space, split_legacy_args, split_modern_args,
};

pub(super) fn parse_legacy_rgb(rest: &str) -> Option<(f32, f32, f32, f32)> {
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

pub(super) fn parse_legacy_hsl(rest: &str) -> Option<(f32, f32, f32, f32)> {
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

pub(super) fn parse_modern_rgb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let r = parse_modern_rgb_component(components[0])?;
    let g = parse_modern_rgb_component(components[1])?;
    let b = parse_modern_rgb_component(components[2])?;
    let a = clamp_channel(parse_modern_alpha(alpha)?);
    Some((r, g, b, a))
}

pub(super) fn parse_modern_hsl(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let hue = parse_hue(components[0])?;
    let saturation = parse_modern_percent_or_number_100(components[1])?;
    let lightness = parse_modern_percent_or_number_100(components[2])?;
    let alpha = clamp_channel(parse_modern_alpha(alpha)?);
    let (red, green, blue) = space::hsl_to_rgb(hue, saturation, lightness);
    Some((red, green, blue, alpha))
}

pub(super) fn parse_hwb(rest: &str) -> Option<(f32, f32, f32, f32)> {
    let (components, alpha) = split_modern_args(rest)?;
    let h = parse_hue(components[0])?;
    let w = parse_modern_percent_or_number_100(components[1])?;
    let b = parse_modern_percent_or_number_100(components[2])?;
    let a = parse_modern_alpha(alpha)?;
    Some(space::hwb_to_rgb(h, w, b, a))
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
