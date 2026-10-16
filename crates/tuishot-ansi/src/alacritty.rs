//! Native ANSI backend powered by Alacritty Terminal.

use crate::AnsiError;
use alacritty_terminal::{
    event::{EventListener, VoidListener},
    grid::Dimensions,
    term::cell::Flags,
    term::{Config as TermConfig, Term},
    vte::ansi::{Color as AlacrittyColor, CursorShape as AlacrittyCursorShape, NamedColor},
};
use tuishot_core::{
    Attributes, Blink, CellWidth, Color, Cursor, CursorShape, Grid, Rgb, Style, Underline,
};

#[derive(Clone, Copy)]
struct Size {
    columns: u16,
    rows: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.columns)
    }
}

/// Incremental ANSI/VT parser backed by Alacritty Terminal.
///
/// `feed` deliberately treats a bare LF as a logical newline by inserting CR
/// before it. Existing CRLF is unchanged, even when split across calls. This
/// makes static strings from tools such as Ink's `renderToString` start each
/// `\n` line at column zero while preserving explicit terminal CRLF input.
pub struct AnsiParser {
    terminal: Term<VoidListener>,
    processor: alacritty_terminal::vte::ansi::Processor,
    previous_was_cr: bool,
}

impl AnsiParser {
    /// Creates an empty fixed-size terminal.
    ///
    /// # Errors
    /// Returns [`AnsiError::EmptyDimensions`] when either dimension is zero.
    pub fn new(columns: u16, rows: u16) -> Result<Self, AnsiError> {
        if columns == 0 || rows == 0 {
            return Err(AnsiError::EmptyDimensions);
        }
        let size = Size { columns, rows };
        let config = TermConfig {
            scrolling_history: 0,
            kitty_keyboard: true,
            ..TermConfig::default()
        };
        Ok(Self {
            terminal: Term::new(config, &size, VoidListener),
            processor: alacritty_terminal::vte::ansi::Processor::new(),
            previous_was_cr: false,
        })
    }

    /// Feeds one byte chunk into the terminal.
    ///
    /// Parser state, UTF-8 state, and the CR/LF boundary state survive between
    /// calls, so callers may split input at arbitrary byte positions.
    pub fn feed(&mut self, input: impl AsRef<[u8]>) {
        let input = input.as_ref();
        let mut normalized = Vec::with_capacity(input.len());
        for &byte in input {
            if byte == b'\n' && !self.previous_was_cr {
                normalized.push(b'\r');
            }
            normalized.push(byte);
            self.previous_was_cr = byte == b'\r';
        }
        self.processor.advance(&mut self.terminal, &normalized);
    }

    /// Captures the current terminal state in the validated shared wire model.
    #[must_use]
    pub fn snapshot(&self) -> Grid {
        term_to_grid(&self.terminal)
    }
}

/// Parses a complete ANSI/VT byte string into a fixed-size grid.
///
/// Bare LF is normalized according to [`AnsiParser`]'s documented static-string
/// policy. No process or PTY is started.
///
/// # Errors
/// Returns an error when either terminal dimension is zero.
pub fn ansi_to_grid(input: impl AsRef<[u8]>, columns: u16, rows: u16) -> Result<Grid, AnsiError> {
    let mut parser = AnsiParser::new(columns, rows)?;
    parser.feed(input);
    Ok(parser.snapshot())
}

/// Converts an Alacritty terminal into the shared validated grid model.
///
/// This is the single conversion used by both direct ANSI parsing and the
/// native PTY driver, preventing the two capture paths from drifting apart.
///
/// # Panics
/// Panics when a caller constructed an Alacritty terminal with zero dimensions
/// or dimensions larger than the `u16` limits of `tuishot-core::Grid`.
#[must_use]
pub fn term_to_grid<T: EventListener>(term: &Term<T>) -> Grid {
    let content = term.renderable_content();
    let cursor = content.cursor;
    let display_offset = content.display_offset;
    let mut grid = Grid::new(
        u16::try_from(term.columns()).expect("terminal columns fit u16"),
        u16::try_from(term.screen_lines()).expect("terminal rows fit u16"),
    )
    .expect("validated terminal dimensions produce a grid");

    for indexed in content.display_iter {
        let y = indexed.point.line.0 + i32::try_from(display_offset).unwrap_or(i32::MAX);
        let Ok(y) = u16::try_from(y) else {
            continue;
        };
        let Ok(x) = u16::try_from(indexed.point.column.0) else {
            continue;
        };
        if indexed.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let mut symbol = indexed.c.to_string();
        if let Some(zerowidth) = indexed.zerowidth() {
            symbol.extend(zerowidth);
        }
        let width = if indexed.flags.contains(Flags::WIDE_CHAR) {
            CellWidth::Wide
        } else {
            CellWidth::Single
        };
        let style = cell_style(&indexed);
        if grid.put(x, y, symbol, width, style).is_err() {
            let _ = grid.put(x, y, " ", CellWidth::Single, style);
        }
    }

    let cursor_y = cursor.point.line.0 + i32::try_from(display_offset).unwrap_or(i32::MAX);
    if let (Ok(x), Ok(y)) = (
        u16::try_from(cursor.point.column.0),
        u16::try_from(cursor_y),
    ) && x < grid.columns()
        && y < grid.rows()
    {
        let (shape, visible) = match cursor.shape {
            AlacrittyCursorShape::Block | AlacrittyCursorShape::HollowBlock => {
                (CursorShape::Block, true)
            }
            AlacrittyCursorShape::Underline => (CursorShape::Underline, true),
            AlacrittyCursorShape::Beam => (CursorShape::Bar, true),
            AlacrittyCursorShape::Hidden => (CursorShape::Block, false),
        };
        grid.cursor = Some(Cursor {
            x,
            y,
            shape,
            visible,
        });
    }
    grid
}

fn cell_style(cell: &alacritty_terminal::term::cell::Cell) -> Style {
    let flags = cell.flags;
    let underline = if flags.contains(Flags::DOUBLE_UNDERLINE) {
        Underline::Double
    } else if flags.contains(Flags::UNDERCURL) {
        Underline::Curly
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        Underline::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        Underline::Dashed
    } else if flags.contains(Flags::UNDERLINE) {
        Underline::Single
    } else {
        Underline::None
    };
    Style {
        foreground: convert_color(cell.fg),
        background: convert_color(cell.bg),
        underline_color: cell.underline_color().map_or(Color::Default, convert_color),
        attributes: Attributes {
            bold: flags.contains(Flags::BOLD),
            dim: flags.contains(Flags::DIM),
            italic: flags.contains(Flags::ITALIC),
            underline,
            blink: Blink::None,
            reverse: flags.contains(Flags::INVERSE),
            hidden: flags.contains(Flags::HIDDEN),
            strikethrough: flags.contains(Flags::STRIKEOUT),
            ..Attributes::default()
        },
    }
}

fn convert_color(color: AlacrittyColor) -> Color {
    match color {
        AlacrittyColor::Spec(color) => Color::Rgb(Rgb::new(color.r, color.g, color.b)),
        AlacrittyColor::Indexed(index) => Color::Indexed(index),
        AlacrittyColor::Named(
            named @ (NamedColor::Black
            | NamedColor::Red
            | NamedColor::Green
            | NamedColor::Yellow
            | NamedColor::Blue
            | NamedColor::Magenta
            | NamedColor::Cyan
            | NamedColor::White
            | NamedColor::BrightBlack
            | NamedColor::BrightRed
            | NamedColor::BrightGreen
            | NamedColor::BrightYellow
            | NamedColor::BrightBlue
            | NamedColor::BrightMagenta
            | NamedColor::BrightCyan
            | NamedColor::BrightWhite),
        ) => Color::Indexed(named_color_index(named)),
        AlacrittyColor::Named(_) => Color::Default,
    }
}

fn named_color_index(color: NamedColor) -> u8 {
    match color {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => unreachable!("only ANSI named colors are converted to palette indices"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuishot_core::{CellWidth, Color, Rgb, Underline};

    #[test]
    fn bare_lf_starts_at_column_zero_and_crlf_is_preserved() {
        let grid = ansi_to_grid("ab\ncd\r\nef", 6, 4).unwrap();
        assert_eq!(grid.get(0, 0).unwrap().symbol, "a");
        assert_eq!(grid.get(1, 0).unwrap().symbol, "b");
        assert_eq!(grid.get(0, 1).unwrap().symbol, "c");
        assert_eq!(grid.get(1, 1).unwrap().symbol, "d");
        assert_eq!(grid.get(0, 2).unwrap().symbol, "e");
        assert_eq!(grid.get(1, 2).unwrap().symbol, "f");
    }

    #[test]
    fn arbitrary_split_input_matches_one_shot_including_split_crlf_and_utf8() {
        let input = "A\r\n\x1b[38;5;196m界\x1b[0m\nZ".as_bytes();
        let expected = ansi_to_grid(input, 8, 4).unwrap();
        for split in 0..=input.len() {
            let mut parser = AnsiParser::new(8, 4).unwrap();
            parser.feed(&input[..split]);
            parser.feed(&input[split..]);
            assert_eq!(parser.snapshot(), expected, "split at byte {split}");
        }
    }

    #[test]
    fn sgr_indexed_rgb_and_underline_color_are_preserved() {
        let grid = ansi_to_grid("\x1b[1;4:3;38;5;196;48;2;1;2;3;58;2;4;5;6mX", 4, 2).unwrap();
        let style = grid.get(0, 0).unwrap().style;
        assert!(style.attributes.bold);
        assert_eq!(style.attributes.underline, Underline::Curly);
        assert_eq!(style.foreground, Color::Indexed(196));
        assert_eq!(style.background, Color::Rgb(Rgb::new(1, 2, 3)));
        assert_eq!(style.underline_color, Color::Rgb(Rgb::new(4, 5, 6)));
    }

    #[test]
    fn wide_cells_have_one_validated_continuation() {
        let grid = ansi_to_grid("A界B", 6, 2).unwrap();
        assert_eq!(grid.get(1, 0).unwrap().symbol, "界");
        assert_eq!(grid.get(1, 0).unwrap().width, CellWidth::Wide);
        assert_eq!(grid.get(2, 0).unwrap().symbol, "");
        assert_eq!(grid.get(2, 0).unwrap().width, CellWidth::Continuation);
        assert_eq!(grid.get(3, 0).unwrap().symbol, "B");
    }

    #[test]
    fn cursor_movement_and_erase_update_the_grid() {
        let grid = ansi_to_grid("abcdef\x1b[3D\x1b[KZ\x1b[2;3HQ", 8, 3).unwrap();
        assert_eq!(grid.get(0, 0).unwrap().symbol, "a");
        assert_eq!(grid.get(2, 0).unwrap().symbol, "c");
        assert_eq!(grid.get(3, 0).unwrap().symbol, "Z");
        assert_eq!(grid.get(4, 0).unwrap().symbol, " ");
        assert_eq!(grid.get(2, 1).unwrap().symbol, "Q");
        let cursor = grid.cursor.unwrap();
        assert_eq!((cursor.x, cursor.y), (3, 1));
    }

    #[test]
    fn cursor_position_shape_and_visibility_are_captured() {
        let visible = ansi_to_grid("\x1b[2;4H\x1b[5 q", 8, 3).unwrap();
        let cursor = visible.cursor.unwrap();
        assert_eq!((cursor.x, cursor.y), (3, 1));
        assert_eq!(cursor.shape, CursorShape::Bar);
        assert!(cursor.visible);

        let hidden = ansi_to_grid("\x1b[?25l", 8, 3).unwrap().cursor.unwrap();
        assert!(!hidden.visible);
    }

    #[test]
    fn zero_dimensions_are_rejected() {
        assert_eq!(
            AnsiParser::new(0, 1).err(),
            Some(AnsiError::EmptyDimensions)
        );
        assert_eq!(
            AnsiParser::new(1, 0).err(),
            Some(AnsiError::EmptyDimensions)
        );
    }
}
