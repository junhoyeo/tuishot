use std::{fmt, path::PathBuf, str::FromStr, time::Duration};
use ttyshot_render::FontSource;
use ttyshot_term::{Key, Size};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Format {
    Png,
    Svg,
    Txt,
    Json,
}
impl Format {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Txt => "txt",
            Self::Json => "json",
        }
    }
    #[must_use]
    pub fn from_extension(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "svg" => Some(Self::Svg),
            "txt" => Some(Self::Txt),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}
impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}
impl FromStr for Format {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_extension(value)
            .ok_or_else(|| format!("invalid format {value:?}; expected png, svg, txt, or json"))
    }
}

/// Parses `COLUMNSxROWS`.
///
/// # Errors
/// Returns an error for malformed, zero, or overflowing dimensions.
pub fn parse_size(value: &str) -> Result<Size, String> {
    let (columns, rows) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| format!("invalid size {value:?}; expected COLUMNSxROWS"))?;
    let columns = columns.parse::<u16>().map_err(|_| {
        format!("invalid size {value:?}; columns must be an integer from 1 to 65535")
    })?;
    let rows = rows
        .parse::<u16>()
        .map_err(|_| format!("invalid size {value:?}; rows must be an integer from 1 to 65535"))?;
    if columns == 0 || rows == 0 {
        return Err(format!(
            "invalid size {value:?}; dimensions must be non-zero"
        ));
    }
    Ok(Size::new(columns, rows))
}

/// Parses a positive integer duration with an `ms`, `s`, `m`, or `h` suffix.
///
/// # Errors
/// Returns an error for malformed, zero, unsupported, or overflowing values.
pub fn parse_duration(value: &str) -> Result<Duration, String> {
    if value.is_empty() {
        return Err("invalid duration: value is empty".to_owned());
    }
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    if number.is_empty() || number.len() + unit.len() != value.len() || unit.is_empty() {
        return Err(format!(
            "invalid duration {value:?}; expected an integer followed by ms, s, m, or h"
        ));
    }
    let amount = number
        .parse::<u64>()
        .map_err(|_| format!("invalid duration {value:?}; number is too large"))?;
    if amount == 0 {
        return Err(format!(
            "invalid duration {value:?}; duration must be greater than zero"
        ));
    }
    let millis = match unit {
        "ms" => Some(amount),
        "s" => amount.checked_mul(1_000),
        "m" => amount.checked_mul(60_000),
        "h" => amount.checked_mul(3_600_000),
        _ => None,
    }
    .ok_or_else(|| {
        format!("invalid duration {value:?}; expected an integer followed by ms, s, m, or h")
    })?;
    Ok(Duration::from_millis(millis))
}

/// Parses and validates one `KEY=VALUE` environment entry.
///
/// # Errors
/// Returns an error for a missing separator, empty key, or embedded NUL.
pub fn parse_env(value: &str) -> Result<(String, String), String> {
    let (key, val) = value
        .split_once('=')
        .ok_or_else(|| format!("invalid environment entry {value:?}; expected KEY=VALUE"))?;
    if key.is_empty() || key.contains('\0') || key.contains('=') {
        return Err(format!(
            "invalid environment key {key:?}; keys must be non-empty and contain neither '=' nor NUL"
        ));
    }
    if val.contains('\0') {
        return Err(format!(
            "invalid environment value for {key:?}; values must not contain NUL"
        ));
    }
    Ok((key.to_owned(), val.to_owned()))
}

/// Parses a named key, function key, control key, or one Unicode scalar.
///
/// # Errors
/// Returns an error when `value` does not match the supported key grammar.
pub fn parse_key(value: &str) -> Result<Key, String> {
    let lower = value.to_ascii_lowercase();
    let key = match lower.as_str() {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "page-up" => Key::PageUp,
        "pagedown" | "page-down" => Key::PageDown,
        "insert" => Key::Insert,
        "delete" => Key::Delete,
        _ if lower.starts_with("ctrl-") => {
            let suffix = &lower[5..];
            let mut chars = suffix.chars();
            match (chars.next(), chars.next()) {
                (Some(c @ 'a'..='z'), None) => {
                    let byte = u8::try_from(u32::from(c)).unwrap_or(b'a');
                    Key::Char(char::from(byte - b'a' + 1))
                }
                _ => return Err(key_error(value)),
            }
        }
        _ if lower.starts_with('f') => match lower[1..].parse::<u8>() {
            Ok(n @ 1..=12) => Key::Function(n),
            _ => return Err(key_error(value)),
        },
        _ => {
            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return Err(key_error(value)),
            }
        }
    };
    Ok(key)
}
fn key_error(value: &str) -> String {
    format!(
        "invalid key {value:?}; expected a named key, f1..f12, ctrl-a..ctrl-z, or one Unicode scalar"
    )
}

/// Parses a font path with an optional collection face index.
///
/// # Errors
/// Returns an error for empty/NUL paths, malformed indices, or duplicate indices.
pub fn parse_font(
    value: &str,
    paired_index: Option<u32>,
    base: Option<&std::path::Path>,
) -> Result<FontSource, String> {
    if value.contains('\0') {
        return Err("font path must not contain NUL".to_owned());
    }
    let (path, index) = if let Some((path, index)) = value.rsplit_once('#') {
        if !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) {
            let parsed = index
                .parse::<u32>()
                .map_err(|_| format!("invalid font face index in {value:?}"))?;
            if paired_index.is_some() {
                return Err(format!("font {value:?} specifies a face index twice"));
            }
            (path, parsed)
        } else {
            (value, paired_index.unwrap_or(0))
        }
    } else {
        (value, paired_index.unwrap_or(0))
    };
    if path.is_empty() {
        return Err("font path must not be empty".to_owned());
    }
    let path = PathBuf::from(path);
    let path = if path.is_relative() {
        base.map_or(path.clone(), |b| b.join(&path))
    } else {
        path
    };
    Ok(FontSource::collection(path, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_sizes_and_rejects_zero() {
        assert_eq!(parse_size("120x40").unwrap(), Size::new(120, 40));
        assert!(parse_size("0x2").unwrap_err().contains("non-zero"));
        assert!(parse_size("wide").is_err());
    }
    #[test]
    fn parses_exact_durations() {
        assert_eq!(parse_duration("300ms").unwrap(), Duration::from_millis(300));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        for bad in ["", "1", "0s", "1.5s", "1day"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn validates_environment() {
        assert_eq!(parse_env("A=b=c").unwrap(), ("A".into(), "b=c".into()));
        for bad in ["A", "=x", "A\0=x", "A=x\0"] {
            assert!(parse_env(bad).is_err());
        }
    }
    #[test]
    fn parses_all_key_forms() {
        assert_eq!(parse_key("ENTER").unwrap(), Key::Enter);
        assert_eq!(parse_key("f12").unwrap(), Key::Function(12));
        assert_eq!(parse_key("界").unwrap(), Key::Char('界'));
        assert_eq!(parse_key("ctrl-z").unwrap(), Key::Char('\u{1a}'));
        for bad in ["f0", "f13", "ctrl-aa", "hello"] {
            assert!(parse_key(bad).is_err());
        }
    }
    #[test]
    fn parses_font_indices() {
        assert_eq!(
            parse_font("font.ttc#2", None, None)
                .unwrap()
                .collection_index,
            2
        );
        assert!(parse_font("font#2", Some(1), None).is_err());
    }
}
