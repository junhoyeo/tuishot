//! Wasm-safe ANSI backend built on the raw [`vte`] parser.
//!
//! This module owns a fixed-size offline screen. It never starts a process,
//! opens a TTY, reads the filesystem, or writes protocol query replies.

use crate::AnsiError;
use ::vte::{Params, Perform};
use tuishot_core::{
    Blink, Cell, CellWidth, Color, Cursor, CursorShape, Frame, Grid, Ideogram, Rgb, Style,
    Underline,
};
use unicode_width::UnicodeWidthChar;

/// Incremental ANSI/VT parser backed by the wasm-safe raw `vte` state machine.
pub struct AnsiParser {
    parser: ::vte::Parser,
    terminal: Terminal,
    previous_was_cr: bool,
}

impl AnsiParser {
    /// Creates an empty fixed-size offline terminal.
    ///
    /// # Errors
    /// Returns [`AnsiError::EmptyDimensions`] when either dimension is zero.
    pub fn new(columns: u16, rows: u16) -> Result<Self, AnsiError> {
        if columns == 0 || rows == 0 {
            return Err(AnsiError::EmptyDimensions);
        }
        Ok(Self {
            parser: ::vte::Parser::new(),
            terminal: Terminal::new(columns, rows),
            previous_was_cr: false,
        })
    }

    /// Feeds an arbitrary byte chunk while retaining UTF-8 and escape state.
    ///
    /// Bare LF is normalized to CRLF for static TUI/Ink strings. A CRLF pair
    /// split between calls remains one pair.
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
        self.parser.advance(&mut self.terminal, &normalized);
    }

    /// Captures the state through `tuishot-core`'s checked grid mutation API.
    #[must_use]
    pub fn snapshot(&self) -> Grid {
        self.terminal.snapshot()
    }
}

/// Parses a complete byte string into a fixed-size grid.
///
/// # Errors
/// Returns an error when either dimension is zero.
pub fn ansi_to_grid(input: impl AsRef<[u8]>, columns: u16, rows: u16) -> Result<Grid, AnsiError> {
    let mut parser = AnsiParser::new(columns, rows)?;
    parser.feed(input);
    Ok(parser.snapshot())
}

#[derive(Clone, Copy)]
struct SavedCursor {
    x: usize,
    y: usize,
    style: Style,
    wrap_pending: bool,
    g0_special: bool,
    g1_special: bool,
}

#[derive(Clone)]
struct Screen {
    cells: Vec<Cell>,
    x: usize,
    y: usize,
    saved: SavedCursor,
    wrap_pending: bool,
}

impl Screen {
    fn new(columns: usize, rows: usize) -> Self {
        Self {
            cells: vec![Cell::default(); columns * rows],
            x: 0,
            y: 0,
            saved: SavedCursor {
                x: 0,
                y: 0,
                style: Style::default(),
                wrap_pending: false,
                g0_special: false,
                g1_special: false,
            },
            wrap_pending: false,
        }
    }
}

#[allow(clippy::struct_excessive_bools)] // Each flag is an independent terminal mode.
struct Terminal {
    columns: usize,
    rows: usize,
    primary: Screen,
    alternate: Screen,
    alternate_active: bool,
    style: Style,
    cursor_shape: Option<CursorShape>,
    cursor_visible: bool,
    top_margin: usize,
    bottom_margin: usize,
    origin_mode: bool,
    auto_wrap: bool,
    insert_mode: bool,
    tab_stops: Vec<bool>,
    g0_special: bool,
    g1_special: bool,
    use_g1: bool,
    preceding_character: Option<char>,
}

impl Terminal {
    fn new(columns: u16, rows: u16) -> Self {
        let columns = usize::from(columns);
        let rows = usize::from(rows);
        // Alacritty's `TabStops::new` marks every multiple of eight, including
        // column zero, which CBT uses as its leftmost stop.
        let tab_stops = (0..columns).map(|column| column % 8 == 0).collect();
        Self {
            columns,
            rows,
            primary: Screen::new(columns, rows),
            alternate: Screen::new(columns, rows),
            alternate_active: false,
            style: Style::default(),
            cursor_shape: None,
            cursor_visible: true,
            top_margin: 0,
            bottom_margin: rows - 1,
            origin_mode: false,
            auto_wrap: true,
            insert_mode: false,
            tab_stops,
            g0_special: false,
            g1_special: false,
            use_g1: false,
            preceding_character: None,
        }
    }

    fn screen(&self) -> &Screen {
        if self.alternate_active {
            &self.alternate
        } else {
            &self.primary
        }
    }

    fn screen_mut(&mut self) -> &mut Screen {
        if self.alternate_active {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    fn snapshot(&self) -> Grid {
        let mut grid = Grid::new(coordinate(self.columns), coordinate(self.rows))
            .expect("terminal dimensions were validated");
        let screen = self.screen();
        for y in 0..self.rows {
            for x in 0..self.columns {
                let cell = &screen.cells[self.index(x, y)];
                match cell.width {
                    CellWidth::Single => {
                        // Coordinates are known-valid and single cells cannot
                        // violate the wide-pair invariant.
                        let _ = grid.put(
                            coordinate(x),
                            coordinate(y),
                            cell.symbol.clone(),
                            CellWidth::Single,
                            cell.style,
                        );
                    }
                    CellWidth::Wide if x + 1 < self.columns => {
                        let trailing = &screen.cells[self.index(x + 1, y)];
                        if trailing.width == CellWidth::Continuation
                            && trailing.symbol.is_empty()
                            && trailing.style == cell.style
                        {
                            let _ = grid.put(
                                coordinate(x),
                                coordinate(y),
                                cell.symbol.clone(),
                                CellWidth::Wide,
                                cell.style,
                            );
                        }
                    }
                    CellWidth::Wide | CellWidth::Continuation => {}
                }
            }
        }
        // A cursor that lands on the trailing half of a wide cell is reported
        // on the leading half, matching Alacritty's renderable cursor.
        let mut x = screen.x.min(self.columns - 1);
        let y = screen.y.min(self.rows - 1);
        if x > 0 && screen.cells[self.index(x, y)].width == CellWidth::Continuation {
            x -= 1;
        }
        // Alacritty reports a hidden cursor as its own shape, so an explicit
        // DECSCUSR shape is not visible while the cursor is hidden.
        let shape = if self.cursor_visible {
            self.cursor_shape.unwrap_or_default()
        } else {
            CursorShape::default()
        };
        grid.cursor = Some(Cursor {
            x: coordinate(x),
            y: coordinate(y),
            shape,
            visible: self.cursor_visible,
        });
        grid
    }

    const fn index(&self, x: usize, y: usize) -> usize {
        y * self.columns + x
    }

    fn cursor(&self) -> (usize, usize) {
        let screen = self.screen();
        (screen.x, screen.y)
    }

    fn set_cursor(&mut self, x: usize, y: usize) {
        let x = x.min(self.columns - 1);
        let y = y.min(self.rows - 1);
        let screen = self.screen_mut();
        screen.x = x;
        screen.y = y;
        screen.wrap_pending = false;
    }

    /// Alacritty's `goto`: the requested row is relative to the scrolling
    /// region in origin mode, and both coordinates are clamped. `row` may be
    /// negative because relative moves pass an unclamped line.
    fn goto(&mut self, row: isize, column: usize) {
        let (offset, max_y) = if self.origin_mode {
            (self.top_margin, self.bottom_margin)
        } else {
            (0, self.rows - 1)
        };
        let y = row
            .saturating_add_unsigned(offset)
            .clamp(0, isize::try_from(max_y).unwrap_or(isize::MAX));
        self.set_cursor(column, usize::try_from(y).unwrap_or(0));
    }

    fn set_cursor_position(&mut self, row: usize, column: usize) {
        self.goto(isize::try_from(row).unwrap_or(isize::MAX), column);
    }

    /// DECSC. Alacritty clones the whole cursor, which also carries the
    /// designated charsets and the pending-wrap flag.
    fn save_cursor(&mut self) {
        let style = self.style;
        let g0_special = self.g0_special;
        let g1_special = self.g1_special;
        let screen = self.screen_mut();
        screen.saved = SavedCursor {
            x: screen.x,
            y: screen.y,
            style,
            wrap_pending: screen.wrap_pending,
            g0_special,
            g1_special,
        };
    }

    fn restore_cursor(&mut self) {
        let saved = self.screen().saved;
        self.style = saved.style;
        self.g0_special = saved.g0_special;
        self.g1_special = saved.g1_special;
        let columns = self.columns;
        let rows = self.rows;
        let screen = self.screen_mut();
        screen.x = saved.x.min(columns - 1);
        screen.y = saved.y.min(rows - 1);
        screen.wrap_pending = saved.wrap_pending;
    }

    /// RIS. The repeat character lives in the parser state in both
    /// implementations, so a full reset does not clear it.
    fn reset(&mut self) {
        let columns = coordinate(self.columns);
        let rows = coordinate(self.rows);
        let preceding_character = self.preceding_character;
        *self = Self::new(columns, rows);
        self.preceding_character = preceding_character;
    }

    /// Background-color erase. Only the background survives a clear, matching
    /// xterm's BCE rule and Alacritty's cell reset template.
    fn blank(style: Style) -> Cell {
        Cell::blank(Style {
            background: style.background,
            ..Style::default()
        })
    }

    fn clear_footprint(&mut self, x: usize, y: usize, style: Style) {
        let index = self.index(x, y);
        let width = self.screen().cells[index].width;
        let columns = self.columns;
        let screen = self.screen_mut();
        match width {
            CellWidth::Single => screen.cells[index] = Self::blank(style),
            CellWidth::Wide => {
                screen.cells[index] = Self::blank(style);
                if x + 1 < columns {
                    screen.cells[index + 1] = Self::blank(style);
                }
            }
            CellWidth::Continuation => {
                screen.cells[index] = Self::blank(style);
                if x > 0 {
                    screen.cells[index - 1] = Self::blank(style);
                }
            }
        }
    }

    /// Breaks the wide pair that covers `(x, y)` before a write.
    ///
    /// Alacritty blanks the character of the partner cell but keeps its style,
    /// unlike an erase, which applies background-color erase.
    fn split_wide_pair(&mut self, x: usize, y: usize) {
        let index = self.index(x, y);
        let width = self.screen().cells[index].width;
        let columns = self.columns;
        let partner = match width {
            CellWidth::Wide if x + 1 < columns => index + 1,
            CellWidth::Continuation if x > 0 => index - 1,
            CellWidth::Single | CellWidth::Wide | CellWidth::Continuation => return,
        };
        let screen = self.screen_mut();
        for cell in [partner, index] {
            " ".clone_into(&mut screen.cells[cell].symbol);
            screen.cells[cell].width = CellWidth::Single;
        }
    }

    fn repair_row(&mut self, y: usize) {
        let start = self.index(0, y);
        let end = start + self.columns;
        let row = &mut self.screen_mut().cells[start..end];
        let mut x = 0;
        while x < row.len() {
            match row[x].width {
                CellWidth::Single => x += 1,
                CellWidth::Wide => {
                    let valid = x + 1 < row.len()
                        && row[x + 1].width == CellWidth::Continuation
                        && row[x + 1].symbol.is_empty()
                        && row[x + 1].style == row[x].style;
                    if valid {
                        x += 2;
                    } else {
                        let style = row[x].style;
                        row[x] = Self::blank(style);
                        x += 1;
                    }
                }
                CellWidth::Continuation => {
                    let valid = x > 0
                        && row[x - 1].width == CellWidth::Wide
                        && row[x].symbol.is_empty()
                        && row[x - 1].style == row[x].style;
                    if !valid {
                        let style = row[x].style;
                        row[x] = Self::blank(style);
                    }
                    x += 1;
                }
            }
        }
    }

    fn erase_range(&mut self, y: usize, start: usize, end_inclusive: usize) {
        if start > end_inclusive || start >= self.columns {
            return;
        }
        let end = end_inclusive.min(self.columns - 1);
        let style = self.style;
        for x in start..=end {
            self.clear_footprint(x, y, style);
        }
        for x in start..=end {
            let index = self.index(x, y);
            self.screen_mut().cells[index] = Self::blank(style);
        }
        self.repair_row(y);
    }

    fn erase_line(&mut self, mode: u16) {
        let (x, y) = self.cursor();
        match mode {
            // Alacritty ignores an erase-right with a pending wrap.
            0 if self.screen().wrap_pending => {}
            0 => self.erase_range(y, x, self.columns - 1),
            1 => self.erase_range(y, 0, x),
            2 => self.erase_range(y, 0, self.columns - 1),
            _ => {}
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let (x, y) = self.cursor();
        match mode {
            0 => {
                self.erase_range(y, x, self.columns - 1);
                for row in y + 1..self.rows {
                    self.erase_range(row, 0, self.columns - 1);
                }
            }
            1 => {
                // Alacritty's `ClearMode::Above` only resets the region above
                // the cursor when the cursor line is greater than one. Both
                // backends must agree, so the quirk is reproduced exactly.
                if y > 1 {
                    for row in 0..y {
                        self.erase_range(row, 0, self.columns - 1);
                    }
                }
                self.erase_range(y, 0, x);
            }
            2 => {
                for row in 0..self.rows {
                    self.erase_range(row, 0, self.columns - 1);
                }
            }
            // Mode 3 erases scrollback. This terminal keeps no history, so it
            // has no observable effect, exactly as in the native backend.
            _ => {}
        }
    }

    fn erase_characters(&mut self, count: usize) {
        let (x, y) = self.cursor();
        self.erase_range(y, x, x.saturating_add(count.saturating_sub(1)));
    }

    fn copy_row(&mut self, source: usize, target: usize) {
        let source_start = self.index(0, source);
        let target_start = self.index(0, target);
        let row = self.screen().cells[source_start..source_start + self.columns].to_vec();
        let columns = self.columns;
        self.screen_mut().cells[target_start..target_start + columns].clone_from_slice(&row);
    }

    fn blank_row(&mut self, y: usize) {
        let style = self.style;
        let start = self.index(0, y);
        let columns = self.columns;
        self.screen_mut().cells[start..start + columns].fill(Self::blank(style));
    }

    fn scroll_up(&mut self, top: usize, bottom: usize, count: usize) {
        if top > bottom || bottom >= self.rows {
            return;
        }
        let count = count.max(1).min(bottom - top + 1);
        if count < bottom - top + 1 {
            for row in top..=bottom - count {
                self.copy_row(row + count, row);
            }
        }
        for row in bottom + 1 - count..=bottom {
            self.blank_row(row);
        }
    }

    fn scroll_down(&mut self, top: usize, bottom: usize, count: usize) {
        if top > bottom || bottom >= self.rows {
            return;
        }
        let count = count.max(1).min(bottom - top + 1);
        for row in (top + count..=bottom).rev() {
            self.copy_row(row - count, row);
        }
        for row in top..top + count {
            self.blank_row(row);
        }
    }

    fn linefeed(&mut self) {
        let (x, y) = self.cursor();
        if y == self.bottom_margin {
            self.scroll_up(self.top_margin, self.bottom_margin, 1);
            self.set_cursor(x, y);
        } else {
            self.set_cursor(x, (y + 1).min(self.rows - 1));
        }
    }

    fn reverse_index(&mut self) {
        let (x, y) = self.cursor();
        if y == self.top_margin {
            self.scroll_down(self.top_margin, self.bottom_margin, 1);
            self.set_cursor(x, y);
        } else {
            self.set_cursor(x, y.saturating_sub(1));
        }
    }

    /// ICH. This mirrors Alacritty's `insert_blank`, including its swap-based
    /// shift and its clamp against the remaining columns.
    fn insert_characters(&mut self, count: usize) {
        let (x, y) = self.cursor();
        let columns = self.columns;
        let count = count.max(1).min(columns - x);
        let destination = x + count;
        let cells = columns - destination;
        let start = self.index(0, y);
        let style = self.style;
        let row = &mut self.screen_mut().cells[start..start + columns];
        for offset in (0..cells).rev() {
            row.swap(destination + offset, x + offset);
        }
        row[x..destination].fill(Self::blank(style));
        self.repair_row(y);
    }

    /// DCH. This mirrors Alacritty's `delete_chars`, whose count is clamped
    /// against the full width, so a count that reaches past the last column
    /// blanks the whole row.
    fn delete_characters(&mut self, count: usize) {
        let (x, y) = self.cursor();
        let columns = self.columns;
        let count = count.max(1).min(columns);
        let end = (x + count).min(columns - 1);
        let cells = columns - end;
        let start = self.index(0, y);
        let style = self.style;
        let row = &mut self.screen_mut().cells[start..start + columns];
        for offset in 0..cells {
            row.swap(x + offset, end + offset);
        }
        row[columns - count..].fill(Self::blank(style));
        self.repair_row(y);
    }

    fn insert_lines(&mut self, count: usize) {
        let (_, y) = self.cursor();
        if (self.top_margin..=self.bottom_margin).contains(&y) {
            self.scroll_down(y, self.bottom_margin, count);
        }
    }

    fn delete_lines(&mut self, count: usize) {
        let (_, y) = self.cursor();
        if (self.top_margin..=self.bottom_margin).contains(&y) {
            self.scroll_up(y, self.bottom_margin, count);
        }
    }

    /// Attaches a zero-width character to the cell the cursor last wrote.
    ///
    /// Alacritty saturates at column zero instead of moving to the previous
    /// row, and attaches to the leading half of a wide pair. A combining mark
    /// with no preceding glyph therefore decorates the blank origin cell.
    fn append_combining(&mut self, character: char) {
        let screen = self.screen();
        let mut x = screen.x;
        let y = screen.y;
        if !screen.wrap_pending {
            x = x.saturating_sub(1);
        }
        let mut index = self.index(x, y);
        if self.screen().cells[index].width == CellWidth::Continuation && x > 0 {
            index -= 1;
        }
        if self.screen().cells[index].width != CellWidth::Continuation {
            self.screen_mut().cells[index].symbol.push(character);
        }
    }

    fn map_character(&self, character: char) -> char {
        let special = if self.use_g1 {
            self.g1_special
        } else {
            self.g0_special
        };
        if !special {
            return character;
        }
        match character {
            '_' => ' ',
            '`' => '◆',
            'a' => '▒',
            'b' => '\u{2409}',
            'c' => '\u{240c}',
            'd' => '\u{240d}',
            'e' => '\u{240a}',
            'f' => '°',
            'h' => '\u{2424}',
            'i' => '\u{240b}',
            'g' => '±',
            'j' => '┘',
            'k' => '┐',
            'l' => '┌',
            'm' => '└',
            'n' => '┼',
            'o' => '⎺',
            'p' => '⎻',
            'q' => '─',
            'r' => '⎼',
            's' => '⎽',
            't' => '├',
            'u' => '┤',
            'v' => '┴',
            'w' => '┬',
            'x' => '│',
            'y' => '≤',
            'z' => '≥',
            '{' => 'π',
            '|' => '≠',
            '}' => '£',
            '~' => '·',
            _ => character,
        }
    }

    /// DECCOLM side effects shared with Alacritty's `deccolm`: reset the
    /// scrolling region, then clear the active screen.
    fn column_mode(&mut self) {
        self.set_scroll_region(1, None);
        for row in 0..self.rows {
            self.blank_row(row);
        }
    }

    /// DECALN. Alacritty fills every cell with a default-styled `E` and leaves
    /// the cursor where it is.
    fn screen_alignment_pattern(&mut self) {
        let template = Cell::single("E", Style::default());
        self.screen_mut().cells.fill(template);
    }

    fn print_character(&mut self, character: char) {
        self.preceding_character = Some(character);
        let character = self.map_character(character);
        let width = UnicodeWidthChar::width(character).unwrap_or(0).min(2);
        if width == 0 {
            self.append_combining(character);
            return;
        }

        if self.screen().wrap_pending {
            if self.auto_wrap {
                let y = self.screen().y;
                self.set_cursor(0, y);
                self.linefeed();
            } else {
                self.screen_mut().wrap_pending = false;
            }
        }

        let (mut x, y) = self.cursor();
        if width == 2 && self.columns < 2 {
            // A two-cell glyph can never fit a one-column screen. Alacritty
            // requires at least two columns and panics here; this backend
            // drops the glyph and arms the pending wrap instead.
            self.screen_mut().wrap_pending = true;
            return;
        }
        if width == 2 && x + 1 >= self.columns {
            if self.auto_wrap {
                // Alacritty writes a styled leading spacer in the cell the
                // wide glyph could not occupy, then wraps.
                self.write_cell(x, y, " ".to_owned(), CellWidth::Single);
                self.set_cursor(0, y);
                self.linefeed();
                x = 0;
            } else {
                // Without autowrap Alacritty only arms the pending wrap and
                // drops the glyph, leaving the cell unchanged.
                self.screen_mut().wrap_pending = true;
                return;
            }
        }
        let y = self.screen().y;
        if self.insert_mode {
            self.insert_characters(width);
        }
        self.write_cell(
            x,
            y,
            character.to_string(),
            if width == 2 {
                CellWidth::Wide
            } else {
                CellWidth::Single
            },
        );
        if x + width >= self.columns {
            let columns = self.columns;
            let screen = self.screen_mut();
            screen.x = columns - 1;
            screen.wrap_pending = true;
        } else {
            let screen = self.screen_mut();
            screen.x = x + width;
            screen.wrap_pending = false;
        }
    }

    /// Writes one grapheme, clearing any wide pair it overlaps.
    ///
    /// Alacritty's `write_at_cursor` only drops the `WIDE_CHAR` flag on the
    /// leading cell and replaces its character with a space; it does not clear
    /// the trailing spacer's cell contents or style. The trailing spacer is
    /// already a blank space, so the observable effect is that the previous
    /// style survives in that trailing cell.
    fn write_cell(&mut self, x: usize, y: usize, symbol: String, width: CellWidth) {
        let style = self.style;
        self.split_wide_pair(x, y);
        if width == CellWidth::Wide {
            self.split_wide_pair(x + 1, y);
        }
        let index = self.index(x, y);
        self.screen_mut().cells[index] = Cell {
            symbol,
            width,
            style,
        };
        if width == CellWidth::Wide {
            self.screen_mut().cells[index + 1] = Cell {
                symbol: String::new(),
                width: CellWidth::Continuation,
                style,
            };
        }
    }

    /// C0 HT. Alacritty's `put_tab` writes the mapped tab character into an
    /// otherwise blank origin cell, and a tab with a pending wrap only breaks
    /// the line.
    fn put_tab(&mut self, count: usize) {
        if self.screen().wrap_pending {
            if self.auto_wrap {
                let y = self.screen().y;
                self.set_cursor(0, y);
                self.linefeed();
            }
            return;
        }
        for _ in 0..count.max(1) {
            let (x, y) = self.cursor();
            let index = self.index(x, y);
            let tab = self.map_character('\t');
            let cell = &mut self.screen_mut().cells[index];
            // Alacritty replaces only the base character, so any combining
            // marks already attached to a blank cell survive the tab.
            if cell.width == CellWidth::Single && cell.symbol.starts_with(' ') {
                cell.symbol
                    .replace_range(..' '.len_utf8(), tab.encode_utf8(&mut [0; 4]));
            }
            self.move_forward_tabs(1);
        }
    }

    /// CHT. Pure movement: unlike HT it never writes a tab character.
    fn move_forward_tabs(&mut self, count: usize) {
        for _ in 0..count.max(1) {
            let (x, y) = self.cursor();
            let next = (x + 1..self.columns)
                .find(|candidate| self.tab_stops[*candidate])
                .unwrap_or(self.columns - 1);
            self.set_cursor(next, y);
        }
    }

    /// CBT. Alacritty leaves the column unchanged when no earlier stop
    /// exists, instead of snapping to column zero.
    fn back_tab(&mut self, count: usize) {
        for _ in 0..count.max(1) {
            let (x, y) = self.cursor();
            if x == 0 {
                break;
            }
            let previous = (0..x)
                .rev()
                .find(|candidate| self.tab_stops[*candidate])
                .unwrap_or(x);
            self.set_cursor(previous, y);
        }
    }

    /// DECSTBM with Alacritty's exact clamping. `top` and `bottom` are
    /// one-based; an out-of-range bottom is clamped instead of rejected, an
    /// inverted or empty region is ignored, and a valid region homes the
    /// cursor through the origin-aware cursor move.
    fn set_scroll_region(&mut self, top: usize, bottom: Option<usize>) {
        let bottom = bottom.unwrap_or(self.rows);
        if top >= bottom {
            return;
        }
        self.top_margin = top.saturating_sub(1).min(self.rows);
        self.bottom_margin = bottom.min(self.rows).saturating_sub(1);
        self.set_cursor_position(0, 0);
    }

    /// Enters or leaves the alternate screen exactly like Alacritty's
    /// `swap_alt`: the alternate buffer is cleared on entry, the cursor keeps
    /// its position, and leaving restores the primary cursor and style.
    fn set_alternate(&mut self, enabled: bool) {
        if enabled == self.alternate_active {
            return;
        }
        if enabled {
            self.save_cursor();
            let saved = self.primary.saved;
            self.alternate = Screen::new(self.columns, self.rows);
            // The alternate buffer is reset through the active cursor
            // template, so background-color erase applies here too.
            self.alternate.cells.fill(Self::blank(self.style));
            self.alternate.x = saved.x;
            self.alternate.y = saved.y;
            self.alternate.wrap_pending = saved.wrap_pending;
            self.alternate_active = true;
        } else {
            self.alternate_active = false;
            self.style = self.primary.saved.style;
        }
    }

    fn set_private_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            6 => {
                self.origin_mode = enabled;
                // Alacritty homes the cursor when origin mode is set and
                // leaves it untouched when the mode is reset.
                if enabled {
                    self.set_cursor_position(0, 0);
                }
            }
            7 => self.auto_wrap = enabled,
            25 => self.cursor_visible = enabled,
            // DECCOLM cannot resize a fixed-size offline screen, but both
            // backends keep its documented side effects: the scrolling region
            // is reset, the screen is erased, and the cursor homes.
            3 => self.column_mode(),
            1049 => self.set_alternate(enabled),
            // Alacritty 0.26 implements only mode 1049. Modes 47, 1047, and
            // 1048 stay unhandled so both backends agree.
            _ => {}
        }
    }

    fn set_mode(&mut self, mode: u16, enabled: bool) {
        if mode == 4 {
            self.insert_mode = enabled;
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let groups = params.iter().collect::<Vec<_>>();
        if groups.is_empty() {
            self.style = Style::default();
            return;
        }
        let mut index = 0;
        while index < groups.len() {
            let group = groups[index];
            let code = group.first().copied().unwrap_or(0);
            match code {
                0 => self.style = Style::default(),
                1 => self.style.attributes.bold = true,
                2 => self.style.attributes.dim = true,
                3 => self.style.attributes.italic = true,
                4 => {
                    self.style.attributes.underline = match group.get(1).copied().unwrap_or(1) {
                        0 => Underline::None,
                        2 => Underline::Double,
                        3 => Underline::Curly,
                        4 => Underline::Dotted,
                        5 => Underline::Dashed,
                        _ => Underline::Single,
                    };
                }
                5 => self.style.attributes.blink = Blink::Slow,
                6 => self.style.attributes.blink = Blink::Rapid,
                7 => self.style.attributes.reverse = true,
                8 => self.style.attributes.hidden = true,
                9 => self.style.attributes.strikethrough = true,
                // The shared `vte` SGR table and Alacritty both read 21 as
                // "cancel bold" rather than ECMA-48 double underline.
                21 => self.style.attributes.bold = false,
                22 => {
                    self.style.attributes.bold = false;
                    self.style.attributes.dim = false;
                }
                23 => self.style.attributes.italic = false,
                24 => self.style.attributes.underline = Underline::None,
                25 => self.style.attributes.blink = Blink::None,
                27 => self.style.attributes.reverse = false,
                28 => self.style.attributes.hidden = false,
                29 => self.style.attributes.strikethrough = false,
                30..=37 => self.style.foreground = Color::Indexed(palette_index(code - 30)),
                38 => {
                    let (color, consumed) = parse_extended_color(&groups, index);
                    if let Some(color) = color {
                        self.style.foreground = color;
                    }
                    index += consumed;
                }
                39 => self.style.foreground = Color::Default,
                40..=47 => self.style.background = Color::Indexed(palette_index(code - 40)),
                48 => {
                    let (color, consumed) = parse_extended_color(&groups, index);
                    if let Some(color) = color {
                        self.style.background = color;
                    }
                    index += consumed;
                }
                49 => self.style.background = Color::Default,
                51 => self.style.attributes.frame = Frame::Framed,
                52 => self.style.attributes.frame = Frame::Encircled,
                53 => self.style.attributes.overline = true,
                54 => self.style.attributes.frame = Frame::None,
                55 => self.style.attributes.overline = false,
                58 => {
                    let (color, consumed) = parse_extended_color(&groups, index);
                    if let Some(color) = color {
                        self.style.underline_color = color;
                    }
                    index += consumed;
                }
                59 => self.style.underline_color = Color::Default,
                60 => self.style.attributes.ideogram = Ideogram::Underline,
                61 => self.style.attributes.ideogram = Ideogram::DoubleUnderline,
                62 => self.style.attributes.ideogram = Ideogram::Overline,
                63 => self.style.attributes.ideogram = Ideogram::DoubleOverline,
                64 => self.style.attributes.ideogram = Ideogram::Stress,
                65 => self.style.attributes.ideogram = Ideogram::None,
                90..=97 => self.style.foreground = Color::Indexed(palette_index(code - 90 + 8)),
                100..=107 => self.style.background = Color::Indexed(palette_index(code - 100 + 8)),
                _ => {}
            }
            index += 1;
        }
    }

    // One flat dispatch table mirrors the CSI switch in the reference
    // implementations, so splitting it would hide the mapping it documents.
    #[allow(clippy::too_many_lines)]
    fn csi(&mut self, params: &Params, intermediates: &[u8], action: char) {
        let groups = params.iter().collect::<Vec<_>>();
        let private = intermediates == b"?";
        let first = || parameter(&groups, 0, 1);
        match (private, intermediates, action) {
            // Relative moves go through the same origin-aware clamp as CUP,
            // matching Alacritty's `move_up`/`move_down`, so a move that
            // leaves the scrolling region is clamped to it only in origin mode.
            (false, b"", 'A') => {
                let (x, y) = self.cursor();
                self.goto(relative_row(y, first(), false), x);
            }
            (false, b"", 'B' | 'e') => {
                let (x, y) = self.cursor();
                self.goto(relative_row(y, first(), true), x);
            }
            (false, b"", 'C' | 'a') => {
                let (x, y) = self.cursor();
                self.set_cursor(x.saturating_add(first()), y);
            }
            (false, b"", 'D') => {
                let (x, y) = self.cursor();
                self.set_cursor(x.saturating_sub(first()), y);
            }
            (false, b"", 'E') => {
                let (_, y) = self.cursor();
                self.goto(relative_row(y, first(), true), 0);
            }
            (false, b"", 'F') => {
                let (_, y) = self.cursor();
                self.goto(relative_row(y, first(), false), 0);
            }
            (false, b"", 'G' | '`') => {
                let (_, y) = self.cursor();
                self.set_cursor(parameter(&groups, 0, 1).saturating_sub(1), y);
            }
            (false, b"", 'd') => {
                let (x, _) = self.cursor();
                self.set_cursor_position(parameter(&groups, 0, 1).saturating_sub(1), x);
            }
            (false, b"", 'H' | 'f') => self.set_cursor_position(
                parameter(&groups, 0, 1).saturating_sub(1),
                parameter(&groups, 1, 1).saturating_sub(1),
            ),
            (false, b"", 'b') => {
                // REP repeats the last printed character, matching `vte`'s
                // own `preceding_char` bookkeeping.
                if let Some(character) = self.preceding_character {
                    for _ in 0..first() {
                        self.print_character(character);
                    }
                }
            }
            (false, b"", 'I') => self.move_forward_tabs(first()),
            (false, b"", 'Z') => self.back_tab(first()),
            (false, b"", 'J') => self.erase_display(
                u16::try_from(parameter(&groups, 0, 0)).expect("CSI parameters fit u16"),
            ),
            (false, b"", 'K') => self.erase_line(
                u16::try_from(parameter(&groups, 0, 0)).expect("CSI parameters fit u16"),
            ),
            (false, b"", 'X') => self.erase_characters(first()),
            (false, b"", '@') => self.insert_characters(first()),
            (false, b"", 'P') => self.delete_characters(first()),
            (false, b"", 'L') => self.insert_lines(first()),
            (false, b"", 'M') => self.delete_lines(first()),
            (false, b"", 'S') => self.scroll_up(self.top_margin, self.bottom_margin, first()),
            (false, b"", 'T') => self.scroll_down(self.top_margin, self.bottom_margin, first()),
            (false, b"", 'm') => self.apply_sgr(params),
            (false, b"", 'r') => {
                let top = parameter(&groups, 0, 1);
                let bottom = groups
                    .get(1)
                    .and_then(|group| group.first())
                    .copied()
                    .filter(|value| *value != 0)
                    .map(usize::from);
                self.set_scroll_region(top, bottom);
            }
            (false, b"", 's') => self.save_cursor(),
            (false, b"", 'u') => self.restore_cursor(),
            (false, b"", 'h' | 'l') | (true, b"?", 'h' | 'l') => {
                let enabled = action == 'h';
                for group in groups {
                    for mode in group {
                        if private {
                            self.set_private_mode(*mode, enabled);
                        } else {
                            self.set_mode(*mode, enabled);
                        }
                    }
                }
            }
            (false, b"", 'g') => match parameter(&groups, 0, 0) {
                0 => {
                    let x = self.screen().x;
                    self.tab_stops[x] = false;
                }
                3 => self.tab_stops.fill(false),
                _ => {}
            },
            (false, b" ", 'q') => {
                // DECSCUSR 0 clears the explicit style, matching Alacritty.
                self.cursor_shape = match parameter(&groups, 0, 0) {
                    0 => None,
                    3 | 4 => Some(CursorShape::Underline),
                    5 | 6 => Some(CursorShape::Bar),
                    1 | 2 => Some(CursorShape::Block),
                    _ => self.cursor_shape,
                };
            }
            // DA/DSR/DECRQSS/window queries are deliberately offline no-ops.
            _ => {}
        }
    }
}

impl Perform for Terminal {
    fn print(&mut self, character: char) {
        self.print_character(character);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => {
                let (x, y) = self.cursor();
                self.set_cursor(x.saturating_sub(1), y);
            }
            b'\t' => self.put_tab(1),
            b'\n' | 0x0b | 0x0c => self.linefeed(),
            b'\r' => {
                let y = self.screen().y;
                self.set_cursor(0, y);
            }
            0x0e => self.use_g1 = true,
            0x0f => self.use_g1 = false,
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if !ignore {
            self.csi(params, intermediates, action);
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        match (intermediates, byte) {
            (b"", b'7') => self.save_cursor(),
            (b"", b'8') => self.restore_cursor(),
            (b"", b'D') => self.linefeed(),
            (b"", b'E') => {
                let y = self.screen().y;
                self.set_cursor(0, y);
                self.linefeed();
            }
            (b"", b'M') => self.reverse_index(),
            (b"", b'H') => {
                let x = self.screen().x;
                self.tab_stops[x] = true;
            }
            (b"", b'c') => self.reset(),
            (b"#", b'8') => self.screen_alignment_pattern(),
            (b"(", b'0') => self.g0_special = true,
            (b"(", b'B') => self.g0_special = false,
            (b")", b'0') => self.g1_special = true,
            (b")", b'B') => self.g1_special = false,
            _ => {}
        }
    }

    // DCS, OSC, APC, PM, and SOS payloads are intentionally ignored. Query
    // sequences never receive replies because this terminal is offline.
}

/// Offsets a zero-based row without clamping, so the caller's origin-aware
/// `goto` performs the one clamp. Alacritty subtracts before clamping, which
/// lets a relative move pass the origin offset.
fn relative_row(row: usize, delta: usize, forward: bool) -> isize {
    let row = isize::try_from(row).unwrap_or(isize::MAX);
    let delta = isize::try_from(delta).unwrap_or(isize::MAX);
    if forward {
        row.saturating_add(delta)
    } else {
        row.saturating_sub(delta)
    }
}

fn coordinate(value: usize) -> u16 {
    u16::try_from(value).expect("terminal coordinates originate from u16 dimensions")
}

fn palette_index(value: u16) -> u8 {
    u8::try_from(value).expect("matched ANSI palette ranges fit u8")
}

fn parameter(groups: &[&[u16]], index: usize, default: usize) -> usize {
    groups
        .get(index)
        .and_then(|group| group.first())
        .copied()
        .map_or(default, |value| {
            if value == 0 && default != 0 {
                default
            } else {
                usize::from(value)
            }
        })
}

fn channel(value: u16) -> Option<u8> {
    u8::try_from(value).ok()
}

/// Parses both `38;2;r;g;b` / `38;5;n` and colon forms such as `38:2::r:g:b`.
///
/// The returned count is the number of extra parameter groups consumed after
/// the leading 38/48/58 group. Consumption stops at the first missing or
/// out-of-range value, exactly like `vte`'s `parse_sgr_color`, so a malformed
/// color leaves the remaining parameters to be read as ordinary attributes.
fn parse_extended_color(groups: &[&[u16]], index: usize) -> (Option<Color>, usize) {
    let leading = groups[index];
    if leading.len() > 1 {
        // Colon form: every value is inside the leading group, so no extra
        // semicolon-separated group is consumed.
        let values = if leading.len() > 5 {
            // The ISO form carries an unused color-space slot after the mode.
            &leading[2..]
        } else {
            &leading[1..]
        };
        let mut parameters = std::iter::once(leading[1]).chain(values.iter().copied().skip(1));
        return (color_from_parameters(&mut parameters).0, 0);
    }

    let mut parameters = groups[index + 1..]
        .iter()
        .filter_map(|group| group.first().copied());
    let (color, consumed) = color_from_parameters(&mut parameters);
    (color, consumed)
}

/// Reads one SGR color from a flat parameter stream and reports how many
/// values were taken, including a value that failed to convert.
fn color_from_parameters(parameters: &mut impl Iterator<Item = u16>) -> (Option<Color>, usize) {
    let mut consumed = 0;
    let mut channels = [0_u8; 3];
    let wanted = match take(parameters, &mut consumed) {
        Some(2) => 3,
        Some(5) => 1,
        _ => return (None, consumed),
    };
    for slot in channels.iter_mut().take(wanted) {
        match take(parameters, &mut consumed).and_then(channel) {
            Some(value) => *slot = value,
            None => return (None, consumed),
        }
    }
    let color = if wanted == 1 {
        Color::Indexed(channels[0])
    } else {
        Color::Rgb(Rgb::new(channels[0], channels[1], channels[2]))
    };
    (Some(color), consumed)
}

fn take(parameters: &mut impl Iterator<Item = u16>, consumed: &mut usize) -> Option<u16> {
    let value = parameters.next();
    if value.is_some() {
        *consumed += 1;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_utf8_crlf_wide_and_combining_are_valid() {
        let input = "A\r\n界e\u{301}\nZ".as_bytes();
        let expected = ansi_to_grid(input, 8, 4).unwrap();
        for split in 0..=input.len() {
            let mut parser = AnsiParser::new(8, 4).unwrap();
            parser.feed(&input[..split]);
            parser.feed(&input[split..]);
            let actual = parser.snapshot();
            assert_eq!(actual, expected, "split at {split}");
            let encoded = serde_json::to_string(&actual).unwrap();
            let _: Grid = serde_json::from_str(&encoded).unwrap();
        }
        assert_eq!(expected.get(0, 1).unwrap().width, CellWidth::Wide);
        assert_eq!(expected.get(1, 1).unwrap().width, CellWidth::Continuation);
        assert_eq!(expected.get(2, 1).unwrap().symbol, "e\u{301}");
    }

    #[test]
    fn sgr_preserves_all_core_style_fields() {
        let grid = ansi_to_grid(
            "\x1b[1;2;3;4:3;5;7;8;9;38;5;196;48;2;1;2;3;58:2::4:5:6;51;53;60mX",
            4,
            2,
        )
        .unwrap();
        let style = grid.get(0, 0).unwrap().style;
        assert_eq!(style.foreground, Color::Indexed(196));
        assert_eq!(style.background, Color::Rgb(Rgb::new(1, 2, 3)));
        assert_eq!(style.underline_color, Color::Rgb(Rgb::new(4, 5, 6)));
        assert_eq!(style.attributes.underline, Underline::Curly);
        assert_eq!(style.attributes.blink, Blink::Slow);
        assert_eq!(style.attributes.frame, Frame::Framed);
        assert_eq!(style.attributes.ideogram, Ideogram::Underline);
        assert!(style.attributes.bold);
        assert!(style.attributes.dim);
        assert!(style.attributes.italic);
        assert!(style.attributes.reverse);
        assert!(style.attributes.hidden);
        assert!(style.attributes.strikethrough);
        assert!(style.attributes.overline);
    }

    #[test]
    fn movement_editing_scroll_tabs_alt_and_cursor_modes() {
        let input = concat!(
            "abcdef",
            "\x1b[3D\x1b[PZ",
            "\x1b[2;3HQ",
            "\x1b[2;3r\x1b[3;1Hbottom\nscroll",
            "\x1b[?1049hALT\x1b[?1049l",
            "\x1b[?25l\x1b[5 q"
        );
        let grid = ansi_to_grid(input, 10, 4).unwrap();
        let cursor = grid.cursor.unwrap();
        assert!(!cursor.visible);
        // Alacritty models a hidden cursor as its own shape, so DECSCUSR is
        // masked while the cursor is hidden and reappears when it is shown.
        assert_eq!(cursor.shape, CursorShape::Block);
        let shown = ansi_to_grid(concat!("\x1b[?25l\x1b[5 q", "\x1b[?25h"), 10, 4)
            .unwrap()
            .cursor
            .unwrap();
        assert!(shown.visible);
        assert_eq!(shown.shape, CursorShape::Bar);
        let encoded = serde_json::to_string(&grid).unwrap();
        let _: Grid = serde_json::from_str(&encoded).unwrap();
    }

    #[test]
    fn dec_special_graphics_and_query_sequences() {
        let grid = ansi_to_grid("\x1b(0lqk\x1b(B\x1b[6nX", 8, 2).unwrap();
        assert_eq!(&grid.plain_text()[.."┌─┐X".len()], "┌─┐X");
    }

    #[test]
    fn insert_delete_and_line_editing_have_fixed_screen_semantics() {
        let inserted = ansi_to_grid("abcdef\x1b[3D\x1b[2@XY", 8, 2).unwrap();
        assert_eq!(inserted.plain_text().lines().next().unwrap(), "abcXYdef");

        let deleted = ansi_to_grid("abcdef\x1b[4D\x1b[2P", 8, 2).unwrap();
        assert_eq!(deleted.plain_text().lines().next().unwrap(), "abef    ");

        let lines = ansi_to_grid("one\ntwo\nthree\x1b[2;1H\x1b[L", 8, 3).unwrap();
        let rows = lines
            .plain_text()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["one     ", "        ", "two     "]);
    }

    #[test]
    fn full_region_scroll_alt_restore_and_one_column_wide_are_safe() {
        let scrolled = ansi_to_grid("a\nb\nc\x1b[3S", 3, 3).unwrap();
        assert_eq!(scrolled.plain_text(), "   \n   \n   ");

        let restored = ansi_to_grid("main\x1b[?1049hALT\x1b[?1049l", 8, 2).unwrap();
        assert!(restored.plain_text().starts_with("main"));
        assert_eq!(restored.cursor.unwrap().x, 4);

        let narrow = ansi_to_grid("界", 1, 2).unwrap();
        assert_eq!(narrow.get(0, 0).unwrap().width, CellWidth::Single);
        let json = serde_json::to_vec(&narrow).unwrap();
        let _: Grid = serde_json::from_slice(&json).unwrap();
    }

    #[test]
    fn arbitrary_bytes_and_chunking_never_break_wide_pairs() {
        let mut bytes = Vec::new();
        let mut state = 0x1234_5678_u32;
        for _ in 0..16_384 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push(state.to_le_bytes()[0]);
        }
        let mut parser = AnsiParser::new(37, 11).unwrap();
        for chunk in bytes.chunks(17) {
            parser.feed(chunk);
            let grid = parser.snapshot();
            let json = serde_json::to_vec(&grid).unwrap();
            let _: Grid = serde_json::from_slice(&json).unwrap();
        }
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

#[cfg(all(test, feature = "alacritty"))]
mod differential_tests {
    use super::*;

    struct Fixture {
        name: &'static str,
        input: &'static str,
        columns: u16,
        rows: u16,
    }

    /// This corpus is the compatibility boundary promised by the wasm backend.
    /// Keep additions limited to state that Alacritty exposes through
    /// `term_to_grid`; the vte backend intentionally preserves extra style
    /// fields such as blink, frame, overline, and ideogram decoration.
    const SHARED_CORPUS: &[Fixture] = &[
        Fixture {
            name: "plain LF and CRLF",
            input: "plain\ntext\r\nend",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "shared SGR attributes and resets",
            input: "\x1b[1;2;3;4;7;8;9;31;44mX\x1b[22;23;24;27;28;29;39;49mY",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "SGR 21 cancels bold",
            input: "\x1b[1mA\x1b[21mB",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "bright indexed and RGB colors",
            input: "\x1b[90;107mA\x1b[38;5;196;48;2;1;2;3;58;5;45mB\x1b[0m",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "colon RGB and malformed color parameters",
            input: "\x1b[48:2::9:8:7mA\x1b[38mB\x1b[38;5mC\x1b[38;2;300;1;2mD",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "underline variants and cancels",
            input: "\x1b[4:2mA\x1b[4:3mB\x1b[4:4mC\x1b[4:5mD\x1b[4:0mE\x1b[4mF\x1b[24mG\x1b[58;5;9mH\x1b[59mI",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "wide and combining Unicode",
            input: "A界B\ne\u{301}界\u{301}\u{301}Z\u{200d}",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "cursor movement erase and shape",
            input: "abcdef\x1b[3D\x1b[KZ\x1b[2;3HQ\x1b[5 q",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "cursor visibility",
            input: "\x1b[?25l",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "shape while hidden and after showing again",
            input: "\x1b[?25l\x1b[5 q",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "shape survives hide and show",
            input: "\x1b[5 q\x1b[?25l\x1b[?25h",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "relative movement and column addressing",
            input: "abcdef\x1b[2DXX\x1b[2;3HQ\x1b[1A\x1b[2C!\x1b[1B\x1b[1GZ\x1b[3dW\x1b[3GV",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "next and previous line",
            input: "one\ntwo\nthree\x1b[2;3H\x1b[1EX\x1b[1FY",
            columns: 20,
            rows: 4,
        },
        Fixture {
            name: "character and line editing",
            input: "one\ntwo\nthree\x1b[2;2H\x1b[2@XY\x1b[P\x1b[L\x1b[M",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "erase modes for line and display",
            input: "one\ntwo\nthree\x1b[2;2H\x1b[1J\x1b[1K\x1b[3;2H\x1b[J",
            columns: 20,
            rows: 4,
        },
        Fixture {
            name: "erase above only clears below line one",
            input: "one\ntwo\nthree\x1b[2;2H\x1b[1J",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "erase scrollback is a no-op",
            input: "one\x1b[3JX",
            columns: 10,
            rows: 3,
        },
        Fixture {
            name: "background color erase",
            input: "\x1b[1;4;7;31;44m\x1b[2JX\x1b[2K",
            columns: 8,
            rows: 3,
        },
        Fixture {
            name: "erase characters keeps background",
            input: "abcdef\x1b[1;2H\x1b[44m\x1b[3X",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "delete past the last column clears the row",
            input: "abcdef\x1b[1;5H\x1b[10P",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "insert past the last column",
            input: "abcdef\x1b[1;2H\x1b[20@",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "save and restore through both encodings",
            input: "A\x1b7\x1b[3;4HZ\x1b8B\x1b[s\x1b[2;2HY\x1b[uC",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "scrolling region and index controls",
            input: "one\ntwo\nthree\nfour\x1b[2;4r\x1b[4;1H\nX\x1bM\x1b D\x1bEnel",
            columns: 20,
            rows: 5,
        },
        Fixture {
            name: "scroll up and down commands",
            input: "one\ntwo\nthree\x1b[2S\x1b[1T\x1b[44m\x1b[2S",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "scroll region reset and rejection",
            input: "\x1b[2;3r\x1b[rX\x1b[3;2rY",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "origin mode",
            input: "\x1b[2;3r\x1b[?6h\x1b[1;1HX\x1b[?6l\x1b[1;1HY",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "insert mode narrow and wide",
            input: "abcdef\x1b[1;1H\x1b[4hXY界\x1b[4lZ",
            columns: 12,
            rows: 2,
        },
        Fixture {
            name: "alternate screen restores cursor and style",
            input: "\x1b[31mmain\x1b[?1049h\x1b[32mALT\x1b[?1049lZ",
            columns: 10,
            rows: 3,
        },
        Fixture {
            name: "nested alternate screen requests",
            input: "a\x1b[?1049hb\x1b[?1049hc\x1b[?1049l\x1b[?1049ld",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "unimplemented alternate screen modes",
            input: "main\x1b[?47hA\x1b[?47l\x1b[?1047hB\x1b[?1047l\x1b[?1048h\x1b[?1048l",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "tab stops movement and clearing",
            input: "A\tB\x1b[2ZC\x1b[3IC\x1b[0g\x1b[1;1H\tD\x1b[3g\x1b[1;1H\tE",
            columns: 24,
            rows: 2,
        },
        Fixture {
            name: "tab after a pending wrap",
            input: "abcdefghij\tX",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "horizontal tabstop control",
            input: "\x1b[1;4H\x1bH\x1b[1;1H\tX",
            columns: 20,
            rows: 2,
        },
        Fixture {
            name: "DEC special graphics",
            input: "\x1b(0lqk\x1b(B\x1b)0\x0elqk\x0f",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "delayed narrow and wide wrapping",
            input: "abcdefghiJX\n123456789界",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "autowrap disabled",
            input: "\x1b[?7labcdefghijklmn\n\x1b[?7l12345678界",
            columns: 10,
            rows: 3,
        },
        Fixture {
            name: "wide cell at the right edge wraps",
            input: "12345678界X",
            columns: 10,
            rows: 3,
        },
        Fixture {
            name: "overwriting either half of a wide cell",
            input: "界X\x1b[1;1HA\x1b[2;1H界\x1b[2;2HB",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "C0 controls",
            input: "a\x07b\x08c\x0bd\x0ce\rf",
            columns: 10,
            rows: 4,
        },
        Fixture {
            name: "repeat preceding character",
            input: "a\x1b[3bX\x1b[bY",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "screen alignment pattern",
            input: "\x1b#8",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "full reset",
            input: "\x1b[31mabc\x1bcX",
            columns: 10,
            rows: 2,
        },
        Fixture {
            name: "ignored strings and queries",
            input: "\x1b]0;title\x07A\x1bP1$tQ\x1b\\B\x1b[6n\x1b[0c\x1b[?3hC",
            columns: 20,
            rows: 3,
        },
        Fixture {
            name: "scrolling past the last row",
            input: "one\ntwo\nthree\nfour\nfive",
            columns: 10,
            rows: 3,
        },
    ];

    #[test]
    fn supported_static_subset_matches_alacritty_for_one_shot_and_every_split() {
        for fixture in SHARED_CORPUS {
            let expected =
                crate::alacritty::ansi_to_grid(fixture.input, fixture.columns, fixture.rows)
                    .unwrap();
            let actual = ansi_to_grid(fixture.input, fixture.columns, fixture.rows).unwrap();
            assert_eq!(actual, expected, "one-shot fixture {:?}", fixture.name);

            let bytes = fixture.input.as_bytes();
            for split in 0..=bytes.len() {
                let mut alacritty =
                    crate::alacritty::AnsiParser::new(fixture.columns, fixture.rows).unwrap();
                let mut vte = AnsiParser::new(fixture.columns, fixture.rows).unwrap();
                alacritty.feed(&bytes[..split]);
                vte.feed(&bytes[..split]);
                alacritty.feed(&bytes[split..]);
                vte.feed(&bytes[split..]);
                assert_eq!(
                    vte.snapshot(),
                    alacritty.snapshot(),
                    "split {split} in fixture {:?}",
                    fixture.name
                );
            }
        }
    }
}
