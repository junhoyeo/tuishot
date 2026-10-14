//! Shared terminal cell-grid model.
//!
//! The model keeps display width explicit. A wide grapheme occupies one
//! [`CellWidth::Wide`] cell followed by one [`CellWidth::Continuation`] cell.
//! Renderers must never infer width again from the stored text.

use serde::{Deserialize, Deserializer, Serialize, de};
use std::{error::Error, fmt};

/// An RGB color with eight bits per channel.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb {
    /// Creates a color from red, green, and blue channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Returns a lowercase CSS hexadecimal color.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A terminal color before it is resolved against a theme.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Color {
    /// Use the theme's foreground or background color.
    #[default]
    Default,
    /// An xterm palette index. Values 0 through 15 use the theme palette.
    Indexed(u8),
    /// A 24-bit color.
    Rgb(Rgb),
}

/// Whether a default color is being resolved for foreground or background.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorRole {
    /// Text and decoration color.
    Foreground,
    /// Cell background color.
    Background,
}

/// A complete terminal palette.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Human-readable theme name.
    pub name: String,
    /// Default foreground.
    pub foreground: Rgb,
    /// Default background.
    pub background: Rgb,
    /// Cursor color.
    pub cursor: Rgb,
    /// ANSI colors 0 through 15.
    pub palette: [Rgb; 16],
}

impl Theme {
    /// Resolves a terminal color deterministically.
    #[must_use]
    pub fn resolve(&self, color: Color, role: ColorRole) -> Rgb {
        match color {
            Color::Default => match role {
                ColorRole::Foreground => self.foreground,
                ColorRole::Background => self.background,
            },
            Color::Indexed(index @ 0..=15) => self.palette[usize::from(index)],
            Color::Indexed(index @ 16..=231) => {
                let offset = index - 16;
                let red = offset / 36;
                let green = (offset % 36) / 6;
                let blue = offset % 6;
                Rgb::new(cube_channel(red), cube_channel(green), cube_channel(blue))
            }
            Color::Indexed(index) => {
                let channel = 8 + (index - 232) * 10;
                Rgb::new(channel, channel, channel)
            }
            Color::Rgb(rgb) => rgb,
        }
    }

    /// ttyshot's deterministic dark palette.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "ttyshot-dark".to_owned(),
            foreground: Rgb::new(0xd8, 0xde, 0xe9),
            background: Rgb::new(0x17, 0x1b, 0x24),
            cursor: Rgb::new(0xd8, 0xde, 0xe9),
            palette: [
                Rgb::new(0x2e, 0x34, 0x40),
                Rgb::new(0xbf, 0x61, 0x6a),
                Rgb::new(0xa3, 0xbe, 0x8c),
                Rgb::new(0xeb, 0xcb, 0x8b),
                Rgb::new(0x81, 0xa1, 0xc1),
                Rgb::new(0xb4, 0x8e, 0xad),
                Rgb::new(0x88, 0xc0, 0xd0),
                Rgb::new(0xe5, 0xe9, 0xf0),
                Rgb::new(0x4c, 0x56, 0x6a),
                Rgb::new(0xbf, 0x61, 0x6a),
                Rgb::new(0xa3, 0xbe, 0x8c),
                Rgb::new(0xeb, 0xcb, 0x8b),
                Rgb::new(0x81, 0xa1, 0xc1),
                Rgb::new(0xb4, 0x8e, 0xad),
                Rgb::new(0x8f, 0xbc, 0xbb),
                Rgb::new(0xec, 0xef, 0xf4),
            ],
        }
    }

    /// ttyshot's deterministic light palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "ttyshot-light".to_owned(),
            foreground: Rgb::new(0x24, 0x29, 0x33),
            background: Rgb::new(0xf4, 0xf6, 0xf8),
            cursor: Rgb::new(0x24, 0x29, 0x33),
            palette: [
                Rgb::new(0x2e, 0x34, 0x40),
                Rgb::new(0xa3, 0x3a, 0x45),
                Rgb::new(0x4f, 0x78, 0x32),
                Rgb::new(0x8a, 0x67, 0x15),
                Rgb::new(0x3c, 0x64, 0x91),
                Rgb::new(0x75, 0x42, 0x73),
                Rgb::new(0x27, 0x70, 0x78),
                Rgb::new(0xd8, 0xde, 0xe9),
                Rgb::new(0x4c, 0x56, 0x6a),
                Rgb::new(0xbf, 0x61, 0x6a),
                Rgb::new(0x6f, 0x93, 0x55),
                Rgb::new(0xb4, 0x87, 0x28),
                Rgb::new(0x5e, 0x81, 0xac),
                Rgb::new(0x8f, 0x62, 0x8d),
                Rgb::new(0x4c, 0x91, 0x99),
                Rgb::new(0xec, 0xef, 0xf4),
            ],
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

const fn cube_channel(value: u8) -> u8 {
    if value == 0 { 0 } else { 55 + value * 40 }
}

/// Underline form selected by SGR 4 and its colon variants.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Underline {
    /// No underline.
    #[default]
    None,
    /// SGR 4 or `4:1`.
    Single,
    /// SGR 21 or `4:2`.
    Double,
    /// SGR `4:3`.
    Curly,
    /// SGR `4:4`.
    Dotted,
    /// SGR `4:5`.
    Dashed,
}

/// Blink mode selected by SGR 5 or 6.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blink {
    /// No blinking.
    #[default]
    None,
    /// SGR 5.
    Slow,
    /// SGR 6.
    Rapid,
}

/// Framing mode selected by SGR 51 or 52.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// No frame.
    #[default]
    None,
    /// SGR 51.
    Framed,
    /// SGR 52.
    Encircled,
}

/// Ideogram decoration selected by SGR 60 through 64.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ideogram {
    /// No ideogram decoration.
    #[default]
    None,
    /// SGR 60.
    Underline,
    /// SGR 61.
    DoubleUnderline,
    /// SGR 62.
    Overline,
    /// SGR 63.
    DoubleOverline,
    /// SGR 64.
    Stress,
}

/// All visual Select Graphic Rendition attributes represented by the grid.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Each flag maps directly to an independent SGR attribute.
pub struct Attributes {
    /// SGR 1.
    pub bold: bool,
    /// SGR 2.
    pub dim: bool,
    /// SGR 3.
    pub italic: bool,
    /// SGR 4/21 and `4:n` variants.
    pub underline: Underline,
    /// SGR 5/6.
    pub blink: Blink,
    /// SGR 7.
    pub reverse: bool,
    /// SGR 8.
    pub hidden: bool,
    /// SGR 9.
    pub strikethrough: bool,
    /// SGR 51/52.
    pub frame: Frame,
    /// SGR 53.
    pub overline: bool,
    /// SGR 60 through 64.
    pub ideogram: Ideogram,
}

/// The visual style of one terminal cell.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Foreground color.
    pub foreground: Color,
    /// Background color.
    pub background: Color,
    /// Underline color from SGR 58.
    pub underline_color: Color,
    /// SGR attributes.
    pub attributes: Attributes,
}

/// The display-width role of a stored cell.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellWidth {
    /// A normal one-cell grapheme.
    #[default]
    Single,
    /// The leading half of a two-cell grapheme.
    Wide,
    /// The trailing half of a two-cell grapheme. It carries no text.
    Continuation,
}

/// One terminal grid cell.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// A grapheme cluster. Continuation cells always store an empty string.
    pub symbol: String,
    /// The explicit display-width role.
    pub width: CellWidth,
    /// Cell style. A continuation repeats its leading cell's style.
    pub style: Style,
}

impl Cell {
    /// Creates a one-cell grapheme.
    #[must_use]
    pub fn single(symbol: impl Into<String>, style: Style) -> Self {
        Self {
            symbol: symbol.into(),
            width: CellWidth::Single,
            style,
        }
    }

    /// Creates a blank cell with the supplied style.
    #[must_use]
    pub fn blank(style: Style) -> Self {
        Self::single(" ", style)
    }

    fn continuation(style: Style) -> Self {
        Self {
            symbol: String::new(),
            width: CellWidth::Continuation,
            style,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank(Style::default())
    }
}

/// Cursor shape.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    /// A filled cell.
    #[default]
    Block,
    /// A vertical bar.
    Bar,
    /// A horizontal underline.
    Underline,
}

/// Cursor state captured with a grid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Cursor {
    /// Zero-based column.
    pub x: u16,
    /// Zero-based row.
    pub y: u16,
    /// Cursor shape.
    pub shape: CursorShape,
    /// Whether the cursor is visible.
    pub visible: bool,
}

/// A cell-aligned crop rectangle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CellRect {
    /// Zero-based left column.
    pub x: u16,
    /// Zero-based top row.
    pub y: u16,
    /// Width in cells.
    pub width: u16,
    /// Height in cells.
    pub height: u16,
}

/// A complete terminal frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Grid {
    columns: u16,
    rows: u16,
    cells: Vec<Cell>,
    /// Optional cursor state.
    pub cursor: Option<Cursor>,
}

impl Grid {
    /// Creates a non-empty blank grid.
    ///
    /// # Errors
    /// Returns [`GridError::EmptyGrid`] when either dimension is zero.
    pub fn new(columns: u16, rows: u16) -> Result<Self, GridError> {
        if columns == 0 || rows == 0 {
            return Err(GridError::EmptyGrid);
        }
        Ok(Self {
            columns,
            rows,
            cells: vec![Cell::default(); usize::from(columns) * usize::from(rows)],
            cursor: None,
        })
    }

    /// Builds a grid from row-major cells after checking every invariant.
    ///
    /// # Errors
    /// Returns a [`GridError`] for empty dimensions, an incorrect cell count,
    /// malformed wide-cell pairs, or a cursor outside the grid.
    pub fn from_cells(
        columns: u16,
        rows: u16,
        cells: Vec<Cell>,
        cursor: Option<Cursor>,
    ) -> Result<Self, GridError> {
        let grid = Self {
            columns,
            rows,
            cells,
            cursor,
        };
        grid.validate()?;
        Ok(grid)
    }

    /// Returns the number of columns.
    #[must_use]
    pub const fn columns(&self) -> u16 {
        self.columns
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn rows(&self) -> u16 {
        self.rows
    }

    /// Returns all cells in row-major order.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Returns a cell by zero-based coordinate.
    #[must_use]
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).map(|index| &self.cells[index])
    }

    /// Writes a one- or two-cell grapheme and preserves the width invariant.
    ///
    /// # Errors
    /// Returns [`GridError::OutOfBounds`] for an invalid coordinate,
    /// [`GridError::WideAtRightEdge`] when a wide grapheme cannot fit, or
    /// [`GridError::InvalidCellWidth`] for a continuation cell.
    pub fn put(
        &mut self,
        x: u16,
        y: u16,
        symbol: impl Into<String>,
        width: CellWidth,
        style: Style,
    ) -> Result<(), GridError> {
        let index = self.index(x, y).ok_or(GridError::OutOfBounds { x, y })?;
        if width == CellWidth::Continuation {
            return Err(GridError::InvalidCellWidth);
        }
        if width == CellWidth::Wide && x + 1 >= self.columns {
            return Err(GridError::WideAtRightEdge { x, y });
        }

        self.clear_footprint(x, y);
        if width == CellWidth::Wide {
            self.clear_footprint(x + 1, y);
        }

        self.cells[index] = Cell {
            symbol: symbol.into(),
            width,
            style,
        };
        if width == CellWidth::Wide {
            let Some(continuation) = self.index(x + 1, y) else {
                return Err(GridError::WideAtRightEdge { x, y });
            };
            self.cells[continuation] = Cell::continuation(style);
        }
        Ok(())
    }

    /// Replaces one cell with a blank and clears any connected wide-cell half.
    ///
    /// # Errors
    /// Returns [`GridError::OutOfBounds`] for an invalid coordinate.
    pub fn clear_cell(&mut self, x: u16, y: u16) -> Result<(), GridError> {
        self.index(x, y).ok_or(GridError::OutOfBounds { x, y })?;
        self.clear_footprint(x, y);
        Ok(())
    }

    /// Returns a validated cell crop.
    ///
    /// A wide glyph cut by a crop boundary becomes a styled blank cell.
    ///
    /// # Errors
    /// Returns [`GridError::InvalidRect`] when the rectangle is empty or lies
    /// outside the grid.
    pub fn crop(&self, rect: CellRect) -> Result<Self, GridError> {
        let right = rect.x.checked_add(rect.width);
        let bottom = rect.y.checked_add(rect.height);
        if rect.width == 0
            || rect.height == 0
            || right.is_none_or(|right| right > self.columns)
            || bottom.is_none_or(|bottom| bottom > self.rows)
        {
            return Err(GridError::InvalidRect(rect));
        }

        let mut cropped = Self::new(rect.width, rect.height)?;
        for y in 0..rect.height {
            for x in 0..rect.width {
                let Some(source) = self.get(rect.x + x, rect.y + y) else {
                    return Err(GridError::InvalidRect(rect));
                };
                let target = usize::from(y) * usize::from(rect.width) + usize::from(x);
                cropped.cells[target] = source.clone();
            }
        }
        cropped.repair_boundary_widths();
        cropped.cursor = self.cursor.and_then(|cursor| {
            let inside = cursor.x >= rect.x
                && cursor.x < rect.x + rect.width
                && cursor.y >= rect.y
                && cursor.y < rect.y + rect.height;
            inside.then_some(Cursor {
                x: cursor.x - rect.x,
                y: cursor.y - rect.y,
                ..cursor
            })
        });
        Ok(cropped)
    }

    /// Returns the visible text, preserving the full rectangular frame.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut output = String::new();
        for (y, row) in self.cells.chunks(usize::from(self.columns)).enumerate() {
            for cell in row {
                if cell.width != CellWidth::Continuation {
                    output.push_str(&cell.symbol);
                }
            }
            if y + 1 != usize::from(self.rows) {
                output.push('\n');
            }
        }
        output
    }

    fn validate(&self) -> Result<(), GridError> {
        if self.columns == 0 || self.rows == 0 {
            return Err(GridError::EmptyGrid);
        }
        let expected = usize::from(self.columns) * usize::from(self.rows);
        if self.cells.len() != expected {
            return Err(GridError::InvalidCellCount {
                expected,
                actual: self.cells.len(),
            });
        }
        if let Some(cursor) = self.cursor
            && (cursor.x >= self.columns || cursor.y >= self.rows)
        {
            return Err(GridError::InvalidCursor {
                x: cursor.x,
                y: cursor.y,
            });
        }

        for y in 0..self.rows {
            let row_start = usize::from(y) * usize::from(self.columns);
            for x in 0..self.columns {
                let index = row_start + usize::from(x);
                let cell = &self.cells[index];
                match cell.width {
                    CellWidth::Single => {}
                    CellWidth::Wide => {
                        if x + 1 >= self.columns {
                            return Err(GridError::MalformedWideCell { x, y });
                        }
                        let trailing = &self.cells[index + 1];
                        if trailing.width != CellWidth::Continuation
                            || !trailing.symbol.is_empty()
                            || trailing.style != cell.style
                        {
                            return Err(GridError::MalformedWideCell { x, y });
                        }
                    }
                    CellWidth::Continuation => {
                        if !cell.symbol.is_empty()
                            || x == 0
                            || self.cells[index - 1].width != CellWidth::Wide
                            || self.cells[index - 1].style != cell.style
                        {
                            return Err(GridError::MalformedWideCell { x, y });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.columns && y < self.rows)
            .then_some(usize::from(y) * usize::from(self.columns) + usize::from(x))
    }

    fn clear_footprint(&mut self, x: u16, y: u16) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        match self.cells[index].width {
            CellWidth::Single => self.cells[index] = Cell::default(),
            CellWidth::Wide => {
                self.cells[index] = Cell::default();
                if let Some(next) = self.index(x + 1, y) {
                    self.cells[next] = Cell::default();
                }
            }
            CellWidth::Continuation => {
                let style = self.cells[index].style;
                self.cells[index] = Cell::blank(style);
                if x > 0 {
                    let previous = self.index(x - 1, y).expect("previous cell is in bounds");
                    if self.cells[previous].width == CellWidth::Wide {
                        self.cells[previous] = Cell::default();
                    }
                }
            }
        }
    }

    fn repair_boundary_widths(&mut self) {
        for y in 0..self.rows {
            let first = usize::from(y) * usize::from(self.columns);
            if self.cells[first].width == CellWidth::Continuation {
                let style = self.cells[first].style;
                self.cells[first] = Cell::blank(style);
            }
            let last = first + usize::from(self.columns) - 1;
            if self.cells[last].width == CellWidth::Wide {
                let style = self.cells[last].style;
                self.cells[last] = Cell::blank(style);
            }
        }
    }
}

/// Grid construction or access error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridError {
    /// A grid dimension was zero.
    EmptyGrid,
    /// A coordinate was outside the grid.
    OutOfBounds {
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// A wide cell started in the final column.
    WideAtRightEdge {
        /// Column.
        x: u16,
        /// Row.
        y: u16,
    },
    /// Callers cannot write continuation cells directly.
    InvalidCellWidth,
    /// The row-major cell count did not match the dimensions.
    InvalidCellCount {
        /// Required number of cells.
        expected: usize,
        /// Supplied number of cells.
        actual: usize,
    },
    /// A wide/continuation pair was malformed.
    MalformedWideCell {
        /// Column of the malformed half.
        x: u16,
        /// Row of the malformed half.
        y: u16,
    },
    /// The cursor position was outside the grid.
    InvalidCursor {
        /// Cursor column.
        x: u16,
        /// Cursor row.
        y: u16,
    },
    /// A crop was empty or outside the grid.
    InvalidRect(CellRect),
}

impl fmt::Display for GridError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGrid => formatter.write_str("terminal grid dimensions must be non-zero"),
            Self::OutOfBounds { x, y } => write!(formatter, "cell ({x}, {y}) is outside the grid"),
            Self::WideAtRightEdge { x, y } => {
                write!(formatter, "wide cell at ({x}, {y}) crosses the right edge")
            }
            Self::InvalidCellWidth => {
                formatter.write_str("continuation cells cannot be written directly")
            }
            Self::InvalidCellCount { expected, actual } => write!(
                formatter,
                "grid dimensions require {expected} cells, but {actual} were supplied"
            ),
            Self::MalformedWideCell { x, y } => {
                write!(formatter, "wide-cell invariant is broken at ({x}, {y})")
            }
            Self::InvalidCursor { x, y } => {
                write!(formatter, "cursor ({x}, {y}) is outside the grid")
            }
            Self::InvalidRect(rect) => write!(
                formatter,
                "cell crop {},{} {}x{} is empty or outside the grid",
                rect.x, rect.y, rect.width, rect.height
            ),
        }
    }
}

impl<'de> Deserialize<'de> for Grid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedGrid {
            columns: u16,
            rows: u16,
            cells: Vec<Cell>,
            cursor: Option<Cursor>,
        }

        let serialized = SerializedGrid::deserialize(deserializer)?;
        Self::from_cells(
            serialized.columns,
            serialized.rows,
            serialized.cells,
            serialized.cursor,
        )
        .map_err(de::Error::custom)
    }
}

impl Error for GridError {}

/// Reports the crate version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_grapheme_occupies_exactly_two_cells() {
        let mut grid = Grid::new(4, 1).unwrap();
        grid.put(1, 0, "界", CellWidth::Wide, Style::default())
            .unwrap();

        assert_eq!(grid.get(1, 0).unwrap().symbol, "界");
        assert_eq!(grid.get(1, 0).unwrap().width, CellWidth::Wide);
        assert_eq!(grid.get(2, 0).unwrap().symbol, "");
        assert_eq!(grid.get(2, 0).unwrap().width, CellWidth::Continuation);
        assert_eq!(grid.plain_text(), " 界 ");
    }

    #[test]
    fn overwriting_either_half_clears_the_wide_cell() {
        let mut grid = Grid::new(4, 1).unwrap();
        grid.put(1, 0, "界", CellWidth::Wide, Style::default())
            .unwrap();
        grid.put(2, 0, "x", CellWidth::Single, Style::default())
            .unwrap();

        assert_eq!(grid.get(1, 0).unwrap(), &Cell::default());
        assert_eq!(grid.get(2, 0).unwrap().symbol, "x");
    }

    #[test]
    fn crop_repairs_a_split_wide_cell() {
        let style = Style {
            background: Color::Indexed(1),
            ..Style::default()
        };
        let mut grid = Grid::new(4, 1).unwrap();
        grid.put(1, 0, "界", CellWidth::Wide, style).unwrap();

        let left = grid
            .crop(CellRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            })
            .unwrap();
        let right = grid
            .crop(CellRect {
                x: 2,
                y: 0,
                width: 2,
                height: 1,
            })
            .unwrap();

        assert_eq!(left.get(1, 0).unwrap(), &Cell::blank(style));
        assert_eq!(right.get(0, 0).unwrap(), &Cell::blank(style));
    }

    #[test]
    fn indexed_colors_follow_xterm_cube_and_gray_ramp() {
        let theme = Theme::dark();
        assert_eq!(
            theme.resolve(Color::Indexed(16), ColorRole::Foreground),
            Rgb::new(0, 0, 0)
        );
        assert_eq!(
            theme.resolve(Color::Indexed(196), ColorRole::Foreground),
            Rgb::new(255, 0, 0)
        );
        assert_eq!(
            theme.resolve(Color::Indexed(255), ColorRole::Foreground),
            Rgb::new(238, 238, 238)
        );
    }

    #[test]
    fn truecolor_and_defaults_resolve_directly() {
        let theme = Theme::light();
        assert_eq!(
            theme.resolve(Color::Default, ColorRole::Foreground),
            theme.foreground
        );
        assert_eq!(
            theme.resolve(Color::Default, ColorRole::Background),
            theme.background
        );
        assert_eq!(
            theme.resolve(Color::Rgb(Rgb::new(1, 2, 3)), ColorRole::Foreground),
            Rgb::new(1, 2, 3)
        );
    }

    #[test]
    fn deserialize_rejects_zero_dimensions_and_wrong_cell_count() {
        let grid = Grid::new(2, 1).unwrap();
        let mut zero = serde_json::to_value(&grid).unwrap();
        zero["columns"] = serde_json::json!(0);
        assert!(serde_json::from_value::<Grid>(zero).is_err());

        let mut short = serde_json::to_value(&grid).unwrap();
        short["cells"].as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<Grid>(short).is_err());
    }

    #[test]
    fn deserialize_rejects_malformed_wide_pairs_and_cursor() {
        let mut grid = Grid::new(3, 1).unwrap();
        grid.put(0, 0, "界", CellWidth::Wide, Style::default())
            .unwrap();

        let mut malformed = serde_json::to_value(&grid).unwrap();
        malformed["cells"][1]["width"] = serde_json::json!("single");
        assert!(serde_json::from_value::<Grid>(malformed).is_err());

        grid.cursor = Some(Cursor {
            x: 1,
            y: 0,
            shape: CursorShape::Block,
            visible: true,
        });
        let encoded = serde_json::to_string(&grid).unwrap();
        assert_eq!(serde_json::from_str::<Grid>(&encoded).unwrap(), grid);

        let mut invalid_cursor: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        invalid_cursor["cursor"]["x"] = serde_json::json!(3);
        assert!(serde_json::from_value::<Grid>(invalid_cursor).is_err());
    }
}
