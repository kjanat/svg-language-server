use super::OKLCH_ACHROMATIC_CHROMA_THRESHOLD;
use super::clamp_channel;
use super::space::{oklab_to_srgb, srgb_to_oklab, srgb_to_oklch};

pub(super) fn mix_srgb(
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

pub(super) fn mix_oklab(
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

pub(super) fn mix_oklch(
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
