//! `wasm-bindgen` boundary for deterministic in-process TUI rendering.
//!
//! The binding intentionally exposes no filesystem or process API. Every
//! renderer uses the font embedded by `tuishot-render`, and ANSI input is
//! handled by `tuishot-ansi`'s raw-vte offline backend.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use tuishot_core::{Grid, Theme};
use tuishot_render::{FontMode, RenderOptions, WindowChrome};

const MAX_GRID_CELLS: u64 = 1_000_000;
const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct WasmRenderOptions {
    scale: Option<u8>,
    theme: Option<String>,
    chrome: Option<String>,
}

/// A deterministic renderer backed by the bundled `JetBrains Mono` font.
#[wasm_bindgen(js_name = NativeRenderer)]
pub struct WasmRenderer {
    renderer: tuishot_render::Renderer,
}

#[allow(clippy::missing_errors_doc)] // Errors cross the wasm boundary as documented JS exceptions.
#[wasm_bindgen(js_class = NativeRenderer)]
impl WasmRenderer {
    /// Creates a renderer. The JavaScript facade documents the stable options.
    #[wasm_bindgen(constructor)]
    pub fn new(options: Option<JsValue>) -> Result<WasmRenderer, JsError> {
        let options = decode_options(options)?;
        let scale = options.scale.unwrap_or(1);
        if !(1..=4).contains(&scale) {
            return Err(argument_error("scale must be between 1 and 4"));
        }
        let theme = match options.theme.as_deref().unwrap_or("dark") {
            "dark" => Theme::dark(),
            "light" => Theme::light(),
            value => {
                return Err(argument_error(format!(
                    "invalid theme {value:?}; expected dark or light"
                )));
            }
        };
        let chrome = match options.chrome.as_deref().unwrap_or("none") {
            "none" => WindowChrome::None,
            "macos" => WindowChrome::MacOs,
            value => {
                return Err(argument_error(format!(
                    "invalid chrome {value:?}; expected none or macos"
                )));
            }
        };
        let renderer = tuishot_render::Renderer::new(RenderOptions {
            theme,
            scale,
            chrome,
            // Never discover system fonts. This also keeps browser output
            // byte-identical to Node, Bun, and Deno.
            font_mode: FontMode::Deterministic,
            font: None,
            fallback_fonts: Vec::new(),
            ..RenderOptions::default()
        })
        .map_err(|error| argument_error(format!("invalid render options: {error}")))?;
        Ok(Self { renderer })
    }

    /// Validates and returns the canonical `tuishot-core::Grid` JavaScript object.
    #[wasm_bindgen(js_name = normalizeGrid)]
    pub fn normalize_grid(&self, grid: JsValue) -> Result<JsValue, JsError> {
        let grid = decode_grid(grid)?;
        encode_grid(&grid)
    }

    /// Validates a grid and returns canonical JSON with stable Rust field order.
    #[wasm_bindgen(js_name = gridToJson)]
    pub fn grid_to_json(&self, grid: JsValue) -> Result<String, JsError> {
        canonical_json(&decode_grid(grid)?)
    }

    /// Validates a grid and returns full rectangular text.
    #[wasm_bindgen(js_name = gridToText)]
    pub fn grid_to_text(&self, grid: JsValue) -> Result<String, JsError> {
        Ok(decode_grid(grid)?.plain_text())
    }

    /// Validates and renders a grid to deterministic PNG bytes.
    #[wasm_bindgen(js_name = gridToPng)]
    pub fn grid_to_png(&self, grid: JsValue) -> Result<Vec<u8>, JsError> {
        self.render_png(&decode_grid(grid)?)
    }

    /// Validates and renders a grid to a standalone SVG document.
    #[wasm_bindgen(js_name = gridToSvg)]
    pub fn grid_to_svg(&self, grid: JsValue) -> Result<String, JsError> {
        self.render_svg(&decode_grid(grid)?)
    }

    /// Parses ANSI into the canonical grid JavaScript object.
    #[wasm_bindgen(js_name = ansiToGrid)]
    pub fn ansi_to_grid(&self, ansi: &str, columns: f64, rows: f64) -> Result<JsValue, JsError> {
        encode_grid(&parse_ansi(ansi, columns, rows)?)
    }

    /// Parses ANSI and returns canonical grid JSON.
    #[wasm_bindgen(js_name = ansiToJson)]
    pub fn ansi_to_json(&self, ansi: &str, columns: f64, rows: f64) -> Result<String, JsError> {
        canonical_json(&parse_ansi(ansi, columns, rows)?)
    }

    /// Parses ANSI and returns full rectangular text.
    #[wasm_bindgen(js_name = ansiToText)]
    pub fn ansi_to_text(&self, ansi: &str, columns: f64, rows: f64) -> Result<String, JsError> {
        Ok(parse_ansi(ansi, columns, rows)?.plain_text())
    }

    /// Parses ANSI and renders deterministic PNG bytes.
    #[wasm_bindgen(js_name = ansiToPng)]
    pub fn ansi_to_png(&self, ansi: &str, columns: f64, rows: f64) -> Result<Vec<u8>, JsError> {
        self.render_png(&parse_ansi(ansi, columns, rows)?)
    }

    /// Parses ANSI and renders a standalone SVG document.
    #[wasm_bindgen(js_name = ansiToSvg)]
    pub fn ansi_to_svg(&self, ansi: &str, columns: f64, rows: f64) -> Result<String, JsError> {
        self.render_svg(&parse_ansi(ansi, columns, rows)?)
    }
}

impl WasmRenderer {
    fn render_png(&self, grid: &Grid) -> Result<Vec<u8>, JsError> {
        self.renderer
            .render_png(grid)
            .map_err(|error| argument_error(format!("could not render PNG: {error}")))
    }

    fn render_svg(&self, grid: &Grid) -> Result<String, JsError> {
        self.renderer
            .render_svg(grid)
            .map_err(|error| argument_error(format!("could not render SVG: {error}")))
    }
}

fn decode_options(options: Option<JsValue>) -> Result<WasmRenderOptions, JsError> {
    let Some(options) = options.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(WasmRenderOptions::default());
    };
    serde_wasm_bindgen::from_value(options)
        .map_err(|error| argument_error(format!("invalid renderer options: {error}")))
}

fn decode_grid(value: JsValue) -> Result<Grid, JsError> {
    // Grid's custom Deserialize checks dimensions, cell count, cursor bounds,
    // and every wide/continuation pair.
    let grid: Grid = serde_wasm_bindgen::from_value(value)
        .map_err(|error| argument_error(format!("invalid grid: {error}")))?;
    validate_grid_resources(&grid)?;
    Ok(grid)
}

fn validate_grid_resources(grid: &Grid) -> Result<(), JsError> {
    let cells = u64::from(grid.columns()) * u64::from(grid.rows());
    if cells > MAX_GRID_CELLS {
        return Err(argument_error(format!(
            "grid columns × rows must not exceed {MAX_GRID_CELLS} cells"
        )));
    }
    let bytes = grid
        .cells()
        .iter()
        .try_fold(0_usize, |total, cell| total.checked_add(cell.symbol.len()));
    if bytes.is_none_or(|bytes| bytes > MAX_INPUT_BYTES) {
        return Err(argument_error(format!(
            "grid symbols must not exceed {MAX_INPUT_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn parse_ansi(ansi: &str, columns: f64, rows: f64) -> Result<Grid, JsError> {
    let (columns, rows) = validate_ansi_input(ansi, columns, rows).map_err(argument_error)?;
    tuishot_ansi::vte::ansi_to_grid(ansi, columns, rows)
        .map_err(|error| argument_error(error.to_string()))
}

fn validate_ansi_input(ansi: &str, columns: f64, rows: f64) -> Result<(u16, u16), String> {
    if ansi.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "ANSI input must not exceed {MAX_INPUT_BYTES} UTF-8 bytes"
        ));
    }
    let columns = checked_dimension("columns", columns)?;
    let rows = checked_dimension("rows", rows)?;
    let cells = u64::from(columns) * u64::from(rows);
    if cells > MAX_GRID_CELLS {
        return Err(format!(
            "columns × rows must not exceed {MAX_GRID_CELLS} cells"
        ));
    }
    Ok((columns, rows))
}

fn checked_dimension(name: &str, value: f64) -> Result<u16, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    if value.fract() != 0.0 {
        return Err(format!("{name} must be an integer"));
    }
    if !(1.0..=f64::from(u16::MAX)).contains(&value) {
        return Err(format!("{name} must be between 1 and {}", u16::MAX));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u16)
}

fn encode_grid(grid: &Grid) -> Result<JsValue, JsError> {
    grid.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|error| JsError::new(&format!("could not serialize grid: {error}")))
}

fn canonical_json(grid: &Grid) -> Result<String, JsError> {
    serde_json::to_string(grid)
        .map_err(|error| JsError::new(&format!("could not serialize grid JSON: {error}")))
}

fn argument_error(message: impl AsRef<str>) -> JsError {
    JsError::new(message.as_ref())
}

/// Reports the Rust crate version compiled into the wasm binary.
#[wasm_bindgen(js_name = version)]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn renderer() -> tuishot_render::Renderer {
        tuishot_render::Renderer::new(RenderOptions {
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap()
    }

    #[test]
    fn vte_grid_render_is_deterministic() {
        let grid = tuishot_ansi::vte::ansi_to_grid("\x1b[1;38;5;196mwide 界\x1b[0m\nsecond", 16, 3)
            .unwrap();
        let renderer = renderer();
        let first = renderer.render_png(&grid).unwrap();
        let second = renderer.render_png(&grid).unwrap();
        assert_eq!(first, second);
        assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            format!("{:x}", Sha256::digest(&first)),
            format!("{:x}", Sha256::digest(&second))
        );
        assert!(renderer.render_svg(&grid).unwrap().starts_with("<svg "));
        assert_eq!(grid.plain_text().lines().count(), 3);
        serde_json::from_str::<Grid>(&serde_json::to_string(&grid).unwrap()).unwrap();
    }

    #[test]
    fn dimensions_and_resource_limits_are_checked() {
        assert!(checked_dimension("columns", 0.0).is_err());
        assert!(checked_dimension("columns", 65_536.0).is_err());
        assert_eq!(checked_dimension("columns", 80.0).unwrap(), 80);
        assert!(validate_ansi_input("", 1_001.0, 1_000.0).is_err());
    }
}
