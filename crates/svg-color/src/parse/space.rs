use super::clamp_channel;

const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

pub(super) fn hwb_to_rgb(h: f32, w: f32, b: f32, alpha: f64) -> (f32, f32, f32, f32) {
    let w = w.clamp(0.0, 1.0);
    let b = b.clamp(0.0, 1.0);

    if w + b >= 1.0 {
        let gray = w / (w + b);
        return (gray, gray, gray, clamp_channel(alpha));
    }

    let (hr, hg, hb) = hsl_to_rgb(h, 1.0, 0.5);
    let scale = 1.0 - w - b;
    (
        hr.mul_add(scale, w),
        hg.mul_add(scale, w),
        hb.mul_add(scale, w),
        clamp_channel(alpha),
    )
}

pub(super) fn lab_to_srgb(l: f64, a: f64, b: f64, alpha: f64) -> Option<(f32, f32, f32, f32)> {
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

pub(super) fn srgb_to_oklab(r: f32, g: f32, b: f32) -> Option<[f64; 3]> {
    let linear = [
        srgb_to_linear(f64::from(r)),
        srgb_to_linear(f64::from(g)),
        srgb_to_linear(f64::from(b)),
    ];
    let xyz = linear_srgb_to_xyz(linear);
    let lms = [
        (-0.128_859_713_7f64).mul_add(
            xyz[2],
            0.361_866_742_4f64.mul_add(xyz[1], 0.818_933_010_1 * xyz[0]),
        ),
        0.036_145_638_7f64.mul_add(
            xyz[2],
            0.929_311_871_5f64.mul_add(xyz[1], 0.032_984_543_6 * xyz[0]),
        ),
        0.633_851_707f64.mul_add(
            xyz[2],
            0.264_366_269_1f64.mul_add(xyz[1], 0.048_200_301_8 * xyz[0]),
        ),
    ]
    .map(f64::cbrt);

    let oklab = [
        (-0.004_072_046_8f64).mul_add(
            lms[2],
            0.793_617_785f64.mul_add(lms[1], 0.210_454_255_3 * lms[0]),
        ),
        0.450_593_709_9f64.mul_add(
            lms[2],
            (-2.428_592_205f64).mul_add(lms[1], 1.977_998_495_1 * lms[0]),
        ),
        (-0.808_675_766f64).mul_add(
            lms[2],
            0.782_771_766_2f64.mul_add(lms[1], 0.025_904_037_1 * lms[0]),
        ),
    ];

    if oklab.iter().all(|value| value.is_finite()) {
        Some(oklab)
    } else {
        None
    }
}

pub(super) fn srgb_to_oklch(red: f32, green: f32, blue: f32) -> Option<[f64; 3]> {
    let [lightness, axis_a, axis_b] = srgb_to_oklab(red, green, blue)?;
    let chroma = axis_a.hypot(axis_b);
    let hue = if chroma <= super::OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        axis_b.atan2(axis_a).to_degrees().rem_euclid(360.0)
    };
    Some([lightness, chroma, hue])
}

pub(super) fn oklab_to_srgb(l: f64, a: f64, b: f64, alpha: f64) -> Option<(f32, f32, f32, f32)> {
    let lms_nl = [
        0.215_803_757_309_913_6f64.mul_add(b, 0.396_337_777_376_174_9f64.mul_add(a, l)),
        (-0.063_854_172_825_813_3f64).mul_add(b, (-0.105_561_345_815_658_6f64).mul_add(a, l)),
        (-1.291_485_548_019_409_2f64).mul_add(b, (-0.089_484_177_529_811_9f64).mul_add(a, l)),
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

/// Convert HSL to RGB. All inputs/outputs are normalised to 0.0–1.0 except
/// `hue`, which is in degrees (0–360, wrapping).
pub(super) fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let saturation = saturation.clamp(0.0, 1.0);
    let lightness = lightness.clamp(0.0, 1.0);

    let chroma = (1.0 - 2.0f32.mul_add(lightness, -1.0).abs()) * saturation;
    let hue_prime = hue.rem_euclid(360.0) / 60.0;
    let secondary = chroma * (1.0 - (hue_prime % 2.0 - 1.0).abs());

    let (red_base, green_base, blue_base) = if hue_prime < 1.0 {
        (chroma, secondary, 0.0)
    } else if hue_prime < 2.0 {
        (secondary, chroma, 0.0)
    } else if hue_prime < 3.0 {
        (0.0, chroma, secondary)
    } else if hue_prime < 4.0 {
        (0.0, secondary, chroma)
    } else if hue_prime < 5.0 {
        (secondary, 0.0, chroma)
    } else {
        (chroma, 0.0, secondary)
    };

    let match_lightness = lightness - chroma / 2.0;
    (
        red_base + match_lightness,
        green_base + match_lightness,
        blue_base + match_lightness,
    )
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
            116.0f64.mul_add(fx, -16.0) / kappa
        },
        if l > kappa * epsilon {
            ((l + 16.0) / 116.0).powi(3)
        } else {
            l / kappa
        },
        if fz.powi(3) > epsilon {
            fz.powi(3)
        } else {
            116.0f64.mul_add(fz, -16.0) / kappa
        },
    ];

    [xyz[0] * D50[0], xyz[1] * D50[1], xyz[2] * D50[2]]
}

fn d50_to_d65([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        0.063_259_243_200_570_72f64.mul_add(
            z,
            (-0.023_098_454_948_764_71f64).mul_add(y, 0.955_473_421_488_075 * x),
        ),
        0.021_041_441_191_917_323f64.mul_add(
            z,
            1.009_995_398_081_304_1f64.mul_add(y, -0.028_369_709_333_863_7 * x),
        ),
        1.330_365_926_242_124f64.mul_add(
            z,
            (-0.020_507_649_298_898_964f64).mul_add(y, 0.012_314_014_864_481_998 * x),
        ),
    ]
}

fn ok_lab_lms_to_xyz([l, m, s]: [f64; 3]) -> [f64; 3] {
    [
        0.281_391_045_665_964_7f64.mul_add(
            s,
            (-0.557_814_994_460_217_1f64).mul_add(m, 1.226_879_875_845_924_3 * l),
        ),
        (-0.071_711_058_065_516_4f64).mul_add(
            s,
            1.112_286_803_280_317f64.mul_add(m, -0.040_575_745_214_800_8 * l),
        ),
        1.586_924_019_836_781_6f64.mul_add(
            s,
            (-0.421_493_332_402_243_2f64).mul_add(m, -0.076_372_936_674_660_1 * l),
        ),
    ]
}

fn linear_srgb_to_xyz([r, g, b]: [f64; 3]) -> [f64; 3] {
    [
        0.180_480_788_401_834_3f64.mul_add(
            b,
            0.357_584_339_383_877_96f64.mul_add(g, 0.412_390_799_265_959_34 * r),
        ),
        0.072_192_315_360_733_71f64.mul_add(
            b,
            0.715_168_678_767_755_9f64.mul_add(g, 0.212_639_005_871_510_27 * r),
        ),
        0.950_532_152_249_660_7f64.mul_add(
            b,
            0.119_194_779_794_625_99f64.mul_add(g, 0.019_330_818_715_591_85 * r),
        ),
    ]
}

fn xyz_to_linear_srgb([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        (-1_974.0f64 / 3_959.0).mul_add(
            z,
            (-329.0f64 / 214.0).mul_add(y, (12_831.0f64 / 3_959.0) * x),
        ),
        (36_519.0f64 / 878_810.0).mul_add(
            z,
            (1_648_619.0f64 / 878_810.0).mul_add(y, (-851_781.0f64 / 878_810.0) * x),
        ),
        (705.0f64 / 667.0).mul_add(
            z,
            (-2_585.0f64 / 12_673.0).mul_add(y, (705.0f64 / 12_673.0) * x),
        ),
    ]
}

fn srgb_to_linear(value: f64) -> f64 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let abs = value.abs();
    if abs <= 0.04045 {
        value / 12.92
    } else {
        sign * ((abs + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f64) -> f64 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let abs = value.abs();
    if abs > 0.003_130_8 {
        sign * 1.055f64.mul_add(abs.powf(1.0 / 2.4), -0.055)
    } else {
        12.92 * value
    }
}

fn cube(value: f64) -> f64 {
    value * value * value
}
