//! In-process Node-API bindings for ANSI parsing and deterministic rendering.

use napi::bindgen_prelude::Buffer;
use napi::{Error, Status};
use napi_derive::napi;
use serde_json::Value;
use tuishot_core::{Grid, Theme};
use tuishot_render::{FontMode, RenderOptions, Renderer, WindowChrome};

const DEFAULT_SCALE: f64 = 1.0;
const MAX_GRID_CELLS: u64 = 1_000_000;
const MAX_GRID_SYMBOL_BYTES: usize = 16 * 1024 * 1024;

/// Options for rendering a static ANSI string.
#[napi(object)]
pub struct AnsiRenderOptions {
    /// Terminal columns. Must be a finite integer from 1 through 65535.
    pub cols: f64,
    /// Terminal rows. Must be a finite integer from 1 through 65535.
    pub rows: f64,
    /// Integer raster scale from 1 through 4.
    pub scale: Option<f64>,
    /// Built-in color theme: `dark` or `light`.
    pub theme: Option<String>,
    /// Window decoration: `none` or `macos`.
    pub chrome: Option<String>,
    /// Requested outputs. Defaults to all four formats.
    pub formats: Option<Vec<String>>,
}

/// Options for rendering an already validated grid.
#[napi(object)]
pub struct GridRenderOptions {
    /// Integer raster scale from 1 through 4.
    pub scale: Option<f64>,
    /// Built-in color theme: `dark` or `light`.
    pub theme: Option<String>,
    /// Window decoration: `none` or `macos`.
    pub chrome: Option<String>,
    /// Requested outputs. Defaults to all four formats.
    pub formats: Option<Vec<String>>,
}

/// Outputs requested from a render operation.
#[napi(object)]
pub struct RenderResult {
    /// Deterministic PNG bytes.
    pub png: Option<Buffer>,
    /// Standalone SVG document.
    pub svg: Option<String>,
    /// Full rectangular plain-text grid.
    pub txt: Option<String>,
    /// Exact `tuishot-core::Grid` Serde object.
    pub json: Option<Value>,
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(clippy::struct_excessive_bools)] // Each flag maps directly to one requested output.
struct Formats {
    png: bool,
    svg: bool,
    txt: bool,
    json: bool,
}

impl Formats {
    const fn all() -> Self {
        Self {
            png: true,
            svg: true,
            txt: true,
            json: true,
        }
    }

    const fn needs_renderer(self) -> bool {
        self.png || self.svg
    }
}

struct ValidatedOptions {
    scale: u8,
    theme: Theme,
    chrome: WindowChrome,
    formats: Formats,
}

/// Parse ANSI in-process and render the resulting terminal grid.
///
/// Bare LF is treated as a newline at column zero. Existing CRLF is preserved.
///
/// # Errors
/// Returns a JavaScript argument error for invalid dimensions, options, or
/// formats, and for output dimensions that exceed renderer limits.
#[napi(js_name = "renderAnsi")]
pub fn render_ansi(ansi: String, options: AnsiRenderOptions) -> napi::Result<RenderResult> {
    let AnsiRenderOptions {
        cols,
        rows,
        scale,
        theme,
        chrome,
        formats,
    } = options;
    let columns = checked_u16("cols", cols)?;
    let rows = checked_u16("rows", rows)?;
    validate_ansi_area(columns, rows)?;
    let validated = validate_options(
        scale,
        theme.as_deref(),
        chrome.as_deref(),
        formats.as_deref(),
    )?;
    let grid = tuishot_ansi::ansi_to_grid(ansi, columns, rows)
        .map_err(|error| invalid_argument(error.to_string()))?;
    render(&grid, validated)
}

/// Validate an exact `tuishot-core::Grid` Serde object and render it in-process.
///
/// # Errors
/// Returns a JavaScript argument error when the grid violates any Rust wire
/// invariant, exceeds the cell or symbol-byte resource limits, an option is
/// invalid, or output dimensions exceed renderer limits.
#[napi(js_name = "renderGrid")]
pub fn render_grid(grid: Value, options: Option<GridRenderOptions>) -> napi::Result<RenderResult> {
    validate_grid_value_area(&grid)?;
    // This intentionally uses Grid's custom Deserialize implementation. It
    // checks dimensions, cell count, cursor bounds, and wide-cell pair invariants.
    let grid: Grid = serde_json::from_value(grid)
        .map_err(|error| invalid_argument(format!("invalid grid: {error}")))?;
    validate_grid_symbols(&grid)?;
    let options = options.unwrap_or(GridRenderOptions {
        scale: None,
        theme: None,
        chrome: None,
        formats: None,
    });
    let validated = validate_options(
        options.scale,
        options.theme.as_deref(),
        options.chrome.as_deref(),
        options.formats.as_deref(),
    )?;
    render(&grid, validated)
}

fn render(grid: &Grid, options: ValidatedOptions) -> napi::Result<RenderResult> {
    let renderer = if options.formats.needs_renderer() {
        Some(
            Renderer::new(RenderOptions {
                theme: options.theme,
                scale: options.scale,
                chrome: options.chrome,
                font_mode: FontMode::Deterministic,
                ..RenderOptions::default()
            })
            .map_err(|error| invalid_argument(format!("invalid render options: {error}")))?,
        )
    } else {
        None
    };

    let png = if options.formats.png {
        Some(Buffer::from(
            renderer
                .as_ref()
                .expect("renderer exists for PNG")
                .render_png(grid)
                .map_err(|error| invalid_argument(format!("could not render PNG: {error}")))?,
        ))
    } else {
        None
    };
    let svg = if options.formats.svg {
        Some(
            renderer
                .as_ref()
                .expect("renderer exists for SVG")
                .render_svg(grid)
                .map_err(|error| invalid_argument(format!("could not render SVG: {error}")))?,
        )
    } else {
        None
    };
    let txt = options.formats.txt.then(|| grid.plain_text());
    let json = if options.formats.json {
        Some(
            serde_json::to_value(grid)
                .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?,
        )
    } else {
        None
    };

    Ok(RenderResult {
        png,
        svg,
        txt,
        json,
    })
}

fn validate_options(
    scale: Option<f64>,
    theme: Option<&str>,
    chrome: Option<&str>,
    formats: Option<&[String]>,
) -> napi::Result<ValidatedOptions> {
    let scale = checked_scale(scale.unwrap_or(DEFAULT_SCALE))?;
    let theme = match theme.unwrap_or("dark") {
        "dark" => Theme::dark(),
        "light" => Theme::light(),
        value => {
            return Err(invalid_argument(format!(
                "invalid theme {value:?}; expected dark or light"
            )));
        }
    };
    let chrome = match chrome.unwrap_or("none") {
        "none" => WindowChrome::None,
        "macos" => WindowChrome::MacOs,
        value => {
            return Err(invalid_argument(format!(
                "invalid chrome {value:?}; expected none or macos"
            )));
        }
    };
    let formats = parse_formats(formats)?;
    Ok(ValidatedOptions {
        scale,
        theme,
        chrome,
        formats,
    })
}

fn checked_u16(name: &str, value: f64) -> napi::Result<u16> {
    if !value.is_finite() {
        return Err(invalid_argument(format!("{name} must be finite")));
    }
    if value.fract() != 0.0 {
        return Err(invalid_argument(format!("{name} must be an integer")));
    }
    if !(1.0..=f64::from(u16::MAX)).contains(&value) {
        return Err(invalid_argument(format!(
            "{name} must be between 1 and {}",
            u16::MAX
        )));
    }
    // Bounds and integrality are checked above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u16)
}

fn validate_ansi_area(columns: u16, rows: u16) -> napi::Result<()> {
    let cells = u64::from(columns) * u64::from(rows);
    if cells > MAX_GRID_CELLS {
        return Err(invalid_argument(format!(
            "cols × rows must not exceed {MAX_GRID_CELLS} cells"
        )));
    }
    Ok(())
}

fn validate_grid_value_area(grid: &Value) -> napi::Result<()> {
    let Some(columns) = grid.get("columns").and_then(Value::as_u64) else {
        return Ok(());
    };
    let Some(rows) = grid.get("rows").and_then(Value::as_u64) else {
        return Ok(());
    };
    if columns
        .checked_mul(rows)
        .is_none_or(|cells| cells > MAX_GRID_CELLS)
    {
        return Err(invalid_argument(format!(
            "grid columns × rows must not exceed {MAX_GRID_CELLS} cells"
        )));
    }
    Ok(())
}

fn validate_grid_symbols(grid: &Grid) -> napi::Result<()> {
    let symbol_bytes = grid
        .cells()
        .iter()
        .try_fold(0_usize, |total, cell| total.checked_add(cell.symbol.len()));
    if symbol_bytes.is_none_or(|bytes| bytes > MAX_GRID_SYMBOL_BYTES) {
        return Err(invalid_argument(format!(
            "grid symbols must not exceed {MAX_GRID_SYMBOL_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn checked_scale(value: f64) -> napi::Result<u8> {
    if !value.is_finite() {
        return Err(invalid_argument("scale must be finite"));
    }
    if value.fract() != 0.0 {
        return Err(invalid_argument("scale must be an integer"));
    }
    if !(1.0..=4.0).contains(&value) {
        return Err(invalid_argument("scale must be between 1 and 4"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u8)
}

fn parse_formats(formats: Option<&[String]>) -> napi::Result<Formats> {
    let Some(formats) = formats else {
        return Ok(Formats::all());
    };
    if formats.is_empty() {
        return Err(invalid_argument("formats must not be empty"));
    }
    let mut selected = Formats::default();
    for format in formats {
        match format.as_str() {
            "png" => selected.png = true,
            "svg" => selected.svg = true,
            "txt" => selected.txt = true,
            "json" => selected.json = true,
            value => {
                return Err(invalid_argument(format!(
                    "invalid format {value:?}; expected png, svg, txt, or json"
                )));
            }
        }
    }
    Ok(selected)
}

fn invalid_argument(message: impl Into<String>) -> Error {
    Error::new(Status::InvalidArg, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_validation_rejects_non_finite_fractional_and_out_of_range_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(checked_u16("cols", value).is_err());
            assert!(checked_scale(value).is_err());
        }
        assert!(checked_u16("cols", 1.5).is_err());
        assert!(checked_u16("cols", 0.0).is_err());
        assert!(checked_u16("cols", 65_536.0).is_err());
        assert!(checked_scale(2.5).is_err());
        assert!(checked_scale(0.0).is_err());
        assert!(checked_scale(5.0).is_err());
        assert_eq!(checked_u16("cols", 65_535.0).unwrap(), u16::MAX);
        assert_eq!(checked_scale(4.0).unwrap(), 4);
        assert!(validate_ansi_area(u16::MAX, u16::MAX).is_err());
        assert!(validate_ansi_area(1_000, 1_000).is_ok());
    }

    #[test]
    fn grid_resource_limits_reject_excessive_area_and_symbol_bytes() {
        let oversized = serde_json::json!({ "columns": 1_001, "rows": 1_000 });
        assert!(validate_grid_value_area(&oversized).is_err());

        let mut grid = Grid::new(1, 1).unwrap();
        grid.put(
            0,
            0,
            "x".repeat(MAX_GRID_SYMBOL_BYTES + 1),
            tuishot_core::CellWidth::Single,
            tuishot_core::Style::default(),
        )
        .unwrap();
        assert!(validate_grid_symbols(&grid).is_err());
    }

    #[test]
    fn empty_and_unknown_formats_are_rejected() {
        assert!(parse_formats(Some(&[])).is_err());
        assert!(parse_formats(Some(&["pdf".to_owned()])).is_err());
    }
}
