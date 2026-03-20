/// Parse a hex color string like `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
pub fn parse_hex(text: &str) -> Option<(f32, f32, f32, f32)> {
    let hex = text.strip_prefix('#')?;

    let to_f32 = |b: u8| b as f32 / 255.0;

    match hex.len() {
        3 => {
            let r = hex_nibble_pair(hex, 0, 0)?;
            let g = hex_nibble_pair(hex, 1, 1)?;
            let b = hex_nibble_pair(hex, 2, 2)?;
            Some((to_f32(r), to_f32(g), to_f32(b), 1.0))
        }
        4 => {
            let r = hex_nibble_pair(hex, 0, 0)?;
            let g = hex_nibble_pair(hex, 1, 1)?;
            let b = hex_nibble_pair(hex, 2, 2)?;
            let a = hex_nibble_pair(hex, 3, 3)?;
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
fn hex_nibble_pair(s: &str, pos: usize, _: usize) -> Option<u8> {
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

/// Parse a functional color string like `rgb(255, 0, 0)` or `hsl(120, 50%, 50%)`.
/// Returns (r, g, b, a) as f32 values 0.0–1.0, or None if invalid.
///
/// Accepted forms:
/// - `rgb(<n>, <n>, <n>)` — integer 0–255 or percentage 0%–100%
/// - `rgba(<n>, <n>, <n>, <alpha>)` — same, with alpha 0.0–1.0
/// - `hsl(<hue>, <sat>%, <light>%)` — hue in degrees, sat/light as percentages
/// - `hsla(<hue>, <sat>%, <light>%, <alpha>)`
pub fn parse_functional(text: &str) -> Option<(f32, f32, f32, f32)> {
    let text = text.trim();

    // Split off the function name and the parenthesised argument list.
    let paren_open = text.find('(')?;
    let func = &text[..paren_open];
    let rest = text[paren_open + 1..].strip_suffix(')')?;

    // Split args on commas; reject empty arg lists.
    let raw_args: Vec<&str> = rest.split(',').map(str::trim).collect();
    if raw_args.is_empty() || (raw_args.len() == 1 && raw_args[0].is_empty()) {
        return None;
    }

    match func {
        "rgb" | "rgba" => {
            if raw_args.len() != 3 && raw_args.len() != 4 {
                return None;
            }
            let r = parse_rgb_component(raw_args[0])?;
            let g = parse_rgb_component(raw_args[1])?;
            let b = parse_rgb_component(raw_args[2])?;
            let a = if raw_args.len() == 4 {
                parse_alpha(raw_args[3])?
            } else {
                1.0
            };
            Some((r, g, b, a))
        }
        "hsl" | "hsla" => {
            if raw_args.len() != 3 && raw_args.len() != 4 {
                return None;
            }
            let h = parse_number(raw_args[0])?;
            let s = parse_percent(raw_args[1])?;
            let l = parse_percent(raw_args[2])?;
            let a = if raw_args.len() == 4 {
                parse_alpha(raw_args[3])?
            } else {
                1.0
            };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            Some((r, g, b, a))
        }
        _ => None,
    }
}

/// Parse a single RGB component: integer 0–255 or percentage 0%–100%.
/// Returns a normalised f32 in 0.0–1.0.
fn parse_rgb_component(s: &str) -> Option<f32> {
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

/// Parse a percentage value like `50%`, returning the value divided by 100.
fn parse_percent(s: &str) -> Option<f32> {
    let inner = s.strip_suffix('%')?;
    let v: f32 = inner.trim().parse().ok()?;
    Some(v / 100.0)
}

/// Parse a bare number (used for hue degrees and alpha).
fn parse_number(s: &str) -> Option<f32> {
    s.parse().ok()
}

/// Parse an alpha value in 0.0–1.0.
fn parse_alpha(s: &str) -> Option<f32> {
    let v: f32 = parse_number(s)?;
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some(v)
}

/// Convert HSL to RGB.  All inputs/outputs are normalised to 0.0–1.0 except
/// `h`, which is in degrees (0–360, wrapping).
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    // Clamp s and l to [0, 1].
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = (h % 360.0) / 60.0;
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

    // ─── hex ────────────────────────────────────────────

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
        assert_eq!(parse_hex("#f008"), Some((1.0, 0.0, 0.0, 0x88 as f32 / 255.0)));
    }

    #[test]
    fn hex_case_insensitive() {
        assert_eq!(parse_hex("#FF0000"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex("#Ff0000"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hex_invalid() {
        assert_eq!(parse_hex("#gg0000"), None);
        assert_eq!(parse_hex("#ff"), None);    // 2 digits — invalid length
        assert_eq!(parse_hex("ff0000"), None); // missing #
        assert_eq!(parse_hex(""), None);
    }

    // ─── functional ─────────────────────────────────────

    #[test]
    fn rgb_integers() {
        assert_eq!(parse_functional("rgb(255, 0, 0)"), Some((1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_functional("rgb(0,128,255)"), Some((0.0, 128.0/255.0, 1.0, 1.0)));
    }

    #[test]
    fn rgba_with_alpha() {
        let result = parse_functional("rgba(255, 0, 0, 0.5)");
        assert!(result.is_some());
        let (r, _, _, a) = result.unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!((a - 0.5).abs() < 0.01);
    }

    #[test]
    fn rgb_percentages() {
        assert_eq!(parse_functional("rgb(100%, 0%, 0%)"), Some((1.0, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hsl_basic() {
        // hsl(0, 100%, 50%) = red
        let result = parse_functional("hsl(0, 100%, 50%)");
        assert!(result.is_some());
        let (r, g, b, _) = result.unwrap();
        assert!((r - 1.0).abs() < 0.02);
        assert!(g < 0.02);
        assert!(b < 0.02);
    }

    #[test]
    fn hsl_green() {
        // hsl(120, 100%, 50%) = lime green
        let result = parse_functional("hsl(120, 100%, 50%)");
        assert!(result.is_some());
        let (r, g, b, _) = result.unwrap();
        assert!(r < 0.02);
        assert!((g - 1.0).abs() < 0.02);
        assert!(b < 0.02);
    }

    #[test]
    fn hsla_with_alpha() {
        let result = parse_functional("hsla(0, 100%, 50%, 0.5)");
        assert!(result.is_some());
        let (_, _, _, a) = result.unwrap();
        assert!((a - 0.5).abs() < 0.01);
    }

    #[test]
    fn functional_whitespace_variations() {
        assert!(parse_functional("rgb( 255 , 0 , 0 )").is_some());
        assert!(parse_functional("rgb(255,0,0)").is_some());
    }

    #[test]
    fn functional_invalid() {
        assert_eq!(parse_functional("rgb()"), None);
        assert_eq!(parse_functional("rgb(a, b, c)"), None);
        assert_eq!(parse_functional("notafunction(1,2,3)"), None);
        assert_eq!(parse_functional(""), None);
    }
}
