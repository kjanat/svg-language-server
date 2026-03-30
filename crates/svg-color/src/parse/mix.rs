use super::{
    OKLCH_ACHROMATIC_CHROMA_THRESHOLD, clamp_channel,
    space::{oklab_to_srgb, srgb_to_oklab, srgb_to_oklch},
};

pub(super) fn mix_srgb(
    left: (f32, f32, f32, f32),
    left_weight: f64,
    right: (f32, f32, f32, f32),
    right_weight: f64,
) -> (f32, f32, f32, f32) {
    let left = [
        f64::from(left.0),
        f64::from(left.1),
        f64::from(left.2),
        f64::from(left.3),
    ];
    let right = [
        f64::from(right.0),
        f64::from(right.1),
        f64::from(right.2),
        f64::from(right.3),
    ];
    let (coords, alpha) = mix_premultiplied(
        [left[0], left[1], left[2]],
        left[3],
        [right[0], right[1], right[2]],
        right[3],
        left_weight,
        right_weight,
    );
    (
        clamp_channel(coords[0]),
        clamp_channel(coords[1]),
        clamp_channel(coords[2]),
        clamp_channel(alpha),
    )
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
        f64::from(left.3),
        right_lab,
        f64::from(right.3),
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
    let left_alpha = f64::from(left.3);
    let right_alpha = f64::from(right.3);

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

    let alpha = right_alpha.mul_add(right_weight, left_alpha * left_weight);
    if alpha <= 1e-12 {
        return Some((0.0, 0.0, 0.0, 0.0));
    }

    let lightness = right_lch[0].mul_add(right_weight, left_lch[0] * left_weight);
    let chroma = right_lch[1].mul_add(right_weight, left_lch[1] * left_weight);
    let hue = mix_hue_shorter(left_lch[2], right_lch[2], left_weight, right_weight);
    let axis_a = chroma * hue.to_radians().cos();
    let axis_b = chroma * hue.to_radians().sin();
    oklab_to_srgb(lightness, axis_a, axis_b, alpha)
}

fn mix_premultiplied(
    left_coords: [f64; 3],
    left_alpha: f64,
    right_coords: [f64; 3],
    right_alpha: f64,
    left_weight: f64,
    right_weight: f64,
) -> ([f64; 3], f64) {
    let alpha = right_alpha.mul_add(right_weight, left_alpha * left_weight);
    if alpha <= 1e-12 {
        return ([0.0, 0.0, 0.0], 0.0);
    }

    let coords = [
        (right_coords[0] * right_alpha)
            .mul_add(right_weight, left_coords[0] * left_alpha * left_weight)
            / alpha,
        (right_coords[1] * right_alpha)
            .mul_add(right_weight, left_coords[1] * left_alpha * left_weight)
            / alpha,
        (right_coords[2] * right_alpha)
            .mul_add(right_weight, left_coords[2] * left_alpha * left_weight)
            / alpha,
    ];
    (coords, alpha)
}

fn mix_hue_shorter(left: f64, right: f64, left_weight: f64, right_weight: f64) -> f64 {
    let delta = (right - left + 180.0).rem_euclid(360.0) - 180.0;
    (left + delta * right_weight / (left_weight + right_weight)).rem_euclid(360.0)
}
