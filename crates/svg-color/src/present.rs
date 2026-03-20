use crate::{named_colors, types::ColorKind};

/// All 148 CSS named color names, in the same order as the `named_colors::lookup` table.
/// Used for reverse lookup (RGB → name).
const NAMED_COLOR_NAMES: &[&str] = &[
    "aliceblue", "antiquewhite", "aqua", "aquamarine", "azure",
    "beige", "bisque", "black", "blanchedalmond", "blue",
    "blueviolet", "brown", "burlywood", "cadetblue", "chartreuse",
    "chocolate", "coral", "cornflowerblue", "cornsilk", "crimson",
    "cyan", "darkblue", "darkcyan", "darkgoldenrod", "darkgray",
    "darkgreen", "darkgrey", "darkkhaki", "darkmagenta", "darkolivegreen",
    "darkorange", "darkorchid", "darkred", "darksalmon", "darkseagreen",
    "darkslateblue", "darkslategray", "darkslategrey", "darkturquoise",
    "darkviolet", "deeppink", "deepskyblue", "dimgray", "dimgrey",
    "dodgerblue", "firebrick", "floralwhite", "forestgreen", "fuchsia",
    "gainsboro", "ghostwhite", "gold", "goldenrod", "gray", "green",
    "greenyellow", "grey", "honeydew", "hotpink", "indianred", "indigo",
    "ivory", "khaki", "lavender", "lavenderblush", "lawngreen",
    "lemonchiffon", "lightblue", "lightcoral", "lightcyan",
    "lightgoldenrodyellow", "lightgray", "lightgreen", "lightgrey",
    "lightpink", "lightsalmon", "lightseagreen", "lightskyblue",
    "lightslategray", "lightslategrey", "lightsteelblue", "lightyellow",
    "lime", "limegreen", "linen", "magenta", "maroon",
    "mediumaquamarine", "mediumblue", "mediumorchid", "mediumpurple",
    "mediumseagreen", "mediumslateblue", "mediumspringgreen",
    "mediumturquoise", "mediumvioletred", "midnightblue", "mintcream",
    "mistyrose", "moccasin", "navajowhite", "navy", "oldlace", "olive",
    "olivedrab", "orange", "orangered", "orchid", "palegoldenrod",
    "palegreen", "paleturquoise", "palevioletred", "papayawhip",
    "peachpuff", "peru", "pink", "plum", "powderblue", "purple",
    "rebeccapurple", "red", "rosybrown", "royalblue", "saddlebrown",
    "salmon", "sandybrown", "seagreen", "seashell", "sienna", "silver",
    "skyblue", "slateblue", "slategray", "slategrey", "snow",
    "springgreen", "steelblue", "tan", "teal", "thistle", "tomato",
    "turquoise", "violet", "wheat", "white", "whitesmoke", "yellow",
    "yellowgreen",
];

/// Convert RGB (each in `[0.0, 1.0]`) to HSL.
///
/// Returns `(hue_degrees, saturation_percent, lightness_percent)` as integers.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (u16, u8, u8) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let l = (max + min) / 2.0;

    let s = if delta == 0.0 {
        0.0_f32
    } else {
        delta / (1.0 - (2.0 * l - 1.0).abs())
    };

    let h = if delta == 0.0 {
        0.0_f32
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };

    // Normalise hue to [0, 360)
    let h = ((h % 360.0) + 360.0) % 360.0;

    (h.round() as u16, (s * 100.0).round() as u8, (l * 100.0).round() as u8)
}

/// Reverse-lookup: given integer RGB components, return the first matching CSS name.
fn reverse_named_lookup(ri: u8, gi: u8, bi: u8) -> Option<&'static str> {
    for &name in NAMED_COLOR_NAMES {
        if let Some((nr, ng, nb)) = named_colors::lookup(name) {
            let nri = (nr * 255.0).round() as u8;
            let ngi = (ng * 255.0).round() as u8;
            let nbi = (nb * 255.0).round() as u8;
            if nri == ri && ngi == gi && nbi == bi {
                return Some(name);
            }
        }
    }
    None
}

/// Format alpha as a minimal decimal string (no trailing zeros beyond one decimal place).
fn fmt_alpha(a: f32) -> String {
    // Use up to 4 significant decimal digits, then strip trailing zeros.
    let s = format!("{:.4}", a);
    let s = s.trim_end_matches('0');
    // Always keep at least one decimal place (e.g. "0.5" not "0.").
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_owned()
    }
}

/// Generate color presentation strings for a given RGBA color.
///
/// Returns a `Vec<String>` of alternative format strings, with the original
/// format (as identified by `original`) listed first.  All other formats
/// follow in a stable order: hex → rgb/rgba → hsl/hsla → named (when
/// applicable).
///
/// Rules:
/// - When `a == 1.0`: opaque forms only (`#RRGGBB`, `rgb(…)`, `hsl(…)`,
///   named). No 8-digit hex, no `rgba`, no `hsla`.
/// - When `a < 1.0`: alpha-bearing forms (`#RRGGBBAA`, `rgba(…)`, `hsla(…)`).
///   Named colors are never emitted for semi-transparent colors.
/// - The named color is included only when an exact RGB match exists in the
///   148-color CSS table and `a == 1.0`.
pub fn color_presentations(r: f32, g: f32, b: f32, a: f32, original: ColorKind) -> Vec<String> {
    let opaque = (a - 1.0).abs() < f32::EPSILON;

    let ri = (r * 255.0).round() as u8;
    let gi = (g * 255.0).round() as u8;
    let bi = (b * 255.0).round() as u8;
    let ai = (a * 255.0).round() as u8;

    let (hh, hs, hl) = rgb_to_hsl(r, g, b);
    let alpha_str = fmt_alpha(a);

    // Build each format string.
    let hex = if opaque {
        format!("#{:02x}{:02x}{:02x}", ri, gi, bi)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", ri, gi, bi, ai)
    };

    let rgb_str = if opaque {
        format!("rgb({}, {}, {})", ri, gi, bi)
    } else {
        format!("rgba({}, {}, {}, {})", ri, gi, bi, alpha_str)
    };

    let hsl_str = if opaque {
        format!("hsl({}, {}%, {}%)", hh, hs, hl)
    } else {
        format!("hsla({}, {}%, {}%, {})", hh, hs, hl, alpha_str)
    };

    let named: Option<&'static str> = if opaque {
        reverse_named_lookup(ri, gi, bi)
    } else {
        None
    };

    // Collect all applicable formats in canonical order: hex, rgb, hsl, named.
    let mut all: Vec<String> = Vec::with_capacity(4);
    all.push(hex.clone());
    all.push(rgb_str.clone());
    all.push(hsl_str.clone());
    if let Some(n) = named {
        all.push(n.to_owned());
    }

    // Determine which entry corresponds to the original format.
    let original_str: String = match original {
        ColorKind::Hex => hex,
        ColorKind::Functional => rgb_str,
        ColorKind::Named => named.map(|n| n.to_owned()).unwrap_or_else(|| rgb_str),
    };

    // Put the original format first; keep the rest in stable order.
    let mut result: Vec<String> = Vec::with_capacity(all.len());
    result.push(original_str.clone());
    for s in all {
        if s != original_str {
            result.push(s);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_presentations() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff0000"));
        assert!(results.iter().any(|s| s == "rgb(255, 0, 0)"));
        assert!(results.iter().any(|s| s.starts_with("hsl(")));
        // Original format should be first
        assert!(results[0] == "#ff0000");
    }

    #[test]
    fn hex_with_alpha() {
        let results = color_presentations(1.0, 0.0, 0.0, 0.5, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "#ff000080"));
        assert!(results.iter().any(|s| s == "rgba(255, 0, 0, 0.5)"));
    }

    #[test]
    fn named_color_included() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Named);
        assert!(results.iter().any(|s| s == "red"));
        // Named should be first when original is Named
        assert!(results[0] == "red");
    }

    #[test]
    fn functional_original_first() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Functional);
        assert!(results[0].starts_with("rgb(") || results[0].starts_with("hsl("));
    }

    #[test]
    fn no_alpha_suffix_when_opaque() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        // Should NOT include #ff0000ff (redundant alpha)
        assert!(!results.iter().any(|s| s == "#ff0000ff"));
        // Should NOT include rgba when alpha is 1.0
        assert!(!results.iter().any(|s| s.starts_with("rgba(")));
    }

    #[test]
    fn hsl_values_for_red() {
        let results = color_presentations(1.0, 0.0, 0.0, 1.0, ColorKind::Hex);
        assert!(results.iter().any(|s| s == "hsl(0, 100%, 50%)"));
    }

    #[test]
    fn alpha_formatting_minimal() {
        let results = color_presentations(0.0, 0.0, 1.0, 0.5, ColorKind::Functional);
        // Alpha should be "0.5" not "0.5000"
        assert!(results.iter().any(|s| s == "rgba(0, 0, 255, 0.5)"));
    }

    #[test]
    fn unnamed_color_no_named_entry() {
        // coral = (255, 127, 80) — has a name; but a slightly shifted value won't
        let results = color_presentations(1.0, 0.5, 0.5, 1.0, ColorKind::Hex);
        // (255, 128, 128) = lightcoral? no — lightcoral is (240,128,128). So no name.
        // Just verify the function returns at least hex, rgb, hsl.
        assert!(results.iter().any(|s| s.starts_with('#')));
        assert!(results.iter().any(|s| s.starts_with("rgb(")));
        assert!(results.iter().any(|s| s.starts_with("hsl(")));
    }
}
