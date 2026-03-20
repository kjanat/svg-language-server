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
pub fn parse_functional(_text: &str) -> Option<(f32, f32, f32, f32)> {
    todo!()
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
}
