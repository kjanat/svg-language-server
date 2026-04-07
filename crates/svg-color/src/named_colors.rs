/// Case-insensitive lookup of CSS named colors.
///
/// Returns the normalised RGB triple `(r, g, b)` with each component in `[0.0, 1.0]`
/// for the 148 CSS Color Level 4 named colors, or `None` if the name is unrecognised.
///
/// RGB values are sourced from the W3C CSS Color Level 4 specification:
/// <https://www.w3.org/TR/css-color-4/#named-colors>
#[must_use]
pub fn lookup(name: &str) -> Option<(f32, f32, f32)> {
    let lower = name.to_ascii_lowercase();
    let rgb = lookup_rgb(lower.as_str())?;
    Some((
        f32::from(rgb.0) / 255.0,
        f32::from(rgb.1) / 255.0,
        f32::from(rgb.2) / 255.0,
    ))
}

fn lookup_rgb(name: &str) -> Option<(u8, u8, u8)> {
    match name.as_bytes().first().copied()? {
        b'a'..=b'd' => lookup_rgb_a_to_d(name),
        b'e'..=b'l' => lookup_rgb_e_to_l(name),
        b'm'..=b'r' => lookup_rgb_m_to_r(name),
        b's'..=b'z' => lookup_rgb_s_to_z(name),
        _ => None,
    }
}

fn lookup_rgb_a_to_d(name: &str) -> Option<(u8, u8, u8)> {
    let rgb: (u8, u8, u8) = match name {
        "aliceblue" => (240, 248, 255),
        "antiquewhite" => (250, 235, 215),
        "aqua" | "cyan" => (0, 255, 255),
        "aquamarine" => (127, 255, 212),
        "azure" => (240, 255, 255),
        "beige" => (245, 245, 220),
        "bisque" => (255, 228, 196),
        "black" => (0, 0, 0),
        "blanchedalmond" => (255, 235, 205),
        "blue" => (0, 0, 255),
        "blueviolet" => (138, 43, 226),
        "brown" => (165, 42, 42),
        "burlywood" => (222, 184, 135),
        "cadetblue" => (95, 158, 160),
        "chartreuse" => (127, 255, 0),
        "chocolate" => (210, 105, 30),
        "coral" => (255, 127, 80),
        "cornflowerblue" => (100, 149, 237),
        "cornsilk" => (255, 248, 220),
        "crimson" => (220, 20, 60),
        "darkblue" => (0, 0, 139),
        "darkcyan" => (0, 139, 139),
        "darkgoldenrod" => (184, 134, 11),
        "darkgray" | "darkgrey" => (169, 169, 169),
        "darkgreen" => (0, 100, 0),
        "darkkhaki" => (189, 183, 107),
        "darkmagenta" => (139, 0, 139),
        "darkolivegreen" => (85, 107, 47),
        "darkorange" => (255, 140, 0),
        "darkorchid" => (153, 50, 204),
        "darkred" => (139, 0, 0),
        "darksalmon" => (233, 150, 122),
        "darkseagreen" => (143, 188, 143),
        "darkslateblue" => (72, 61, 139),
        "darkslategray" | "darkslategrey" => (47, 79, 79),
        "darkturquoise" => (0, 206, 209),
        "darkviolet" => (148, 0, 211),
        "deeppink" => (255, 20, 147),
        "deepskyblue" => (0, 191, 255),
        "dimgray" | "dimgrey" => (105, 105, 105),
        "dodgerblue" => (30, 144, 255),
        _ => return None,
    };
    Some(rgb)
}

fn lookup_rgb_e_to_l(name: &str) -> Option<(u8, u8, u8)> {
    let rgb: (u8, u8, u8) = match name {
        "firebrick" => (178, 34, 34),
        "floralwhite" => (255, 250, 240),
        "forestgreen" => (34, 139, 34),
        "fuchsia" => (255, 0, 255),
        "gainsboro" => (220, 220, 220),
        "ghostwhite" => (248, 248, 255),
        "gold" => (255, 215, 0),
        "goldenrod" => (218, 165, 32),
        "gray" | "grey" => (128, 128, 128),
        "green" => (0, 128, 0),
        "greenyellow" => (173, 255, 47),
        "honeydew" => (240, 255, 240),
        "hotpink" => (255, 105, 180),
        "indianred" => (205, 92, 92),
        "indigo" => (75, 0, 130),
        "ivory" => (255, 255, 240),
        "khaki" => (240, 230, 140),
        "lavender" => (230, 230, 250),
        "lavenderblush" => (255, 240, 245),
        "lawngreen" => (124, 252, 0),
        "lemonchiffon" => (255, 250, 205),
        "lightblue" => (173, 216, 230),
        "lightcoral" => (240, 128, 128),
        "lightcyan" => (224, 255, 255),
        "lightgoldenrodyellow" => (250, 250, 210),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "lightgreen" => (144, 238, 144),
        "lightpink" => (255, 182, 193),
        "lightsalmon" => (255, 160, 122),
        "lightseagreen" => (32, 178, 170),
        "lightskyblue" => (135, 206, 250),
        "lightslategray" | "lightslategrey" => (119, 136, 153),
        "lightsteelblue" => (176, 196, 222),
        "lightyellow" => (255, 255, 224),
        "lime" => (0, 255, 0),
        "limegreen" => (50, 205, 50),
        "linen" => (250, 240, 230),
        _ => return None,
    };
    Some(rgb)
}

fn lookup_rgb_m_to_r(name: &str) -> Option<(u8, u8, u8)> {
    let rgb: (u8, u8, u8) = match name {
        "magenta" => (255, 0, 255),
        "maroon" => (128, 0, 0),
        "mediumaquamarine" => (102, 205, 170),
        "mediumblue" => (0, 0, 205),
        "mediumorchid" => (186, 85, 211),
        "mediumpurple" => (147, 112, 219),
        "mediumseagreen" => (60, 179, 113),
        "mediumslateblue" => (123, 104, 238),
        "mediumspringgreen" => (0, 250, 154),
        "mediumturquoise" => (72, 209, 204),
        "mediumvioletred" => (199, 21, 133),
        "midnightblue" => (25, 25, 112),
        "mintcream" => (245, 255, 250),
        "mistyrose" => (255, 228, 225),
        "moccasin" => (255, 228, 181),
        "navajowhite" => (255, 222, 173),
        "navy" => (0, 0, 128),
        "oldlace" => (253, 245, 230),
        "olive" => (128, 128, 0),
        "olivedrab" => (107, 142, 35),
        "orange" => (255, 165, 0),
        "orangered" => (255, 69, 0),
        "orchid" => (218, 112, 214),
        "palegoldenrod" => (238, 232, 170),
        "palegreen" => (152, 251, 152),
        "paleturquoise" => (175, 238, 238),
        "palevioletred" => (219, 112, 147),
        "papayawhip" => (255, 239, 213),
        "peachpuff" => (255, 218, 185),
        "peru" => (205, 133, 63),
        "pink" => (255, 192, 203),
        "plum" => (221, 160, 221),
        "powderblue" => (176, 224, 230),
        "purple" => (128, 0, 128),
        "rebeccapurple" => (102, 51, 153),
        "red" => (255, 0, 0),
        "rosybrown" => (188, 143, 143),
        "royalblue" => (65, 105, 225),
        _ => return None,
    };
    Some(rgb)
}

fn lookup_rgb_s_to_z(name: &str) -> Option<(u8, u8, u8)> {
    let rgb: (u8, u8, u8) = match name {
        "saddlebrown" => (139, 69, 19),
        "salmon" => (250, 128, 114),
        "sandybrown" => (244, 164, 96),
        "seagreen" => (46, 139, 87),
        "seashell" => (255, 245, 238),
        "sienna" => (160, 82, 45),
        "silver" => (192, 192, 192),
        "skyblue" => (135, 206, 235),
        "slateblue" => (106, 90, 205),
        "slategray" | "slategrey" => (112, 128, 144),
        "snow" => (255, 250, 250),
        "springgreen" => (0, 255, 127),
        "steelblue" => (70, 130, 180),
        "tan" => (210, 180, 140),
        "teal" => (0, 128, 128),
        "thistle" => (216, 191, 216),
        "tomato" => (255, 99, 71),
        "turquoise" => (64, 224, 208),
        "violet" => (238, 130, 238),
        "wheat" => (245, 222, 179),
        "white" => (255, 255, 255),
        "whitesmoke" => (245, 245, 245),
        "yellow" => (255, 255, 0),
        "yellowgreen" => (154, 205, 50),
        _ => return None,
    };
    Some(rgb)
}

/// All 148 CSS Color Level 4 named colors.
const ALL_NAMES: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// Reverse-lookup: given integer RGB components, return a matching CSS name.
///
/// Uses a lazily-built `HashMap` keyed by `(r, g, b)` bytes so each call
/// after initialization is O(1) instead of scanning all 148 entries.
#[must_use]
pub fn reverse_lookup(r: u8, g: u8, b: u8) -> Option<&'static str> {
    use std::{collections::HashMap, sync::LazyLock};

    static RGB_TO_NAME: LazyLock<HashMap<(u8, u8, u8), &'static str>> = LazyLock::new(|| {
        let mut map = HashMap::with_capacity(ALL_NAMES.len());
        for &name in ALL_NAMES {
            if let Some(rgb) = lookup_rgb(name) {
                map.entry(rgb).or_insert(name);
            }
        }
        map
    });

    RGB_TO_NAME.get(&(r, g, b)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_colors() {
        assert_eq!(lookup("red"), Some((1.0, 0.0, 0.0)));
        assert_eq!(lookup("lime"), Some((0.0, 1.0, 0.0)));
        assert_eq!(lookup("blue"), Some((0.0, 0.0, 1.0)));
        assert_eq!(lookup("white"), Some((1.0, 1.0, 1.0)));
        assert_eq!(lookup("black"), Some((0.0, 0.0, 0.0)));
        assert_eq!(
            lookup("coral"),
            Some((255.0 / 255.0, 127.0 / 255.0, 80.0 / 255.0))
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(lookup("Red"), Some((1.0, 0.0, 0.0)));
        assert_eq!(lookup("RED"), Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn unknown_names() {
        assert_eq!(lookup("banana"), None);
        assert_eq!(lookup("notacolor"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn all_148_colors_present() {
        assert_eq!(ALL_NAMES.len(), 148, "expected 148 CSS named colors");
        for &name in ALL_NAMES {
            assert!(lookup(name).is_some(), "missing named color: {name}");
        }
    }

    #[test]
    fn reverse_lookup_aliceblue() {
        assert_eq!(reverse_lookup(240, 248, 255), Some("aliceblue"));
    }

    #[test]
    fn reverse_lookup_antiquewhite() {
        assert_eq!(reverse_lookup(250, 235, 215), Some("antiquewhite"));
    }

    #[test]
    fn reverse_lookup_aqua() {
        assert_eq!(reverse_lookup(0, 255, 255), Some("aqua"));
    }

    #[test]
    fn reverse_lookup_aquamarine() {
        assert_eq!(reverse_lookup(127, 255, 212), Some("aquamarine"));
    }

    #[test]
    fn reverse_lookup_azure() {
        assert_eq!(reverse_lookup(240, 255, 255), Some("azure"));
    }

    #[test]
    fn reverse_lookup_beige() {
        assert_eq!(reverse_lookup(245, 245, 220), Some("beige"));
    }

    #[test]
    fn reverse_lookup_bisque() {
        assert_eq!(reverse_lookup(255, 228, 196), Some("bisque"));
    }

    #[test]
    fn reverse_lookup_black() {
        assert_eq!(reverse_lookup(0, 0, 0), Some("black"));
    }

    #[test]
    fn reverse_lookup_blanchedalmond() {
        assert_eq!(reverse_lookup(255, 235, 205), Some("blanchedalmond"));
    }

    #[test]
    fn reverse_lookup_blue() {
        assert_eq!(reverse_lookup(0, 0, 255), Some("blue"));
    }

    #[test]
    fn reverse_lookup_blueviolet() {
        assert_eq!(reverse_lookup(138, 43, 226), Some("blueviolet"));
    }

    #[test]
    fn reverse_lookup_brown() {
        assert_eq!(reverse_lookup(165, 42, 42), Some("brown"));
    }

    #[test]
    fn reverse_lookup_burlywood() {
        assert_eq!(reverse_lookup(222, 184, 135), Some("burlywood"));
    }

    #[test]
    fn reverse_lookup_cadetblue() {
        assert_eq!(reverse_lookup(95, 158, 160), Some("cadetblue"));
    }

    #[test]
    fn reverse_lookup_chartreuse() {
        assert_eq!(reverse_lookup(127, 255, 0), Some("chartreuse"));
    }

    #[test]
    fn reverse_lookup_chocolate() {
        assert_eq!(reverse_lookup(210, 105, 30), Some("chocolate"));
    }

    #[test]
    fn reverse_lookup_coral() {
        assert_eq!(reverse_lookup(255, 127, 80), Some("coral"));
    }

    #[test]
    fn reverse_lookup_cornflowerblue() {
        assert_eq!(reverse_lookup(100, 149, 237), Some("cornflowerblue"));
    }

    #[test]
    fn reverse_lookup_cornsilk() {
        assert_eq!(reverse_lookup(255, 248, 220), Some("cornsilk"));
    }

    #[test]
    fn reverse_lookup_crimson() {
        assert_eq!(reverse_lookup(220, 20, 60), Some("crimson"));
    }

    #[test]
    fn reverse_lookup_cyan() {
        assert_eq!(reverse_lookup(0, 255, 255), Some("aqua"));
    }

    #[test]
    fn reverse_lookup_darkblue() {
        assert_eq!(reverse_lookup(0, 0, 139), Some("darkblue"));
    }

    #[test]
    fn reverse_lookup_darkcyan() {
        assert_eq!(reverse_lookup(0, 139, 139), Some("darkcyan"));
    }

    #[test]
    fn reverse_lookup_darkgoldenrod() {
        assert_eq!(reverse_lookup(184, 134, 11), Some("darkgoldenrod"));
    }

    #[test]
    fn reverse_lookup_darkgray() {
        assert_eq!(reverse_lookup(169, 169, 169), Some("darkgray"));
    }

    #[test]
    fn reverse_lookup_darkgrey() {
        assert_eq!(reverse_lookup(169, 169, 169), Some("darkgray"));
    }

    #[test]
    fn reverse_lookup_darkgreen() {
        assert_eq!(reverse_lookup(0, 100, 0), Some("darkgreen"));
    }

    #[test]
    fn reverse_lookup_darkkhaki() {
        assert_eq!(reverse_lookup(189, 183, 107), Some("darkkhaki"));
    }

    #[test]
    fn reverse_lookup_darkmagenta() {
        assert_eq!(reverse_lookup(139, 0, 139), Some("darkmagenta"));
    }

    #[test]
    fn reverse_lookup_darkolivegreen() {
        assert_eq!(reverse_lookup(85, 107, 47), Some("darkolivegreen"));
    }

    #[test]
    fn reverse_lookup_darkorange() {
        assert_eq!(reverse_lookup(255, 140, 0), Some("darkorange"));
    }

    #[test]
    fn reverse_lookup_darkorchid() {
        assert_eq!(reverse_lookup(153, 50, 204), Some("darkorchid"));
    }

    #[test]
    fn reverse_lookup_darkred() {
        assert_eq!(reverse_lookup(139, 0, 0), Some("darkred"));
    }

    #[test]
    fn reverse_lookup_darksalmon() {
        assert_eq!(reverse_lookup(233, 150, 122), Some("darksalmon"));
    }

    #[test]
    fn reverse_lookup_darkseagreen() {
        assert_eq!(reverse_lookup(143, 188, 143), Some("darkseagreen"));
    }

    #[test]
    fn reverse_lookup_darkslateblue() {
        assert_eq!(reverse_lookup(72, 61, 139), Some("darkslateblue"));
    }

    #[test]
    fn reverse_lookup_darkslategray() {
        assert_eq!(reverse_lookup(47, 79, 79), Some("darkslategray"));
    }

    #[test]
    fn reverse_lookup_darkslategrey() {
        assert_eq!(reverse_lookup(47, 79, 79), Some("darkslategray"));
    }

    #[test]
    fn reverse_lookup_darkturquoise() {
        assert_eq!(reverse_lookup(0, 206, 209), Some("darkturquoise"));
    }

    #[test]
    fn reverse_lookup_darkviolet() {
        assert_eq!(reverse_lookup(148, 0, 211), Some("darkviolet"));
    }

    #[test]
    fn reverse_lookup_deeppink() {
        assert_eq!(reverse_lookup(255, 20, 147), Some("deeppink"));
    }

    #[test]
    fn reverse_lookup_deepskyblue() {
        assert_eq!(reverse_lookup(0, 191, 255), Some("deepskyblue"));
    }

    #[test]
    fn reverse_lookup_dimgray() {
        assert_eq!(reverse_lookup(105, 105, 105), Some("dimgray"));
    }

    #[test]
    fn reverse_lookup_dimgrey() {
        assert_eq!(reverse_lookup(105, 105, 105), Some("dimgray"));
    }

    #[test]
    fn reverse_lookup_dodgerblue() {
        assert_eq!(reverse_lookup(30, 144, 255), Some("dodgerblue"));
    }

    #[test]
    fn reverse_lookup_firebrick() {
        assert_eq!(reverse_lookup(178, 34, 34), Some("firebrick"));
    }

    #[test]
    fn reverse_lookup_floralwhite() {
        assert_eq!(reverse_lookup(255, 250, 240), Some("floralwhite"));
    }

    #[test]
    fn reverse_lookup_forestgreen() {
        assert_eq!(reverse_lookup(34, 139, 34), Some("forestgreen"));
    }

    #[test]
    fn reverse_lookup_fuchsia() {
        assert_eq!(reverse_lookup(255, 0, 255), Some("fuchsia"));
    }

    #[test]
    fn reverse_lookup_gainsboro() {
        assert_eq!(reverse_lookup(220, 220, 220), Some("gainsboro"));
    }

    #[test]
    fn reverse_lookup_ghostwhite() {
        assert_eq!(reverse_lookup(248, 248, 255), Some("ghostwhite"));
    }

    #[test]
    fn reverse_lookup_gold() {
        assert_eq!(reverse_lookup(255, 215, 0), Some("gold"));
    }

    #[test]
    fn reverse_lookup_goldenrod() {
        assert_eq!(reverse_lookup(218, 165, 32), Some("goldenrod"));
    }

    #[test]
    fn reverse_lookup_gray() {
        assert_eq!(reverse_lookup(128, 128, 128), Some("gray"));
    }

    #[test]
    fn reverse_lookup_green() {
        assert_eq!(reverse_lookup(0, 128, 0), Some("green"));
    }

    #[test]
    fn reverse_lookup_greenyellow() {
        assert_eq!(reverse_lookup(173, 255, 47), Some("greenyellow"));
    }

    #[test]
    fn reverse_lookup_grey() {
        assert_eq!(reverse_lookup(128, 128, 128), Some("gray"));
    }

    #[test]
    fn reverse_lookup_honeydew() {
        assert_eq!(reverse_lookup(240, 255, 240), Some("honeydew"));
    }

    #[test]
    fn reverse_lookup_hotpink() {
        assert_eq!(reverse_lookup(255, 105, 180), Some("hotpink"));
    }

    #[test]
    fn reverse_lookup_indianred() {
        assert_eq!(reverse_lookup(205, 92, 92), Some("indianred"));
    }

    #[test]
    fn reverse_lookup_indigo() {
        assert_eq!(reverse_lookup(75, 0, 130), Some("indigo"));
    }

    #[test]
    fn reverse_lookup_ivory() {
        assert_eq!(reverse_lookup(255, 255, 240), Some("ivory"));
    }

    #[test]
    fn reverse_lookup_khaki() {
        assert_eq!(reverse_lookup(240, 230, 140), Some("khaki"));
    }

    #[test]
    fn reverse_lookup_lavender() {
        assert_eq!(reverse_lookup(230, 230, 250), Some("lavender"));
    }

    #[test]
    fn reverse_lookup_lavenderblush() {
        assert_eq!(reverse_lookup(255, 240, 245), Some("lavenderblush"));
    }

    #[test]
    fn reverse_lookup_lawngreen() {
        assert_eq!(reverse_lookup(124, 252, 0), Some("lawngreen"));
    }

    #[test]
    fn reverse_lookup_lemonchiffon() {
        assert_eq!(reverse_lookup(255, 250, 205), Some("lemonchiffon"));
    }

    #[test]
    fn reverse_lookup_lightblue() {
        assert_eq!(reverse_lookup(173, 216, 230), Some("lightblue"));
    }

    #[test]
    fn reverse_lookup_lightcoral() {
        assert_eq!(reverse_lookup(240, 128, 128), Some("lightcoral"));
    }

    #[test]
    fn reverse_lookup_lightcyan() {
        assert_eq!(reverse_lookup(224, 255, 255), Some("lightcyan"));
    }

    #[test]
    fn reverse_lookup_lightgoldenrodyellow() {
        assert_eq!(reverse_lookup(250, 250, 210), Some("lightgoldenrodyellow"));
    }

    #[test]
    fn reverse_lookup_lightgray() {
        assert_eq!(reverse_lookup(211, 211, 211), Some("lightgray"));
    }

    #[test]
    fn reverse_lookup_lightgreen() {
        assert_eq!(reverse_lookup(144, 238, 144), Some("lightgreen"));
    }

    #[test]
    fn reverse_lookup_lightgrey() {
        assert_eq!(reverse_lookup(211, 211, 211), Some("lightgray"));
    }

    #[test]
    fn reverse_lookup_lightpink() {
        assert_eq!(reverse_lookup(255, 182, 193), Some("lightpink"));
    }

    #[test]
    fn reverse_lookup_lightsalmon() {
        assert_eq!(reverse_lookup(255, 160, 122), Some("lightsalmon"));
    }

    #[test]
    fn reverse_lookup_lightseagreen() {
        assert_eq!(reverse_lookup(32, 178, 170), Some("lightseagreen"));
    }

    #[test]
    fn reverse_lookup_lightskyblue() {
        assert_eq!(reverse_lookup(135, 206, 250), Some("lightskyblue"));
    }

    #[test]
    fn reverse_lookup_lightslategray() {
        assert_eq!(reverse_lookup(119, 136, 153), Some("lightslategray"));
    }

    #[test]
    fn reverse_lookup_lightslategrey() {
        assert_eq!(reverse_lookup(119, 136, 153), Some("lightslategray"));
    }

    #[test]
    fn reverse_lookup_lightsteelblue() {
        assert_eq!(reverse_lookup(176, 196, 222), Some("lightsteelblue"));
    }

    #[test]
    fn reverse_lookup_lightyellow() {
        assert_eq!(reverse_lookup(255, 255, 224), Some("lightyellow"));
    }

    #[test]
    fn reverse_lookup_lime() {
        assert_eq!(reverse_lookup(0, 255, 0), Some("lime"));
    }

    #[test]
    fn reverse_lookup_limegreen() {
        assert_eq!(reverse_lookup(50, 205, 50), Some("limegreen"));
    }

    #[test]
    fn reverse_lookup_linen() {
        assert_eq!(reverse_lookup(250, 240, 230), Some("linen"));
    }

    #[test]
    fn reverse_lookup_magenta() {
        assert_eq!(reverse_lookup(255, 0, 255), Some("fuchsia"));
    }

    #[test]
    fn reverse_lookup_maroon() {
        assert_eq!(reverse_lookup(128, 0, 0), Some("maroon"));
    }

    #[test]
    fn reverse_lookup_mediumaquamarine() {
        assert_eq!(reverse_lookup(102, 205, 170), Some("mediumaquamarine"));
    }

    #[test]
    fn reverse_lookup_mediumblue() {
        assert_eq!(reverse_lookup(0, 0, 205), Some("mediumblue"));
    }

    #[test]
    fn reverse_lookup_mediumorchid() {
        assert_eq!(reverse_lookup(186, 85, 211), Some("mediumorchid"));
    }

    #[test]
    fn reverse_lookup_mediumpurple() {
        assert_eq!(reverse_lookup(147, 112, 219), Some("mediumpurple"));
    }

    #[test]
    fn reverse_lookup_mediumseagreen() {
        assert_eq!(reverse_lookup(60, 179, 113), Some("mediumseagreen"));
    }

    #[test]
    fn reverse_lookup_mediumslateblue() {
        assert_eq!(reverse_lookup(123, 104, 238), Some("mediumslateblue"));
    }

    #[test]
    fn reverse_lookup_mediumspringgreen() {
        assert_eq!(reverse_lookup(0, 250, 154), Some("mediumspringgreen"));
    }

    #[test]
    fn reverse_lookup_mediumturquoise() {
        assert_eq!(reverse_lookup(72, 209, 204), Some("mediumturquoise"));
    }

    #[test]
    fn reverse_lookup_mediumvioletred() {
        assert_eq!(reverse_lookup(199, 21, 133), Some("mediumvioletred"));
    }

    #[test]
    fn reverse_lookup_midnightblue() {
        assert_eq!(reverse_lookup(25, 25, 112), Some("midnightblue"));
    }

    #[test]
    fn reverse_lookup_mintcream() {
        assert_eq!(reverse_lookup(245, 255, 250), Some("mintcream"));
    }

    #[test]
    fn reverse_lookup_mistyrose() {
        assert_eq!(reverse_lookup(255, 228, 225), Some("mistyrose"));
    }

    #[test]
    fn reverse_lookup_moccasin() {
        assert_eq!(reverse_lookup(255, 228, 181), Some("moccasin"));
    }

    #[test]
    fn reverse_lookup_navajowhite() {
        assert_eq!(reverse_lookup(255, 222, 173), Some("navajowhite"));
    }

    #[test]
    fn reverse_lookup_navy() {
        assert_eq!(reverse_lookup(0, 0, 128), Some("navy"));
    }

    #[test]
    fn reverse_lookup_oldlace() {
        assert_eq!(reverse_lookup(253, 245, 230), Some("oldlace"));
    }

    #[test]
    fn reverse_lookup_olive() {
        assert_eq!(reverse_lookup(128, 128, 0), Some("olive"));
    }

    #[test]
    fn reverse_lookup_olivedrab() {
        assert_eq!(reverse_lookup(107, 142, 35), Some("olivedrab"));
    }

    #[test]
    fn reverse_lookup_orange() {
        assert_eq!(reverse_lookup(255, 165, 0), Some("orange"));
    }

    #[test]
    fn reverse_lookup_orangered() {
        assert_eq!(reverse_lookup(255, 69, 0), Some("orangered"));
    }

    #[test]
    fn reverse_lookup_orchid() {
        assert_eq!(reverse_lookup(218, 112, 214), Some("orchid"));
    }

    #[test]
    fn reverse_lookup_palegoldenrod() {
        assert_eq!(reverse_lookup(238, 232, 170), Some("palegoldenrod"));
    }

    #[test]
    fn reverse_lookup_palegreen() {
        assert_eq!(reverse_lookup(152, 251, 152), Some("palegreen"));
    }

    #[test]
    fn reverse_lookup_paleturquoise() {
        assert_eq!(reverse_lookup(175, 238, 238), Some("paleturquoise"));
    }

    #[test]
    fn reverse_lookup_palevioletred() {
        assert_eq!(reverse_lookup(219, 112, 147), Some("palevioletred"));
    }

    #[test]
    fn reverse_lookup_papayawhip() {
        assert_eq!(reverse_lookup(255, 239, 213), Some("papayawhip"));
    }

    #[test]
    fn reverse_lookup_peachpuff() {
        assert_eq!(reverse_lookup(255, 218, 185), Some("peachpuff"));
    }

    #[test]
    fn reverse_lookup_peru() {
        assert_eq!(reverse_lookup(205, 133, 63), Some("peru"));
    }

    #[test]
    fn reverse_lookup_pink() {
        assert_eq!(reverse_lookup(255, 192, 203), Some("pink"));
    }

    #[test]
    fn reverse_lookup_plum() {
        assert_eq!(reverse_lookup(221, 160, 221), Some("plum"));
    }

    #[test]
    fn reverse_lookup_powderblue() {
        assert_eq!(reverse_lookup(176, 224, 230), Some("powderblue"));
    }

    #[test]
    fn reverse_lookup_purple() {
        assert_eq!(reverse_lookup(128, 0, 128), Some("purple"));
    }

    #[test]
    fn reverse_lookup_rebeccapurple() {
        assert_eq!(reverse_lookup(102, 51, 153), Some("rebeccapurple"));
    }

    #[test]
    fn reverse_lookup_red() {
        assert_eq!(reverse_lookup(255, 0, 0), Some("red"));
    }

    #[test]
    fn reverse_lookup_rosybrown() {
        assert_eq!(reverse_lookup(188, 143, 143), Some("rosybrown"));
    }

    #[test]
    fn reverse_lookup_royalblue() {
        assert_eq!(reverse_lookup(65, 105, 225), Some("royalblue"));
    }

    #[test]
    fn reverse_lookup_saddlebrown() {
        assert_eq!(reverse_lookup(139, 69, 19), Some("saddlebrown"));
    }

    #[test]
    fn reverse_lookup_salmon() {
        assert_eq!(reverse_lookup(250, 128, 114), Some("salmon"));
    }

    #[test]
    fn reverse_lookup_sandybrown() {
        assert_eq!(reverse_lookup(244, 164, 96), Some("sandybrown"));
    }

    #[test]
    fn reverse_lookup_seagreen() {
        assert_eq!(reverse_lookup(46, 139, 87), Some("seagreen"));
    }

    #[test]
    fn reverse_lookup_seashell() {
        assert_eq!(reverse_lookup(255, 245, 238), Some("seashell"));
    }

    #[test]
    fn reverse_lookup_sienna() {
        assert_eq!(reverse_lookup(160, 82, 45), Some("sienna"));
    }

    #[test]
    fn reverse_lookup_silver() {
        assert_eq!(reverse_lookup(192, 192, 192), Some("silver"));
    }

    #[test]
    fn reverse_lookup_skyblue() {
        assert_eq!(reverse_lookup(135, 206, 235), Some("skyblue"));
    }

    #[test]
    fn reverse_lookup_slateblue() {
        assert_eq!(reverse_lookup(106, 90, 205), Some("slateblue"));
    }

    #[test]
    fn reverse_lookup_slategray() {
        assert_eq!(reverse_lookup(112, 128, 144), Some("slategray"));
    }

    #[test]
    fn reverse_lookup_slategrey() {
        assert_eq!(reverse_lookup(112, 128, 144), Some("slategray"));
    }

    #[test]
    fn reverse_lookup_snow() {
        assert_eq!(reverse_lookup(255, 250, 250), Some("snow"));
    }

    #[test]
    fn reverse_lookup_springgreen() {
        assert_eq!(reverse_lookup(0, 255, 127), Some("springgreen"));
    }

    #[test]
    fn reverse_lookup_steelblue() {
        assert_eq!(reverse_lookup(70, 130, 180), Some("steelblue"));
    }

    #[test]
    fn reverse_lookup_tan() {
        assert_eq!(reverse_lookup(210, 180, 140), Some("tan"));
    }

    #[test]
    fn reverse_lookup_teal() {
        assert_eq!(reverse_lookup(0, 128, 128), Some("teal"));
    }

    #[test]
    fn reverse_lookup_thistle() {
        assert_eq!(reverse_lookup(216, 191, 216), Some("thistle"));
    }

    #[test]
    fn reverse_lookup_tomato() {
        assert_eq!(reverse_lookup(255, 99, 71), Some("tomato"));
    }

    #[test]
    fn reverse_lookup_turquoise() {
        assert_eq!(reverse_lookup(64, 224, 208), Some("turquoise"));
    }

    #[test]
    fn reverse_lookup_violet() {
        assert_eq!(reverse_lookup(238, 130, 238), Some("violet"));
    }

    #[test]
    fn reverse_lookup_wheat() {
        assert_eq!(reverse_lookup(245, 222, 179), Some("wheat"));
    }

    #[test]
    fn reverse_lookup_white() {
        assert_eq!(reverse_lookup(255, 255, 255), Some("white"));
    }

    #[test]
    fn reverse_lookup_whitesmoke() {
        assert_eq!(reverse_lookup(245, 245, 245), Some("whitesmoke"));
    }

    #[test]
    fn reverse_lookup_yellow() {
        assert_eq!(reverse_lookup(255, 255, 0), Some("yellow"));
    }

    #[test]
    fn reverse_lookup_yellowgreen() {
        assert_eq!(reverse_lookup(154, 205, 50), Some("yellowgreen"));
    }

    #[test]
    fn reverse_lookup_no_match() {
        assert_eq!(reverse_lookup(1, 2, 3), None);
    }
}
