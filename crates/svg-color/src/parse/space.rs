use super::clamp_channel;

const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];

pub(super) fn hwb_to_rgb(h: f32, w: f32, b: f32, alpha: f64) -> (f32, f32, f32, f32) {
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

pub(super) fn srgb_to_oklch(r: f32, g: f32, b: f32) -> Option<[f64; 3]> {
    let [l, a, b] = srgb_to_oklab(r, g, b)?;
    let c = (a * a + b * b).sqrt();
    let h = if c <= super::OKLCH_ACHROMATIC_CHROMA_THRESHOLD {
        0.0
    } else {
        b.atan2(a).to_degrees().rem_euclid(360.0)
    };
    Some([l, c, h])
}

pub(super) fn oklab_to_srgb(l: f64, a: f64, b: f64, alpha: f64) -> Option<(f32, f32, f32, f32)> {
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

/// Convert HSL to RGB. All inputs/outputs are normalised to 0.0–1.0 except
/// `h`, which is in degrees (0–360, wrapping).
pub(super) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
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
