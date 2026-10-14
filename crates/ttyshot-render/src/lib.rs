#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_arguments
)]
// Pixel coordinates are validated against the output canvas before low-level casts.

//! Deterministic terminal cell-grid rendering to PNG and SVG.
//!
//! Terminal drawing characters are emitted as integer geometry instead of font
//! glyphs. The bundled `JetBrains Mono Regular` font is licensed under the SIL
//! Open Font License 1.1; its exact license is in `assets/OFL-JetBrainsMono.txt`.
//!
//! Emoji rendering is best-effort. Color emoji and platform-specific emoji
//! composition are intentionally not promised. Supply a monochrome fallback
//! font when a fixture needs stable non-ASCII glyph coverage.

mod canvas;
mod font;
mod geometry;
mod svg_glyph;

use canvas::Canvas;
use font::LoadedFont;
use geometry::Shape;
use std::{error::Error, fmt, path::PathBuf};
use svg_glyph::{SvgGlyphCatalog, select_font};
use ttyshot_core::{
    Attributes, CellWidth, ColorRole, CursorShape, Frame, Grid, Ideogram, Rgb, Theme, Underline,
};

pub use font::{FontMode, FontSource};
pub use ttyshot_core as core;

const FONT_BYTES: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
/// Exact SIL Open Font License text shipped with the bundled font.
pub const BUNDLED_FONT_LICENSE: &str = include_str!("../assets/OFL-JetBrainsMono.txt");
/// Machine-readable provenance for all bundled non-Rust font assets.
pub const BUNDLED_FONT_MANIFEST: &str = include_str!("../assets/FONT-LICENSES.toml");
/// `JetBrains Mono` version bundled for deterministic output.
pub const BUNDLED_FONT_VERSION: &str = "2.304";
/// SHA-256 of the exact bundled `JetBrainsMono-Regular.ttf` bytes.
pub const BUNDLED_FONT_SHA256: &str =
    "a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f";
/// Upstream release containing the bundled font.
pub const BUNDLED_FONT_SOURCE: &str =
    "https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304";

/// Maximum width or height accepted by the raster renderer.
///
/// The ceiling bounds coordinate conversion and rejects unreasonable canvases
/// before allocation. SVG uses the same layout ceiling for parity with PNG.
pub const MAX_CANVAS_DIMENSION: u32 = 32_768;
/// Maximum accepted canvas area (64 Mi pixels, or 256 MiB of RGBA storage).
pub const MAX_CANVAS_PIXELS: u64 = 64 * 1024 * 1024;
/// Maximum logical font size. At the maximum 4x scale this limits a glyph raster to 2,048 px.
pub const MAX_FONT_SIZE: f32 = 512.0;

/// Insets around the terminal grid, in logical pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Padding {
    /// Top inset.
    pub top: u16,
    /// Right inset.
    pub right: u16,
    /// Bottom inset.
    pub bottom: u16,
    /// Left inset.
    pub left: u16,
}

impl Padding {
    /// Creates equal insets on all sides.
    #[must_use]
    pub const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self::all(16)
    }
}

/// Optional terminal window decoration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowChrome {
    /// Render only the padded terminal surface.
    #[default]
    None,
    /// Render a 28-pixel title bar with three traffic-light controls.
    MacOs,
}

/// Renderer configuration. Pixel sizes are logical and multiplied by `scale`.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Terminal theme, including defaults and ANSI colors.
    pub theme: Theme,
    /// Width of one cell.
    pub cell_width: u16,
    /// Height of one cell.
    pub cell_height: u16,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Integer output scale. Both 1 and 2 are seam-tested.
    pub scale: u8,
    /// Space around the grid.
    pub padding: Padding,
    /// Optional window title bar.
    pub chrome: WindowChrome,
    /// Add a deterministic soft window shadow and transparent outer margin.
    pub shadow: bool,
    /// Disable platform discovery for byte-stable fixtures and goldens.
    pub font_mode: FontMode,
    /// Optional primary font file. The bundled font is used when absent.
    pub font: Option<FontSource>,
    /// Ordered user-supplied fallback fonts, before any system fallback.
    pub fallback_fonts: Vec<FontSource>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            theme: Theme::dark(),
            cell_width: 10,
            cell_height: 20,
            font_size: 15.0,
            scale: 1,
            padding: Padding::default(),
            chrome: WindowChrome::None,
            shadow: false,
            font_mode: FontMode::System,
            font: None,
            fallback_fonts: Vec::new(),
        }
    }
}

/// A validated renderer with parsed fonts.
pub struct Renderer {
    options: RenderOptions,
    fonts: Vec<LoadedFont>,
}

impl Renderer {
    /// Loads fonts and validates an option set.
    ///
    /// # Errors
    /// Returns an error for zero dimensions, a non-finite or excessive font
    /// size, an unsupported scale, or an unreadable/invalid fallback font.
    pub fn new(options: RenderOptions) -> Result<Self, RenderError> {
        if options.cell_width == 0
            || options.cell_height == 0
            || options.font_size <= 0.0
            || !options.font_size.is_finite()
            || options.font_size > MAX_FONT_SIZE
        {
            return Err(RenderError::InvalidOptions(
                "cell sizes must be positive and font size must be finite and at most 512",
            ));
        }
        if options.scale == 0 || options.scale > 4 {
            return Err(RenderError::InvalidOptions("scale must be between 1 and 4"));
        }
        let mut fonts = Vec::new();
        if let Some(source) = &options.font {
            fonts.push(LoadedFont::from_source(source)?);
        } else {
            fonts.push(LoadedFont::load(
                std::sync::Arc::new(FONT_BYTES.to_vec()),
                0,
                "bundled JetBrains Mono 2.304".to_owned(),
            )?);
        }
        for source in &options.fallback_fonts {
            fonts.push(LoadedFont::from_source(source)?);
        }
        if options.font_mode == FontMode::System {
            fonts.extend(font::system_fallbacks());
        }
        Ok(Self { options, fonts })
    }

    /// Returns this renderer's options.
    #[must_use]
    pub const fn options(&self) -> &RenderOptions {
        &self.options
    }

    /// Reports the selected font source for a character.
    #[must_use]
    pub fn font_source_for(&self, character: char) -> &str {
        &self.fonts[select_font(&self.fonts, character)].source
    }

    /// Renders a grid as deterministic RGBA PNG bytes.
    ///
    /// # Errors
    /// Returns an error when dimensions overflow or PNG encoding fails.
    pub fn render_png(&self, grid: &Grid) -> Result<Vec<u8>, RenderError> {
        let layout = self.layout(grid)?;
        let mut canvas = Canvas::new(layout.width, layout.height)?;
        self.paint_shell(&mut canvas, layout);
        self.paint_cells(&mut canvas, grid, layout);
        self.paint_cursor(&mut canvas, grid, layout);
        encode_png(&canvas)
    }

    /// Renders a grid as a deterministic standalone SVG document.
    ///
    /// Procedural glyphs remain vector geometry. Text references reusable
    /// outline paths selected from the same font chain as the PNG renderer;
    /// complete font files are never embedded.
    ///
    /// # Errors
    /// Returns an error when output dimensions overflow.
    pub fn render_svg(&self, grid: &Grid) -> Result<String, RenderError> {
        let layout = self.layout(grid)?;
        let mut svg = String::new();
        use fmt::Write as _;
        write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#, layout.width, layout.height, layout.width, layout.height).unwrap();
        let glyphs = SvgGlyphCatalog::build(grid, &self.fonts);
        svg.push_str(glyphs.definitions());
        self.svg_shell(&mut svg, layout);
        self.svg_backgrounds(&mut svg, grid, layout);
        self.svg_cells(&mut svg, grid, layout, &glyphs);
        self.svg_cursor(&mut svg, grid, layout);
        svg.push_str("</svg>");
        Ok(svg)
    }

    fn layout(&self, grid: &Grid) -> Result<Layout, RenderError> {
        let scale = u32::from(self.options.scale);
        let cell_width = u32::from(self.options.cell_width)
            .checked_mul(scale)
            .ok_or(RenderError::DimensionOverflow)?;
        let cell_height = u32::from(self.options.cell_height)
            .checked_mul(scale)
            .ok_or(RenderError::DimensionOverflow)?;
        let chrome = if self.options.chrome == WindowChrome::MacOs {
            28 * scale
        } else {
            0
        };
        let shadow = if self.options.shadow { 16 * scale } else { 0 };
        let horizontal_padding = u32::from(self.options.padding.left)
            .checked_add(u32::from(self.options.padding.right))
            .and_then(|value| value.checked_mul(scale))
            .ok_or(RenderError::DimensionOverflow)?;
        let vertical_padding = u32::from(self.options.padding.top)
            .checked_add(u32::from(self.options.padding.bottom))
            .and_then(|value| value.checked_mul(scale))
            .ok_or(RenderError::DimensionOverflow)?;
        let grid_x = shadow
            .checked_add(
                u32::from(self.options.padding.left)
                    .checked_mul(scale)
                    .ok_or(RenderError::DimensionOverflow)?,
            )
            .ok_or(RenderError::DimensionOverflow)?;
        let grid_y = shadow
            .checked_add(chrome)
            .and_then(|value| {
                u32::from(self.options.padding.top)
                    .checked_mul(scale)
                    .and_then(|padding| value.checked_add(padding))
            })
            .ok_or(RenderError::DimensionOverflow)?;
        let surface_width = u32::from(grid.columns())
            .checked_mul(cell_width)
            .and_then(|value| value.checked_add(horizontal_padding))
            .ok_or(RenderError::DimensionOverflow)?;
        let surface_height = u32::from(grid.rows())
            .checked_mul(cell_height)
            .and_then(|value| value.checked_add(vertical_padding))
            .and_then(|value| value.checked_add(chrome))
            .ok_or(RenderError::DimensionOverflow)?;
        let width = surface_width
            .checked_add(shadow * 2)
            .ok_or(RenderError::DimensionOverflow)?;
        let height = surface_height
            .checked_add(shadow * 2)
            .ok_or(RenderError::DimensionOverflow)?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(RenderError::DimensionOverflow)?;
        if width > MAX_CANVAS_DIMENSION
            || height > MAX_CANVAS_DIMENSION
            || pixels > MAX_CANVAS_PIXELS
        {
            return Err(RenderError::CanvasTooLarge { width, height });
        }
        Ok(Layout {
            width,
            height,
            surface_x: shadow,
            surface_y: shadow,
            surface_width,
            surface_height,
            grid_x,
            grid_y,
            cell_width,
            cell_height,
            chrome,
        })
    }

    fn paint_shell(&self, canvas: &mut Canvas, layout: Layout) {
        if self.options.shadow {
            let scale = i32::from(self.options.scale);
            for logical_spread in (2..=12).rev() {
                let spread = logical_spread * scale;
                let alpha = u8::try_from(3 + (12 - logical_spread)).unwrap_or(3);
                canvas.fill_rect(
                    layout.surface_x as i32 - spread,
                    layout.surface_y as i32 + 4 * scale - spread,
                    layout.surface_width as i32 + spread * 2,
                    layout.surface_height as i32 + spread * 2,
                    Rgb::new(0, 0, 0),
                    alpha,
                );
            }
        }
        canvas.fill_rect(
            layout.surface_x as i32,
            layout.surface_y as i32,
            layout.surface_width as i32,
            layout.surface_height as i32,
            self.options.theme.background,
            255,
        );
        if layout.chrome > 0 {
            let bar = adjust(
                self.options.theme.background,
                if is_dark(self.options.theme.background) {
                    18
                } else {
                    -12
                },
            );
            canvas.fill_rect(
                layout.surface_x as i32,
                layout.surface_y as i32,
                layout.surface_width as i32,
                layout.chrome as i32,
                bar,
                255,
            );
            for (index, color) in [
                Rgb::new(255, 95, 86),
                Rgb::new(255, 189, 46),
                Rgb::new(39, 201, 63),
            ]
            .into_iter()
            .enumerate()
            {
                let x = layout.surface_x as i32
                    + (14 + index as i32 * 20) * i32::from(self.options.scale);
                let y = layout.surface_y as i32 + 10 * i32::from(self.options.scale);
                canvas.fill_rect(
                    x,
                    y,
                    8 * i32::from(self.options.scale),
                    8 * i32::from(self.options.scale),
                    color,
                    255,
                );
            }
        }
    }

    fn paint_cells(&self, canvas: &mut Canvas, grid: &Grid, layout: Layout) {
        // Paint every background before any foreground. A continuation cell
        // must never erase the second half of a wide glyph.
        for y in 0..grid.rows() {
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (_, background) = colors(&self.options.theme, cell.style);
                let px = layout.grid_x + u32::from(x) * layout.cell_width;
                let py = layout.grid_y + u32::from(y) * layout.cell_height;
                canvas.fill_rect(
                    px as i32,
                    py as i32,
                    layout.cell_width as i32,
                    layout.cell_height as i32,
                    background,
                    255,
                );
            }
        }
        for y in 0..grid.rows() {
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (foreground, _) = colors(&self.options.theme, cell.style);
                let px = layout.grid_x + u32::from(x) * layout.cell_width;
                let py = layout.grid_y + u32::from(y) * layout.cell_height;
                if cell.width == CellWidth::Continuation {
                    continue;
                }
                let span = if cell.width == CellWidth::Wide {
                    layout.cell_width * 2
                } else {
                    layout.cell_width
                };
                let attributes = cell.style.attributes;
                if !attributes.hidden {
                    let mut chars = cell.symbol.chars();
                    if let (Some(character), None) = (chars.next(), chars.next()) {
                        if let Some(shapes) =
                            geometry::shapes(character, span as i32, layout.cell_height as i32)
                        {
                            let alpha = if attributes.dim { 153 } else { 255 };
                            for shape in &shapes {
                                canvas.shape(shape, px as i32, py as i32, foreground, alpha);
                            }
                        } else {
                            self.paint_text(
                                canvas,
                                &cell.symbol,
                                px,
                                py,
                                span,
                                layout.cell_height,
                                foreground,
                                attributes,
                            );
                        }
                    } else {
                        self.paint_text(
                            canvas,
                            &cell.symbol,
                            px,
                            py,
                            span,
                            layout.cell_height,
                            foreground,
                            attributes,
                        );
                    }
                    self.paint_decorations(
                        canvas,
                        px,
                        py,
                        span,
                        layout.cell_height,
                        foreground,
                        cell.style.underline_color,
                        attributes,
                    );
                }
            }
        }
    }

    fn paint_text(
        &self,
        canvas: &mut Canvas,
        text: &str,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: Rgb,
        attributes: Attributes,
    ) {
        let size = self.options.font_size * f32::from(self.options.scale);
        let alpha = if attributes.dim { 153 } else { 255 };
        for character in text
            .chars()
            .filter(|character| *character != '\u{200d}' && *character != '\u{fe0f}')
        {
            let font = &self.fonts[select_font(&self.fonts, character)];
            let glyph = font.rasterize(character, size);
            let center = x as f32 + width as f32 / 2.0;
            let draw_x = (center - glyph.advance_width / 2.0).round() as i32 + glyph.xmin;
            let baseline = y as i32 + (height as f32 / 2.0 + size / 2.0).round() as i32
                - i32::from(self.options.scale);
            let draw_y = baseline - glyph.height as i32 - glyph.ymin;
            canvas.mask(
                draw_x,
                draw_y,
                glyph.width,
                glyph.height,
                &glyph.bitmap,
                color,
                alpha,
                (x as i32, y as i32, (x + width) as i32, (y + height) as i32),
                if attributes.bold {
                    i32::from(self.options.scale)
                } else {
                    0
                },
                if attributes.italic {
                    3 * i32::from(self.options.scale)
                } else {
                    0
                },
            );
        }
    }

    fn paint_decorations(
        &self,
        canvas: &mut Canvas,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        foreground: Rgb,
        underline_color: ttyshot_core::Color,
        attributes: Attributes,
    ) {
        let scale = i32::from(self.options.scale);
        let x = x as i32;
        let y = y as i32;
        let width = width as i32;
        let height = height as i32;
        let underline = self
            .options
            .theme
            .resolve(underline_color, ColorRole::Foreground);
        match attributes.underline {
            Underline::None => {}
            Underline::Single => {
                canvas.fill_rect(x, y + height - 2 * scale, width, scale, underline, 255)
            }
            Underline::Double => {
                canvas.fill_rect(x, y + height - 4 * scale, width, scale, underline, 255);
                canvas.fill_rect(x, y + height - 2 * scale, width, scale, underline, 255);
            }
            Underline::Curly => {
                let run = 2 * scale;
                for offset in (0..width).step_by(run as usize) {
                    let wave = if (offset / run).rem_euclid(2) == 0 {
                        0
                    } else {
                        scale
                    };
                    canvas.fill_rect(
                        x + offset,
                        y + height - 3 * scale + wave,
                        run.min(width - offset),
                        scale,
                        underline,
                        255,
                    );
                }
            }
            Underline::Dotted => {
                for offset in (0..width).step_by((2 * scale) as usize) {
                    canvas.fill_rect(
                        x + offset,
                        y + height - 2 * scale,
                        scale,
                        scale,
                        underline,
                        255,
                    );
                }
            }
            Underline::Dashed => {
                for offset in (0..width).step_by((5 * scale) as usize) {
                    canvas.fill_rect(
                        x + offset,
                        y + height - 2 * scale,
                        (3 * scale).min(width - offset),
                        scale,
                        underline,
                        255,
                    );
                }
            }
        }
        if attributes.strikethrough {
            canvas.fill_rect(x, y + height / 2, width, scale, foreground, 255);
        }
        if attributes.overline {
            canvas.fill_rect(x, y, width, scale, foreground, 255);
        }
        match attributes.frame {
            Frame::None => {}
            Frame::Framed => stroke_rect(canvas, x, y, width, height, scale, foreground),
            Frame::Encircled => {
                canvas.ellipse_outline(x, y, width, height, scale, foreground, 255);
            }
        }
        match attributes.ideogram {
            Ideogram::None => {}
            Ideogram::Underline => {
                canvas.fill_rect(x, y + height - scale, width, scale, foreground, 255)
            }
            Ideogram::DoubleUnderline => {
                canvas.fill_rect(x, y + height - scale, width, scale, foreground, 255);
                canvas.fill_rect(x, y + height - 3 * scale, width, scale, foreground, 255);
            }
            Ideogram::Overline => canvas.fill_rect(x, y, width, scale, foreground, 255),
            Ideogram::DoubleOverline => {
                canvas.fill_rect(x, y, width, scale, foreground, 255);
                canvas.fill_rect(x, y + 2 * scale, width, scale, foreground, 255);
            }
            Ideogram::Stress => {
                canvas.fill_rect(
                    x + width / 2 - scale / 2,
                    y,
                    scale,
                    3 * scale,
                    foreground,
                    255,
                );
            }
        }
    }

    fn paint_cursor(&self, canvas: &mut Canvas, grid: &Grid, layout: Layout) {
        let Some(cursor) = grid
            .cursor
            .filter(|cursor| cursor.visible && cursor.x < grid.columns() && cursor.y < grid.rows())
        else {
            return;
        };
        let x = (layout.grid_x + u32::from(cursor.x) * layout.cell_width) as i32;
        let y = (layout.grid_y + u32::from(cursor.y) * layout.cell_height) as i32;
        let scale = i32::from(self.options.scale);
        match cursor.shape {
            CursorShape::Block => canvas.fill_rect(
                x,
                y,
                layout.cell_width as i32,
                layout.cell_height as i32,
                self.options.theme.cursor,
                128,
            ),
            CursorShape::Bar => canvas.fill_rect(
                x,
                y,
                2 * scale,
                layout.cell_height as i32,
                self.options.theme.cursor,
                255,
            ),
            CursorShape::Underline => canvas.fill_rect(
                x,
                y + layout.cell_height as i32 - 2 * scale,
                layout.cell_width as i32,
                2 * scale,
                self.options.theme.cursor,
                255,
            ),
        }
    }

    fn svg_shell(&self, svg: &mut String, layout: Layout) {
        use fmt::Write as _;
        if self.options.shadow {
            let scale = i32::from(self.options.scale);
            for logical_spread in (2..=12).rev() {
                let spread = logical_spread * scale;
                let alpha = u8::try_from(3 + (12 - logical_spread)).unwrap_or(3);
                write!(
                    svg,
                    r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#000" opacity="{:.4}"/>"##,
                    layout.surface_x as i32 - spread,
                    layout.surface_y as i32 + 4 * scale - spread,
                    layout.surface_width as i32 + spread * 2,
                    layout.surface_height as i32 + spread * 2,
                    f32::from(alpha) / 255.0,
                )
                .unwrap();
            }
        }
        svg_rect(
            svg,
            layout.surface_x as i32,
            layout.surface_y as i32,
            layout.surface_width as i32,
            layout.surface_height as i32,
            self.options.theme.background,
            None,
        );
        if layout.chrome > 0 {
            let bar = adjust(
                self.options.theme.background,
                if is_dark(self.options.theme.background) {
                    18
                } else {
                    -12
                },
            );
            svg_rect(
                svg,
                layout.surface_x as i32,
                layout.surface_y as i32,
                layout.surface_width as i32,
                layout.chrome as i32,
                bar,
                None,
            );
            for (index, color) in [
                Rgb::new(255, 95, 86),
                Rgb::new(255, 189, 46),
                Rgb::new(39, 201, 63),
            ]
            .into_iter()
            .enumerate()
            {
                svg_rect(
                    svg,
                    layout.surface_x as i32
                        + (14 + index as i32 * 20) * i32::from(self.options.scale),
                    layout.surface_y as i32 + 10 * i32::from(self.options.scale),
                    8 * i32::from(self.options.scale),
                    8 * i32::from(self.options.scale),
                    color,
                    None,
                );
            }
        }
    }

    fn svg_backgrounds(&self, svg: &mut String, grid: &Grid, layout: Layout) {
        for y in 0..grid.rows() {
            let mut x = 0;
            while x < grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (_, background) = colors(&self.options.theme, cell.style);
                let start = x;
                x += 1;
                while x < grid.columns() {
                    let next = grid.get(x, y).expect("coordinates come from grid bounds");
                    let (_, next_background) = colors(&self.options.theme, next.style);
                    if next_background != background {
                        break;
                    }
                    x += 1;
                }
                if background != self.options.theme.background {
                    svg_rect(
                        svg,
                        (layout.grid_x + u32::from(start) * layout.cell_width) as i32,
                        (layout.grid_y + u32::from(y) * layout.cell_height) as i32,
                        (u32::from(x - start) * layout.cell_width) as i32,
                        layout.cell_height as i32,
                        background,
                        None,
                    );
                }
            }
        }
    }

    fn svg_cells(&self, svg: &mut String, grid: &Grid, layout: Layout, glyphs: &SvgGlyphCatalog) {
        for y in 0..grid.rows() {
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (foreground, _) = colors(&self.options.theme, cell.style);
                let px = layout.grid_x + u32::from(x) * layout.cell_width;
                let py = layout.grid_y + u32::from(y) * layout.cell_height;
                if cell.width == CellWidth::Continuation {
                    continue;
                }
                let span = if cell.width == CellWidth::Wide {
                    layout.cell_width * 2
                } else {
                    layout.cell_width
                };
                let attributes = cell.style.attributes;
                if !attributes.hidden {
                    let mut chars = cell.symbol.chars();
                    if let (Some(character), None) = (chars.next(), chars.next()) {
                        if let Some(shapes) =
                            geometry::shapes(character, span as i32, layout.cell_height as i32)
                        {
                            for shape in &shapes {
                                svg_shape(
                                    svg,
                                    shape,
                                    px as i32,
                                    py as i32,
                                    foreground,
                                    if attributes.dim { Some(".6") } else { None },
                                );
                            }
                        } else {
                            self.svg_text(
                                svg,
                                &cell.symbol,
                                px,
                                py,
                                span,
                                layout.cell_height,
                                foreground,
                                attributes,
                                glyphs,
                            );
                        }
                    } else {
                        self.svg_text(
                            svg,
                            &cell.symbol,
                            px,
                            py,
                            span,
                            layout.cell_height,
                            foreground,
                            attributes,
                            glyphs,
                        );
                    }
                    self.svg_decorations(
                        svg,
                        px,
                        py,
                        span,
                        layout.cell_height,
                        foreground,
                        cell.style.underline_color,
                        attributes,
                    );
                }
            }
        }
    }

    fn svg_text(
        &self,
        svg: &mut String,
        text: &str,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: Rgb,
        attributes: Attributes,
        glyphs: &SvgGlyphCatalog,
    ) {
        use fmt::Write as _;
        let size = self.options.font_size * f32::from(self.options.scale);
        let center = x as f32 + width as f32 / 2.0;
        let baseline = y as f32 + height as f32 / 2.0 + size / 2.0 - f32::from(self.options.scale);
        let mut body = String::new();
        for character in text
            .chars()
            .filter(|character| !matches!(*character, '\u{200d}' | '\u{fe0f}'))
        {
            let font_index = select_font(&self.fonts, character);
            let font = &self.fonts[font_index];
            let metrics = font.glyph_metrics(character, size);
            let Some(id) = glyphs.id(font_index, metrics.glyph) else {
                continue;
            };
            let origin_x = center - metrics.advance_width / 2.0;
            let shear = if attributes.italic {
                metrics.font_scale * 0.18
            } else {
                0.0
            };
            write!(body, r##"<use href="#{id}" transform="matrix({} 0 {shear} {} {origin_x} {baseline})"/>"##, metrics.font_scale, -metrics.font_scale).unwrap();
            if attributes.bold {
                write!(
                    body,
                    r##"<use href="#{id}" transform="matrix({} 0 {shear} {} {} {baseline})"/>"##,
                    metrics.font_scale,
                    -metrics.font_scale,
                    origin_x + f32::from(self.options.scale)
                )
                .unwrap();
            }
        }
        if body.is_empty() {
            return;
        }
        write!(
            svg,
            r#"<svg x="{x}" y="{y}" width="{width}" height="{height}" viewBox="{x} {y} {width} {height}" overflow="hidden"><g fill="{}"{}>"#,
            color.to_hex(),
            if attributes.dim {
                " opacity=\".6\""
            } else {
                ""
            }
        )
        .unwrap();
        svg.push_str(&body);
        svg.push_str("</g></svg>");
    }

    #[allow(clippy::too_many_lines)] // Each SGR decoration has distinct SVG geometry.
    fn svg_decorations(
        &self,
        svg: &mut String,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        foreground: Rgb,
        underline_color: ttyshot_core::Color,
        attributes: Attributes,
    ) {
        let scale = i32::from(self.options.scale);
        let x = x as i32;
        let y = y as i32;
        let width = width as i32;
        let height = height as i32;
        let underline = self
            .options
            .theme
            .resolve(underline_color, ColorRole::Foreground);
        match attributes.underline {
            Underline::None => {}
            Underline::Single => svg_rect(
                svg,
                x,
                y + height - 2 * scale,
                width,
                scale,
                underline,
                None,
            ),
            Underline::Double => {
                svg_rect(
                    svg,
                    x,
                    y + height - 4 * scale,
                    width,
                    scale,
                    underline,
                    None,
                );
                svg_rect(
                    svg,
                    x,
                    y + height - 2 * scale,
                    width,
                    scale,
                    underline,
                    None,
                );
            }
            Underline::Curly => {
                let run = 2 * scale;
                for offset in (0..width).step_by(run as usize) {
                    let wave = if (offset / run).rem_euclid(2) == 0 {
                        0
                    } else {
                        scale
                    };
                    svg_rect(
                        svg,
                        x + offset,
                        y + height - 3 * scale + wave,
                        run.min(width - offset),
                        scale,
                        underline,
                        None,
                    );
                }
            }
            Underline::Dotted | Underline::Dashed => {
                use fmt::Write as _;
                let dash = if attributes.underline == Underline::Dotted {
                    format!("{scale},{scale}")
                } else {
                    format!("{},{}", 3 * scale, 2 * scale)
                };
                write!(svg, r#"<line x1="{x}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{scale}" stroke-dasharray="{dash}"/>"#, y + height - 2 * scale, x + width, y + height - 2 * scale, underline.to_hex()).unwrap();
            }
        }
        if attributes.strikethrough {
            svg_rect(svg, x, y + height / 2, width, scale, foreground, None);
        }
        if attributes.overline {
            svg_rect(svg, x, y, width, scale, foreground, None);
        }
        match attributes.frame {
            Frame::None => {}
            Frame::Framed => svg_stroke_rect(svg, x, y, width, height, scale, foreground),
            Frame::Encircled => svg_ellipse(svg, x, y, width, height, scale, foreground),
        }
        match attributes.ideogram {
            Ideogram::None => {}
            Ideogram::Underline => {
                svg_rect(svg, x, y + height - scale, width, scale, foreground, None)
            }
            Ideogram::DoubleUnderline => {
                svg_rect(svg, x, y + height - scale, width, scale, foreground, None);
                svg_rect(
                    svg,
                    x,
                    y + height - 3 * scale,
                    width,
                    scale,
                    foreground,
                    None,
                );
            }
            Ideogram::Overline => svg_rect(svg, x, y, width, scale, foreground, None),
            Ideogram::DoubleOverline => {
                svg_rect(svg, x, y, width, scale, foreground, None);
                svg_rect(svg, x, y + 2 * scale, width, scale, foreground, None);
            }
            Ideogram::Stress => svg_rect(
                svg,
                x + width / 2 - scale / 2,
                y,
                scale,
                3 * scale,
                foreground,
                None,
            ),
        }
    }

    fn svg_cursor(&self, svg: &mut String, grid: &Grid, layout: Layout) {
        let Some(cursor) = grid
            .cursor
            .filter(|cursor| cursor.visible && cursor.x < grid.columns() && cursor.y < grid.rows())
        else {
            return;
        };
        let x = (layout.grid_x + u32::from(cursor.x) * layout.cell_width) as i32;
        let y = (layout.grid_y + u32::from(cursor.y) * layout.cell_height) as i32;
        let scale = i32::from(self.options.scale);
        match cursor.shape {
            CursorShape::Block => svg_rect(
                svg,
                x,
                y,
                layout.cell_width as i32,
                layout.cell_height as i32,
                self.options.theme.cursor,
                Some(".5"),
            ),
            CursorShape::Bar => svg_rect(
                svg,
                x,
                y,
                2 * scale,
                layout.cell_height as i32,
                self.options.theme.cursor,
                None,
            ),
            CursorShape::Underline => svg_rect(
                svg,
                x,
                y + layout.cell_height as i32 - 2 * scale,
                layout.cell_width as i32,
                2 * scale,
                self.options.theme.cursor,
                None,
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    width: u32,
    height: u32,
    surface_x: u32,
    surface_y: u32,
    surface_width: u32,
    surface_height: u32,
    grid_x: u32,
    grid_y: u32,
    cell_width: u32,
    cell_height: u32,
    chrome: u32,
}

/// Renderer construction or encoding error.
#[derive(Debug)]
pub enum RenderError {
    /// An option is outside its supported range.
    InvalidOptions(&'static str),
    /// A bundled, configured, or discovered font could not be parsed.
    InvalidFont {
        /// Font path or bundled-font description.
        source: String,
        /// Parser error.
        message: &'static str,
    },
    /// Reading a configured font failed.
    FontIo {
        /// Requested path.
        path: PathBuf,
        /// File-system error.
        error: std::io::Error,
    },
    /// Output dimension arithmetic overflowed.
    DimensionOverflow,
    /// The validated image exceeds the renderer safety cap.
    CanvasTooLarge {
        /// Requested pixel width.
        width: u32,
        /// Requested pixel height.
        height: u32,
    },
    /// The pixel buffer could not be allocated.
    AllocationFailed {
        /// Requested pixel width.
        width: u32,
        /// Requested pixel height.
        height: u32,
    },
    /// PNG encoding failed.
    Png(png::EncodingError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(formatter, "invalid render options: {message}"),
            Self::InvalidFont { source, message } => {
                write!(formatter, "font {source} is invalid: {message}")
            }
            Self::FontIo { path, error } => {
                write!(formatter, "could not read font {}: {error}", path.display())
            }
            Self::DimensionOverflow => formatter.write_str("rendered image dimensions overflowed"),
            Self::CanvasTooLarge { width, height } => write!(
                formatter,
                "rendered image {width}x{height} exceeds the canvas safety limit"
            ),
            Self::AllocationFailed { width, height } => write!(
                formatter,
                "could not allocate the {width}x{height} pixel canvas"
            ),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}
impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FontIo { error, .. } => Some(error),
            Self::Png(error) => Some(error),
            _ => None,
        }
    }
}
impl From<png::EncodingError> for RenderError {
    fn from(error: png::EncodingError) -> Self {
        Self::Png(error)
    }
}

fn colors(theme: &Theme, style: ttyshot_core::Style) -> (Rgb, Rgb) {
    let foreground = theme.resolve(style.foreground, ColorRole::Foreground);
    let background = theme.resolve(style.background, ColorRole::Background);
    if style.attributes.reverse {
        (background, foreground)
    } else {
        (foreground, background)
    }
}
fn is_dark(color: Rgb) -> bool {
    u32::from(color.r) * 299 + u32::from(color.g) * 587 + u32::from(color.b) * 114 < 128_000
}
fn adjust(color: Rgb, amount: i16) -> Rgb {
    let channel =
        |value: u8| u8::try_from((i16::from(value) + amount).clamp(0, 255)).unwrap_or(value);
    Rgb::new(channel(color.r), channel(color.g), channel(color.b))
}
fn stroke_rect(
    canvas: &mut Canvas,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    stroke: i32,
    color: Rgb,
) {
    canvas.fill_rect(x, y, width, stroke, color, 255);
    canvas.fill_rect(x, y + height - stroke, width, stroke, color, 255);
    canvas.fill_rect(x, y, stroke, height, color, 255);
    canvas.fill_rect(x + width - stroke, y, stroke, height, color, 255);
}

fn encode_png(canvas: &Canvas) -> Result<Vec<u8>, RenderError> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, canvas.width, canvas.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::High);
        encoder.set_filter(png::Filter::Sub);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&canvas.pixels)?;
    }
    Ok(output)
}

fn svg_rect(
    svg: &mut String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: Rgb,
    opacity: Option<&str>,
) {
    use fmt::Write as _;
    if width <= 0 || height <= 0 {
        return;
    }
    write!(
        svg,
        r#"<rect x="{x}" y="{y}" width="{width}" height="{height}" fill="{}""#,
        color.to_hex()
    )
    .unwrap();
    if let Some(value) = opacity {
        write!(svg, r#" opacity="{value}""#).unwrap();
    }
    svg.push_str("/>");
}
fn svg_stroke_rect(
    svg: &mut String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    stroke: i32,
    color: Rgb,
) {
    svg_rect(svg, x, y, width, stroke, color, None);
    svg_rect(svg, x, y + height - stroke, width, stroke, color, None);
    svg_rect(svg, x, y, stroke, height, color, None);
    svg_rect(svg, x + width - stroke, y, stroke, height, color, None);
}
fn svg_ellipse(svg: &mut String, x: i32, y: i32, width: i32, height: i32, stroke: i32, color: Rgb) {
    use fmt::Write as _;
    write!(
        svg,
        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="none" stroke="{}" stroke-width="{stroke}"/>"#,
        x + width / 2,
        y + height / 2,
        width / 2,
        height / 2,
        color.to_hex(),
    )
    .unwrap();
}

fn svg_shape(
    svg: &mut String,
    shape: &Shape,
    origin_x: i32,
    origin_y: i32,
    color: Rgb,
    opacity: Option<&str>,
) {
    use fmt::Write as _;
    let opacity = opacity.map_or(String::new(), |value| format!(" opacity=\"{value}\""));
    match shape { Shape::Rect { x, y, width, height } => svg_rect(svg, origin_x + x, origin_y + y, *width, *height, color, if opacity.is_empty() { None } else { Some(".6") }), Shape::Line { x1, y1, x2, y2, width } => write!(svg, r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="square"{opacity}/>"#, origin_x + x1, origin_y + y1, origin_x + x2, origin_y + y2, color.to_hex(), width).unwrap(), Shape::Polygon(points) => { let points = points.iter().map(|(x, y)| format!("{},{}", origin_x + x, origin_y + y)).collect::<Vec<_>>().join(" "); write!(svg, r#"<polygon points="{points}" fill="{}"{opacity}/>"#, color.to_hex()).unwrap(); } }
}
/// Reports the crate version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
