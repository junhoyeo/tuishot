//! Convert Ratatui buffers into ttyshot's version-independent cell grid.
//!
//! Enable `ratatui-0-29` for Ratatui 0.29 or `ratatui-0-30` for Ratatui
//! 0.30. Ratatui 0.29 is enabled by default for the first-party consumer. Both
//! features can be enabled in the same build.
//!
//! Buffers can be converted to [`ttyshot_core::Grid`] or rendered directly to
//! PNG and SVG without starting a terminal or PTY. The `snapshot_*` helpers use
//! only the bundled font so their output is stable across supported systems.

use std::{
    error::Error,
    ffi::OsString,
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(any(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
use ttyshot_core::{Attributes, Blink, CellWidth, Color, Rgb, Style, Underline};
use ttyshot_core::{Grid, GridError};

pub use ttyshot_core as core;
pub use ttyshot_render as render;
pub use ttyshot_render::{FontMode, FontSource, Padding, RenderOptions, WindowChrome};

/// A Ratatui buffer that can be converted into a ttyshot grid.
pub trait BufferExt {
    /// Converts the complete buffer into a local, zero-origin grid.
    ///
    /// A Ratatui buffer's `area.x` and `area.y` identify where it was composed,
    /// not cells in the captured frame, so only its width and height are kept.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] if the buffer is malformed, empty, or contains
    /// a symbol whose display width cannot be represented by ttyshot's one- or
    /// two-cell model.
    fn to_ttyshot_grid(&self) -> Result<Grid, AdapterError>;
}

/// Converts a supported Ratatui buffer into a ttyshot grid.
///
/// # Errors
///
/// Returns [`AdapterError`] when the buffer cannot be represented losslessly.
pub fn to_grid<B: BufferExt + ?Sized>(buffer: &B) -> Result<Grid, AdapterError> {
    buffer.to_ttyshot_grid()
}

/// Returns the buffer's rectangular plain-text snapshot.
///
/// Trailing spaces and blank rows are retained, which makes this suitable for
/// exact [`insta`](https://docs.rs/insta) snapshots.
///
/// # Errors
///
/// Returns [`AdapterError`] when the buffer cannot be represented losslessly.
pub fn snapshot_text<B: BufferExt + ?Sized>(buffer: &B) -> Result<String, AdapterError> {
    Ok(to_grid(buffer)?.plain_text())
}

/// Returns a pretty, deterministic JSON snapshot of the complete grid.
///
/// The JSON includes every cell's symbol, explicit width role, colors, and
/// attributes. This is useful with `insta::assert_snapshot!` when style changes
/// must be visible in the snapshot.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion or JSON serialization fails.
pub fn snapshot_json<B: BufferExt + ?Sized>(buffer: &B) -> Result<String, AdapterError> {
    let grid = to_grid(buffer)?;
    serde_json::to_string_pretty(&grid).map_err(AdapterError::Serialize)
}

/// Renders a supported Ratatui buffer to PNG bytes with the supplied options.
///
/// This is the general image API. Set [`RenderOptions::font_mode`] to
/// [`FontMode::Deterministic`] when the result will be compared as a snapshot.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion or rendering fails.
pub fn render_png<B: BufferExt + ?Sized>(
    buffer: &B,
    options: RenderOptions,
) -> Result<Vec<u8>, AdapterError> {
    let grid = to_grid(buffer)?;
    let renderer = ttyshot_render::Renderer::new(options).map_err(AdapterError::Render)?;
    renderer.render_png(&grid).map_err(AdapterError::Render)
}

/// Renders a supported Ratatui buffer to a standalone SVG document.
///
/// This is the general image API. Set [`RenderOptions::font_mode`] to
/// [`FontMode::Deterministic`] when the result will be compared as a snapshot.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion or rendering fails.
pub fn render_svg<B: BufferExt + ?Sized>(
    buffer: &B,
    options: RenderOptions,
) -> Result<String, AdapterError> {
    let grid = to_grid(buffer)?;
    let renderer = ttyshot_render::Renderer::new(options).map_err(AdapterError::Render)?;
    renderer.render_svg(&grid).map_err(AdapterError::Render)
}

/// Returns renderer options suitable for byte-for-byte image snapshots.
///
/// Unlike [`RenderOptions::default`], these options disable installed system
/// font discovery. Add explicit fallback fonts if a fixture needs deterministic
/// CJK glyph shapes. Cell footprints remain exact without a fallback, but the
/// bundled font does not promise CJK coverage. Emoji and platform-specific ZWJ
/// composition are best-effort and are not portable snapshot content.
#[must_use]
pub fn snapshot_render_options() -> RenderOptions {
    RenderOptions {
        font_mode: FontMode::Deterministic,
        ..RenderOptions::default()
    }
}

/// Renders deterministic PNG bytes for snapshot comparison.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion or rendering fails.
pub fn snapshot_png<B: BufferExt + ?Sized>(buffer: &B) -> Result<Vec<u8>, AdapterError> {
    render_png(buffer, snapshot_render_options())
}

/// Renders a deterministic standalone SVG document for snapshot comparison.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion or rendering fails.
pub fn snapshot_svg<B: BufferExt + ?Sized>(buffer: &B) -> Result<String, AdapterError> {
    render_svg(buffer, snapshot_render_options())
}

/// Atomically writes a deterministic PNG snapshot, creating parent directories as needed.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion, rendering, or writing fails.
pub fn write_snapshot_png<B: BufferExt + ?Sized>(
    buffer: &B,
    path: impl AsRef<Path>,
) -> Result<(), AdapterError> {
    let bytes = snapshot_png(buffer)?;
    write_snapshot(path.as_ref(), &bytes)
}

/// Atomically writes a deterministic SVG snapshot, creating parent directories as needed.
///
/// # Errors
///
/// Returns [`AdapterError`] when conversion, rendering, or writing fails.
pub fn write_snapshot_svg<B: BufferExt + ?Sized>(
    buffer: &B,
    path: impl AsRef<Path>,
) -> Result<(), AdapterError> {
    let svg = snapshot_svg(buffer)?;
    write_snapshot(path.as_ref(), svg.as_bytes())
}

fn write_snapshot(path: &Path, contents: &[u8]) -> Result<(), AdapterError> {
    static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory).map_err(|error| snapshot_io_error(directory, error))?;
    let file_name = path.file_name().ok_or_else(|| {
        snapshot_io_error(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "snapshot path must name a file",
            ),
        )
    })?;

    let (temporary_path, mut temporary_file) = loop {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".ttyshot-{}-{sequence}.tmp", std::process::id()));
        let temporary_path = directory.join(temporary_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => break (temporary_path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(snapshot_io_error(path, error)),
        }
    };

    let write_result = temporary_file
        .write_all(contents)
        .and_then(|()| temporary_file.flush())
        .and_then(|()| temporary_file.sync_all());
    drop(temporary_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(snapshot_io_error(path, error));
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(snapshot_io_error(path, error));
    }
    Ok(())
}

fn snapshot_io_error(path: &Path, error: std::io::Error) -> AdapterError {
    AdapterError::Io {
        path: path.to_path_buf(),
        error,
    }
}

/// A Ratatui buffer could not be converted, rendered, serialized, or written.
#[derive(Debug)]
pub enum AdapterError {
    /// Ratatui permits a zero-sized buffer, while ttyshot grids are non-empty.
    EmptyBuffer,
    /// Public Ratatui fields were used to create inconsistent dimensions and content.
    ContentLength {
        /// Number of cells implied by the buffer area.
        expected: usize,
        /// Number of cells found in the buffer.
        actual: usize,
    },
    /// A symbol did not occupy exactly one or two terminal cells.
    UnsupportedCellWidth {
        /// Zero-based local column.
        x: u16,
        /// Zero-based local row.
        y: u16,
        /// Width reported by the matching Ratatui version.
        width: usize,
    },
    /// A two-cell symbol began in the final column.
    WideAtRightEdge {
        /// Zero-based local column.
        x: u16,
        /// Zero-based local row.
        y: u16,
    },
    /// A cell is deliberately omitted from Ratatui's terminal diff.
    ///
    /// A buffer alone cannot recover the image or prior terminal content behind
    /// such a cell, so converting it as text would not be lossless.
    SkippedCell {
        /// Zero-based local column.
        x: u16,
        /// Zero-based local row.
        y: u16,
    },
    /// The validated conversion was rejected by the core grid.
    Core(GridError),
    /// A JSON snapshot could not be serialized.
    Serialize(serde_json::Error),
    /// The ttyshot renderer rejected the options or could not produce an image.
    Render(ttyshot_render::RenderError),
    /// A snapshot directory or file could not be written.
    Io {
        /// Path that could not be created or written.
        path: PathBuf,
        /// Underlying file-system error.
        error: std::io::Error,
    },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBuffer => formatter.write_str("Ratatui buffer dimensions must be non-zero"),
            Self::ContentLength { expected, actual } => write!(
                formatter,
                "Ratatui buffer contains {actual} cells, but its area requires {expected}"
            ),
            Self::UnsupportedCellWidth { x, y, width } => write!(
                formatter,
                "Ratatui cell ({x}, {y}) has unsupported display width {width}"
            ),
            Self::WideAtRightEdge { x, y } => {
                write!(
                    formatter,
                    "wide Ratatui cell at ({x}, {y}) crosses the right edge"
                )
            }
            Self::SkippedCell { x, y } => write!(
                formatter,
                "Ratatui cell ({x}, {y}) is skipped during terminal diffing and cannot be captured losslessly"
            ),
            Self::Core(error) => write!(formatter, "ttyshot grid rejected the buffer: {error}"),
            Self::Serialize(error) => {
                write!(formatter, "could not serialize grid snapshot: {error}")
            }
            Self::Render(error) => write!(formatter, "could not render buffer: {error}"),
            Self::Io { path, error } => {
                write!(
                    formatter,
                    "could not write snapshot at {}: {error}",
                    path.display()
                )
            }
        }
    }
}

impl Error for AdapterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Core(error) => Some(error),
            Self::Serialize(error) => Some(error),
            Self::Render(error) => Some(error),
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

#[cfg(any(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
struct SourceCell<'a> {
    symbol: &'a str,
    width: usize,
    style: Style,
    skipped: bool,
}

#[cfg(any(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
fn convert_cells<'a>(
    columns: u16,
    rows: u16,
    cells: impl ExactSizeIterator<Item = SourceCell<'a>>,
) -> Result<Grid, AdapterError> {
    if columns == 0 || rows == 0 {
        return Err(AdapterError::EmptyBuffer);
    }

    let expected = usize::from(columns) * usize::from(rows);
    let actual = cells.len();
    if actual != expected {
        return Err(AdapterError::ContentLength { expected, actual });
    }

    let source = cells.collect::<Vec<_>>();
    if let Some(index) = source.iter().position(|cell| cell.skipped) {
        return Err(AdapterError::SkippedCell {
            x: u16::try_from(index % usize::from(columns))
                .expect("column derived from u16 dimensions"),
            y: u16::try_from(index / usize::from(columns))
                .expect("row derived from u16 dimensions"),
        });
    }
    let mut grid = Grid::new(columns, rows).map_err(AdapterError::Core)?;
    for y in 0..rows {
        let mut x = 0;
        while x < columns {
            let index = usize::from(y) * usize::from(columns) + usize::from(x);
            let cell = &source[index];
            let width = match cell.width {
                1 => CellWidth::Single,
                2 => CellWidth::Wide,
                width => return Err(AdapterError::UnsupportedCellWidth { x, y, width }),
            };
            if width == CellWidth::Wide && x + 1 >= columns {
                return Err(AdapterError::WideAtRightEdge { x, y });
            }
            grid.put(x, y, cell.symbol, width, cell.style)
                .map_err(AdapterError::Core)?;
            x += if width == CellWidth::Wide { 2 } else { 1 };
        }
    }
    Ok(grid)
}

#[cfg(any(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // Each field maps to an independent Ratatui modifier.
struct AttributeBits {
    bold: bool,
    dim: bool,
    italic: bool,
    underlined: bool,
    slow_blink: bool,
    rapid_blink: bool,
    reverse: bool,
    hidden: bool,
    crossed_out: bool,
}

#[cfg(any(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
fn attributes(bits: AttributeBits) -> Attributes {
    Attributes {
        bold: bits.bold,
        dim: bits.dim,
        italic: bits.italic,
        underline: if bits.underlined {
            Underline::Single
        } else {
            Underline::None
        },
        // Ratatui's Crossterm backends emit rapid blink after slow blink when
        // both flags are present, so rapid blink is the effective final mode.
        blink: if bits.rapid_blink {
            Blink::Rapid
        } else if bits.slow_blink {
            Blink::Slow
        } else {
            Blink::None
        },
        reverse: bits.reverse,
        hidden: bits.hidden,
        strikethrough: bits.crossed_out,
        ..Attributes::default()
    }
}

#[cfg(feature = "ratatui-0-29")]
mod v29 {
    use ratatui_0_29::{
        buffer::{Buffer, Cell},
        style::{Color as RatatuiColor, Modifier},
    };
    use unicode_width::UnicodeWidthStr;

    use super::{
        AdapterError, AttributeBits, BufferExt, Color, Grid, Rgb, SourceCell, Style, attributes,
        convert_cells,
    };

    impl BufferExt for Buffer {
        fn to_ttyshot_grid(&self) -> Result<Grid, AdapterError> {
            convert_cells(
                self.area.width,
                self.area.height,
                self.content().iter().map(source_cell),
            )
        }
    }

    fn source_cell(cell: &Cell) -> SourceCell<'_> {
        let modifiers = cell.modifier;
        SourceCell {
            symbol: cell.symbol(),
            width: UnicodeWidthStr::width(cell.symbol()),
            style: Style {
                foreground: color(cell.fg),
                background: color(cell.bg),
                underline_color: color(cell.underline_color),
                attributes: attributes(AttributeBits {
                    bold: modifiers.contains(Modifier::BOLD),
                    dim: modifiers.contains(Modifier::DIM),
                    italic: modifiers.contains(Modifier::ITALIC),
                    underlined: modifiers.contains(Modifier::UNDERLINED),
                    slow_blink: modifiers.contains(Modifier::SLOW_BLINK),
                    rapid_blink: modifiers.contains(Modifier::RAPID_BLINK),
                    reverse: modifiers.contains(Modifier::REVERSED),
                    hidden: modifiers.contains(Modifier::HIDDEN),
                    crossed_out: modifiers.contains(Modifier::CROSSED_OUT),
                }),
            },
            skipped: cell.skip,
        }
    }

    fn color(color: RatatuiColor) -> Color {
        match color {
            RatatuiColor::Reset => Color::Default,
            RatatuiColor::Black => Color::Indexed(0),
            RatatuiColor::Red => Color::Indexed(1),
            RatatuiColor::Green => Color::Indexed(2),
            RatatuiColor::Yellow => Color::Indexed(3),
            RatatuiColor::Blue => Color::Indexed(4),
            RatatuiColor::Magenta => Color::Indexed(5),
            RatatuiColor::Cyan => Color::Indexed(6),
            RatatuiColor::Gray => Color::Indexed(7),
            RatatuiColor::DarkGray => Color::Indexed(8),
            RatatuiColor::LightRed => Color::Indexed(9),
            RatatuiColor::LightGreen => Color::Indexed(10),
            RatatuiColor::LightYellow => Color::Indexed(11),
            RatatuiColor::LightBlue => Color::Indexed(12),
            RatatuiColor::LightMagenta => Color::Indexed(13),
            RatatuiColor::LightCyan => Color::Indexed(14),
            RatatuiColor::White => Color::Indexed(15),
            RatatuiColor::Rgb(red, green, blue) => Color::Rgb(Rgb::new(red, green, blue)),
            RatatuiColor::Indexed(index) => Color::Indexed(index),
        }
    }
}

#[cfg(feature = "ratatui-0-30")]
mod v30 {
    use ratatui_0_30::{
        buffer::{Buffer, Cell, CellDiffOption, CellWidth as RatatuiCellWidth},
        style::{Color as RatatuiColor, Modifier},
    };

    use super::{
        AdapterError, AttributeBits, BufferExt, Color, Grid, Rgb, SourceCell, Style, attributes,
        convert_cells,
    };

    impl BufferExt for Buffer {
        fn to_ttyshot_grid(&self) -> Result<Grid, AdapterError> {
            convert_cells(
                self.area.width,
                self.area.height,
                self.content().iter().map(source_cell),
            )
        }
    }

    #[allow(deprecated)] // Ratatui 0.30 still honors the legacy public `skip` field.
    fn source_cell(cell: &Cell) -> SourceCell<'_> {
        let modifiers = cell.modifier;
        SourceCell {
            symbol: cell.symbol(),
            width: usize::from(cell.cell_width()),
            style: Style {
                foreground: color(cell.fg),
                background: color(cell.bg),
                underline_color: color(cell.underline_color),
                attributes: attributes(AttributeBits {
                    bold: modifiers.contains(Modifier::BOLD),
                    dim: modifiers.contains(Modifier::DIM),
                    italic: modifiers.contains(Modifier::ITALIC),
                    underlined: modifiers.contains(Modifier::UNDERLINED),
                    slow_blink: modifiers.contains(Modifier::SLOW_BLINK),
                    rapid_blink: modifiers.contains(Modifier::RAPID_BLINK),
                    reverse: modifiers.contains(Modifier::REVERSED),
                    hidden: modifiers.contains(Modifier::HIDDEN),
                    crossed_out: modifiers.contains(Modifier::CROSSED_OUT),
                }),
            },
            skipped: cell.skip || cell.diff_option == CellDiffOption::Skip,
        }
    }

    fn color(color: RatatuiColor) -> Color {
        match color {
            RatatuiColor::Reset => Color::Default,
            RatatuiColor::Black => Color::Indexed(0),
            RatatuiColor::Red => Color::Indexed(1),
            RatatuiColor::Green => Color::Indexed(2),
            RatatuiColor::Yellow => Color::Indexed(3),
            RatatuiColor::Blue => Color::Indexed(4),
            RatatuiColor::Magenta => Color::Indexed(5),
            RatatuiColor::Cyan => Color::Indexed(6),
            RatatuiColor::Gray => Color::Indexed(7),
            RatatuiColor::DarkGray => Color::Indexed(8),
            RatatuiColor::LightRed => Color::Indexed(9),
            RatatuiColor::LightGreen => Color::Indexed(10),
            RatatuiColor::LightYellow => Color::Indexed(11),
            RatatuiColor::LightBlue => Color::Indexed(12),
            RatatuiColor::LightMagenta => Color::Indexed(13),
            RatatuiColor::LightCyan => Color::Indexed(14),
            RatatuiColor::White => Color::Indexed(15),
            RatatuiColor::Rgb(red, green, blue) => Color::Rgb(Rgb::new(red, green, blue)),
            RatatuiColor::Indexed(index) => Color::Indexed(index),
        }
    }
}

#[cfg(all(test, any(feature = "ratatui-0-29", feature = "ratatui-0-30")))]
mod tests {
    #[cfg(feature = "ratatui-0-29")]
    use std::error::Error as _;

    use super::{AdapterError, snapshot_json, snapshot_png, snapshot_svg, snapshot_text, to_grid};
    #[cfg(feature = "ratatui-0-29")]
    use super::{FontMode, RenderOptions, render_png, write_snapshot_png, write_snapshot_svg};
    use ttyshot_core::{Blink, CellWidth, Color, Grid, Rgb, Underline};

    fn assert_wide_grid(grid: &Grid) {
        assert_eq!(grid.columns(), 4);
        assert_eq!(grid.rows(), 2);
        assert_eq!(grid.get(0, 0).expect("cell").symbol, "A");
        assert_eq!(grid.get(0, 0).expect("cell").width, CellWidth::Single);
        assert_eq!(grid.get(1, 0).expect("cell").symbol, "界");
        assert_eq!(grid.get(1, 0).expect("cell").width, CellWidth::Wide);
        assert_eq!(grid.get(2, 0).expect("cell").symbol, "");
        assert_eq!(grid.get(2, 0).expect("cell").width, CellWidth::Continuation);
        assert_eq!(grid.get(3, 0).expect("cell").symbol, "B");
        assert_eq!(grid.get(0, 1).expect("cell").symbol, "👩‍💻");
        assert_eq!(grid.get(0, 1).expect("cell").width, CellWidth::Wide);
        assert_eq!(grid.get(1, 1).expect("cell").width, CellWidth::Continuation);
        assert_eq!(grid.plain_text(), "A界B\n👩‍💻ok");
    }

    fn assert_style(grid: &Grid) {
        let styled = &grid.get(0, 0).expect("styled cell").style;
        assert_eq!(styled.foreground, Color::Default);
        assert_eq!(styled.background, Color::Indexed(42));
        assert_eq!(styled.underline_color, Color::Rgb(Rgb::new(1, 2, 3)));
        assert!(styled.attributes.bold);
        assert!(styled.attributes.dim);
        assert!(styled.attributes.italic);
        assert_eq!(styled.attributes.underline, Underline::Single);
        assert_eq!(styled.attributes.blink, Blink::Slow);
        assert!(styled.attributes.reverse);
        assert!(styled.attributes.hidden);
        assert!(styled.attributes.strikethrough);

        let rapid = &grid.get(1, 0).expect("rapid cell").style;
        assert_eq!(rapid.foreground, Color::Rgb(Rgb::new(10, 20, 30)));
        assert_eq!(rapid.background, Color::Indexed(6));
        assert_eq!(rapid.underline_color, Color::Indexed(201));
        assert_eq!(rapid.attributes.blink, Blink::Rapid);
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn converts_ratatui_0_29_widths_and_graphemes() {
        use ratatui_0_29::{buffer::Buffer, layout::Rect, style::Style as RatatuiStyle};

        let mut buffer = Buffer::empty(Rect::new(7, 9, 4, 2));
        buffer.set_string(7, 9, "A界B", RatatuiStyle::default());
        buffer.set_string(7, 10, "👩‍💻ok", RatatuiStyle::default());
        assert_wide_grid(&to_grid(&buffer).expect("0.29 buffer converts"));
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn converts_ratatui_0_29_colors_and_modifiers() {
        use ratatui_0_29::{
            buffer::Buffer,
            layout::Rect,
            style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        buffer[(0, 0)].set_symbol("s").set_style(
            RatatuiStyle::default()
                .fg(RatatuiColor::Reset)
                .bg(RatatuiColor::Indexed(42))
                .underline_color(RatatuiColor::Rgb(1, 2, 3))
                .add_modifier(
                    Modifier::BOLD
                        | Modifier::DIM
                        | Modifier::ITALIC
                        | Modifier::UNDERLINED
                        | Modifier::SLOW_BLINK
                        | Modifier::REVERSED
                        | Modifier::HIDDEN
                        | Modifier::CROSSED_OUT,
                ),
        );
        buffer[(1, 0)].set_symbol("r").set_style(
            RatatuiStyle::default()
                .fg(RatatuiColor::Rgb(10, 20, 30))
                .bg(RatatuiColor::Cyan)
                .underline_color(RatatuiColor::Indexed(201))
                .add_modifier(Modifier::RAPID_BLINK),
        );
        assert_style(&to_grid(&buffer).expect("0.29 styles convert"));
    }

    #[cfg(feature = "ratatui-0-30")]
    #[test]
    fn converts_ratatui_0_30_widths_and_graphemes() {
        use ratatui_0_30::{buffer::Buffer, layout::Rect, style::Style as RatatuiStyle};

        let mut buffer = Buffer::empty(Rect::new(7, 9, 4, 2));
        buffer.set_string(7, 9, "A界B", RatatuiStyle::default());
        buffer.set_string(7, 10, "👩‍💻ok", RatatuiStyle::default());
        assert_wide_grid(&to_grid(&buffer).expect("0.30 buffer converts"));
    }

    #[cfg(feature = "ratatui-0-30")]
    #[test]
    fn converts_ratatui_0_30_colors_and_modifiers() {
        use ratatui_0_30::{
            buffer::Buffer,
            layout::Rect,
            style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
        buffer[(0, 0)].set_symbol("s").set_style(
            RatatuiStyle::default()
                .fg(RatatuiColor::Reset)
                .bg(RatatuiColor::Indexed(42))
                .underline_color(RatatuiColor::Rgb(1, 2, 3))
                .add_modifier(
                    Modifier::BOLD
                        | Modifier::DIM
                        | Modifier::ITALIC
                        | Modifier::UNDERLINED
                        | Modifier::SLOW_BLINK
                        | Modifier::REVERSED
                        | Modifier::HIDDEN
                        | Modifier::CROSSED_OUT,
                ),
        );
        buffer[(1, 0)].set_symbol("r").set_style(
            RatatuiStyle::default()
                .fg(RatatuiColor::Rgb(10, 20, 30))
                .bg(RatatuiColor::Cyan)
                .underline_color(RatatuiColor::Indexed(201))
                .add_modifier(Modifier::RAPID_BLINK),
        );
        assert_style(&to_grid(&buffer).expect("0.30 styles convert"));
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn snapshot_helpers_are_rectangular_and_deterministic_for_0_29() {
        use ratatui_0_29::{buffer::Buffer, layout::Rect, style::Style as RatatuiStyle};

        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
        buffer.set_string(0, 0, "hi", RatatuiStyle::default());

        assert_eq!(snapshot_text(&buffer).expect("text snapshot"), "hi \n   ");
        let first = snapshot_json(&buffer).expect("JSON snapshot");
        let second = snapshot_json(&buffer).expect("JSON snapshot");
        assert_eq!(first, second);
        assert!(first.starts_with("{\n  \"columns\": 3,\n  \"rows\": 2,"));
        assert!(first.contains("\"width\": \"single\""));
    }

    #[cfg(feature = "ratatui-0-30")]
    #[test]
    fn snapshot_helpers_are_rectangular_and_deterministic_for_0_30() {
        use ratatui_0_30::{buffer::Buffer, layout::Rect, style::Style as RatatuiStyle};

        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 2));
        buffer.set_string(0, 0, "hi", RatatuiStyle::default());

        assert_eq!(snapshot_text(&buffer).expect("text snapshot"), "hi \n   ");
        let first = snapshot_json(&buffer).expect("JSON snapshot");
        let second = snapshot_json(&buffer).expect("JSON snapshot");
        assert_eq!(first, second);
        assert!(first.starts_with("{\n  \"columns\": 3,\n  \"rows\": 2,"));
        assert!(first.contains("\"width\": \"single\""));
    }

    #[cfg(feature = "ratatui-0-29")]
    fn synthetic_0_29() -> ratatui_0_29::buffer::Buffer {
        use ratatui_0_29::{
            buffer::Buffer,
            layout::Rect,
            style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 2));
        let base = RatatuiStyle::default()
            .fg(RatatuiColor::Rgb(220, 230, 240))
            .bg(RatatuiColor::Indexed(17));
        buffer.set_style(Rect::new(0, 0, 6, 2), base);
        buffer.set_string(
            0,
            0,
            "A界┼█z",
            base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                .underline_color(RatatuiColor::Rgb(240, 80, 160)),
        );
        buffer.set_string(
            0,
            1,
            "colors",
            RatatuiStyle::default()
                .fg(RatatuiColor::LightCyan)
                .bg(RatatuiColor::Rgb(20, 30, 40))
                .add_modifier(Modifier::ITALIC | Modifier::CROSSED_OUT),
        );
        buffer
    }

    #[cfg(feature = "ratatui-0-30")]
    fn synthetic_0_30() -> ratatui_0_30::buffer::Buffer {
        use ratatui_0_30::{
            buffer::Buffer,
            layout::Rect,
            style::{Color as RatatuiColor, Modifier, Style as RatatuiStyle},
        };

        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 2));
        let base = RatatuiStyle::default()
            .fg(RatatuiColor::Rgb(220, 230, 240))
            .bg(RatatuiColor::Indexed(17));
        buffer.set_style(Rect::new(0, 0, 6, 2), base);
        buffer.set_string(
            0,
            0,
            "A界┼█z",
            base.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                .underline_color(RatatuiColor::Rgb(240, 80, 160)),
        );
        buffer.set_string(
            0,
            1,
            "colors",
            RatatuiStyle::default()
                .fg(RatatuiColor::LightCyan)
                .bg(RatatuiColor::Rgb(20, 30, 40))
                .add_modifier(Modifier::ITALIC | Modifier::CROSSED_OUT),
        );
        buffer
    }

    fn assert_cjk_footprint(grid: &Grid) {
        assert_eq!(grid.get(1, 0).expect("CJK cell").symbol, "界");
        assert_eq!(grid.get(1, 0).expect("CJK cell").width, CellWidth::Wide);
        assert_eq!(
            grid.get(2, 0).expect("CJK continuation").width,
            CellWidth::Continuation
        );
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn renders_deterministic_images_from_ratatui_0_29() {
        let buffer = synthetic_0_29();
        let grid = to_grid(&buffer).expect("0.29 buffer converts");
        assert_cjk_footprint(&grid);
        let png = snapshot_png(&buffer).expect("0.29 PNG renders");
        let svg = snapshot_svg(&buffer).expect("0.29 SVG renders");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(svg.starts_with("<svg "));
        assert_eq!(png, snapshot_png(&buffer).expect("PNG repeats"));
        assert_eq!(svg, snapshot_svg(&buffer).expect("SVG repeats"));
    }

    #[cfg(feature = "ratatui-0-30")]
    #[test]
    fn renders_deterministic_images_from_ratatui_0_30() {
        let buffer = synthetic_0_30();
        let grid = to_grid(&buffer).expect("0.30 buffer converts");
        assert_cjk_footprint(&grid);
        let png = snapshot_png(&buffer).expect("0.30 PNG renders");
        let svg = snapshot_svg(&buffer).expect("0.30 SVG renders");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(svg.starts_with("<svg "));
        assert_eq!(png, snapshot_png(&buffer).expect("PNG repeats"));
        assert_eq!(svg, snapshot_svg(&buffer).expect("SVG repeats"));
    }

    #[cfg(all(feature = "ratatui-0-29", feature = "ratatui-0-30"))]
    #[test]
    fn supported_versions_produce_identical_grids_pngs_and_svgs() {
        let old = synthetic_0_29();
        let new = synthetic_0_30();
        assert_eq!(to_grid(&old).unwrap(), to_grid(&new).unwrap());
        assert_eq!(snapshot_png(&old).unwrap(), snapshot_png(&new).unwrap());
        assert_eq!(snapshot_svg(&old).unwrap(), snapshot_svg(&new).unwrap());
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn renderer_errors_are_preserved_as_sources() {
        let error = render_png(
            &synthetic_0_29(),
            RenderOptions {
                scale: 0,
                font_mode: FontMode::Deterministic,
                ..RenderOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error, AdapterError::Render(_)));
        assert!(error.source().is_some());
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn image_snapshot_writers_create_parent_directories() {
        let root = std::env::temp_dir().join(format!(
            "ttyshot-ratatui-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("image-writer")
        ));
        let png_path = root.join("nested/snapshot.png");
        let svg_path = root.join("nested/snapshot.svg");
        let buffer = synthetic_0_29();

        write_snapshot_png(&buffer, &png_path).expect("PNG snapshot writes");
        write_snapshot_svg(&buffer, &svg_path).expect("SVG snapshot writes");
        write_snapshot_png(&buffer, &png_path).expect("PNG snapshot replaces atomically");
        assert_eq!(
            std::fs::read(&png_path).unwrap(),
            snapshot_png(&buffer).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(&svg_path).unwrap(),
            snapshot_svg(&buffer).unwrap()
        );
        assert!(
            std::fs::read_dir(root.join("nested"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("ttyshot-")),
            "successful writes must not leave temporary files"
        );
        std::fs::remove_dir_all(root).expect("temporary snapshots clean up");
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn failed_snapshot_replace_keeps_destination_and_cleans_temporary_file() {
        let root = std::env::temp_dir().join(format!(
            "ttyshot-ratatui-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("failed-writer")
        ));
        let destination = root.join("accepted.png");
        std::fs::create_dir_all(&destination).expect("directory sentinel is created");

        let error = write_snapshot_png(&synthetic_0_29(), &destination).unwrap_err();
        assert!(matches!(error, AdapterError::Io { .. }));
        assert!(destination.is_dir(), "failed rename must keep destination");
        assert!(
            std::fs::read_dir(&root).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("ttyshot-")),
            "failed writes must not leave temporary files"
        );
        std::fs::remove_dir_all(root).expect("temporary snapshots clean up");
    }

    fn assert_skipped(error: &AdapterError, expected_x: u16, expected_y: u16) {
        assert!(
            matches!(
                error,
                AdapterError::SkippedCell { x, y }
                    if *x == expected_x && *y == expected_y
            ),
            "unexpected adapter error: {error}"
        );
    }

    #[cfg(feature = "ratatui-0-29")]
    #[test]
    fn rejects_ratatui_0_29_skipped_cells_including_wide_trailers() {
        use ratatui_0_29::{buffer::Buffer, layout::Rect, style::Style as RatatuiStyle};

        let mut buffer = Buffer::empty(Rect::new(0, 0, 3, 1));
        buffer.set_string(0, 0, "界", RatatuiStyle::default());
        buffer[(1, 0)].set_skip(true);

        assert_skipped(&to_grid(&buffer).unwrap_err(), 1, 0);
    }

    #[cfg(feature = "ratatui-0-30")]
    #[test]
    #[allow(deprecated)] // Both the current and legacy public skip controls remain meaningful.
    fn rejects_ratatui_0_30_current_and_legacy_skipped_cells() {
        use ratatui_0_30::{
            buffer::{Buffer, CellDiffOption},
            layout::Rect,
        };

        let mut current = Buffer::empty(Rect::new(0, 0, 2, 1));
        current[(0, 0)]
            .set_symbol("x")
            .set_diff_option(CellDiffOption::Skip);
        assert_skipped(&to_grid(&current).unwrap_err(), 0, 0);

        let mut legacy = Buffer::empty(Rect::new(0, 0, 2, 1));
        legacy[(1, 0)].set_symbol("y").set_skip(true);
        assert_skipped(&to_grid(&legacy).unwrap_err(), 1, 0);
    }
}
