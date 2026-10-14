#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::manual_midpoint,
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
mod geometry;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use canvas::Canvas;
use fontdue::{Font, FontSettings};
use geometry::Shape;
use std::{error::Error, fmt, fs, path::PathBuf};
use ttyshot_core::{
    Attributes, CellWidth, ColorRole, CursorShape, Frame, Grid, Ideogram, Rgb, Theme, Underline,
};

pub use ttyshot_core as core;

const FONT_BYTES: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
/// Exact SIL Open Font License text shipped with the bundled font.
pub const BUNDLED_FONT_LICENSE: &str = include_str!("../assets/OFL-JetBrainsMono.txt");

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
    /// Optional font file used when the bundled font has no glyph, commonly CJK.
    pub cjk_fallback: Option<PathBuf>,
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
            cjk_fallback: None,
        }
    }
}

/// A validated renderer with parsed fonts.
pub struct Renderer {
    options: RenderOptions,
    primary_font: Font,
    fallback_font: Option<Font>,
    fallback_bytes: Option<Vec<u8>>,
}

impl Renderer {
    /// Loads fonts and validates an option set.
    ///
    /// # Errors
    /// Returns an error for zero dimensions, an unsupported scale, or an
    /// unreadable/invalid fallback font.
    pub fn new(options: RenderOptions) -> Result<Self, RenderError> {
        if options.cell_width == 0
            || options.cell_height == 0
            || options.font_size <= 0.0
            || !options.font_size.is_finite()
        {
            return Err(RenderError::InvalidOptions(
                "cell and font sizes must be finite and positive",
            ));
        }
        if options.scale == 0 || options.scale > 4 {
            return Err(RenderError::InvalidOptions("scale must be between 1 and 4"));
        }
        let primary_font = Font::from_bytes(FONT_BYTES, FontSettings::default())
            .map_err(RenderError::InvalidBundledFont)?;
        let fallback_bytes = options.cjk_fallback.as_ref().map(fs::read).transpose()?;
        let fallback_font = fallback_bytes
            .as_ref()
            .map(|bytes| Font::from_bytes(bytes.as_slice(), FontSettings::default()))
            .transpose()
            .map_err(RenderError::InvalidFallbackFont)?;
        Ok(Self {
            options,
            primary_font,
            fallback_font,
            fallback_bytes,
        })
    }

    /// Returns this renderer's options.
    #[must_use]
    pub const fn options(&self) -> &RenderOptions {
        &self.options
    }

    /// Renders a grid as deterministic RGBA PNG bytes.
    ///
    /// # Errors
    /// Returns an error when dimensions overflow or PNG encoding fails.
    pub fn render_png(&self, grid: &Grid) -> Result<Vec<u8>, RenderError> {
        let layout = self.layout(grid)?;
        let mut canvas = Canvas::new(layout.width, layout.height);
        self.paint_shell(&mut canvas, layout);
        self.paint_cells(&mut canvas, grid, layout);
        self.paint_cursor(&mut canvas, grid, layout);
        encode_png(&canvas)
    }

    /// Renders a grid as a deterministic standalone SVG document.
    ///
    /// Procedural glyphs remain vector geometry. Text uses an embedded copy of
    /// the same OFL font used by the PNG renderer.
    ///
    /// # Errors
    /// Returns an error when output dimensions overflow.
    pub fn render_svg(&self, grid: &Grid) -> Result<String, RenderError> {
        let layout = self.layout(grid)?;
        let mut svg = String::new();
        use fmt::Write as _;
        write!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#, layout.width, layout.height, layout.width, layout.height).unwrap();
        write!(svg, "<defs><style>@font-face{{font-family:ttyshot;src:url(data:font/ttf;base64,{}) format('truetype')}}", STANDARD.encode(FONT_BYTES)).unwrap();
        if let Some(bytes) = &self.fallback_bytes {
            write!(svg, "@font-face{{font-family:ttyshot-fallback;src:url(data:font/ttf;base64,{}) format('truetype')}}", STANDARD.encode(bytes)).unwrap();
        }
        svg.push_str("</style></defs>");
        self.svg_shell(&mut svg, layout);
        self.svg_cells(&mut svg, grid, layout);
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
        let surface_width = u32::from(grid.columns())
            .checked_mul(cell_width)
            .and_then(|value| {
                value.checked_add(
                    (u32::from(self.options.padding.left) + u32::from(self.options.padding.right))
                        * scale,
                )
            })
            .ok_or(RenderError::DimensionOverflow)?;
        let surface_height = u32::from(grid.rows())
            .checked_mul(cell_height)
            .and_then(|value| {
                value.checked_add(
                    (u32::from(self.options.padding.top) + u32::from(self.options.padding.bottom))
                        * scale,
                )
            })
            .and_then(|value| value.checked_add(chrome))
            .ok_or(RenderError::DimensionOverflow)?;
        let width = surface_width
            .checked_add(shadow * 2)
            .ok_or(RenderError::DimensionOverflow)?;
        let height = surface_height
            .checked_add(shadow * 2)
            .ok_or(RenderError::DimensionOverflow)?;
        if width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(RenderError::DimensionOverflow);
        }
        Ok(Layout {
            width,
            height,
            surface_x: shadow,
            surface_y: shadow,
            surface_width,
            surface_height,
            grid_x: shadow + u32::from(self.options.padding.left) * scale,
            grid_y: shadow + chrome + u32::from(self.options.padding.top) * scale,
            cell_width,
            cell_height,
            chrome,
        })
    }

    fn paint_shell(&self, canvas: &mut Canvas, layout: Layout) {
        if self.options.shadow {
            for spread in (2..=12).rev() {
                let alpha = u8::try_from(3 + (12 - spread)).unwrap_or(3);
                canvas.fill_rect(
                    layout.surface_x as i32 - spread,
                    layout.surface_y as i32 + 4 - spread,
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
        for y in 0..grid.rows() {
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (foreground, background) = colors(&self.options.theme, cell.style);
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
            let font = self.font_for(character);
            let (metrics, bitmap) = font.rasterize(character, size);
            let center = x as f32 + width as f32 / 2.0;
            let draw_x = (center - metrics.advance_width / 2.0).round() as i32 + metrics.xmin;
            let baseline = y as i32 + ((height as f32 + size) / 2.0).round() as i32
                - i32::from(self.options.scale);
            let draw_y = baseline - metrics.height as i32 - metrics.ymin;
            canvas.mask(
                draw_x,
                draw_y,
                metrics.width,
                metrics.height,
                &bitmap,
                color,
                alpha,
                (x as i32, y as i32, (x + width) as i32, (y + height) as i32),
                attributes.bold,
                attributes.italic,
            );
        }
    }

    fn font_for(&self, character: char) -> &Font {
        if self.primary_font.lookup_glyph_index(character) != 0 {
            return &self.primary_font;
        }
        self.fallback_font
            .as_ref()
            .filter(|font| font.lookup_glyph_index(character) != 0)
            .unwrap_or(&self.primary_font)
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
                for offset in 0..width {
                    let wave = if (offset / (2 * scale)).rem_euclid(2) == 0 {
                        0
                    } else {
                        scale
                    };
                    canvas.fill_rect(
                        x + offset,
                        y + height - 3 * scale + wave,
                        1,
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
            write!(
                svg,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#000" opacity=".18"/>"##,
                layout.surface_x + 5,
                layout.surface_y + 8,
                layout.surface_width,
                layout.surface_height
            )
            .unwrap();
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

    fn svg_cells(&self, svg: &mut String, grid: &Grid, layout: Layout) {
        for y in 0..grid.rows() {
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).expect("coordinates come from grid bounds");
                let (foreground, background) = colors(&self.options.theme, cell.style);
                let px = layout.grid_x + u32::from(x) * layout.cell_width;
                let py = layout.grid_y + u32::from(y) * layout.cell_height;
                svg_rect(
                    svg,
                    px as i32,
                    py as i32,
                    layout.cell_width as i32,
                    layout.cell_height as i32,
                    background,
                    None,
                );
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
    ) {
        use fmt::Write as _;
        let size = self.options.font_size * f32::from(self.options.scale);
        let center = x as f32 + width as f32 / 2.0;
        let baseline = y as f32 + (height as f32 + size) / 2.0 - f32::from(self.options.scale);
        let clip_id = format!("cell-{x}-{y}");
        write!(
            svg,
            r#"<clipPath id="{clip_id}"><rect x="{x}" y="{y}" width="{width}" height="{height}"/></clipPath>"#,
        )
        .unwrap();
        write!(
            svg,
            r#"<text x="{center}" y="{baseline}" text-anchor="middle" font-family="ttyshot,ttyshot-fallback" font-size="{size}" fill="{}" clip-path="url(#{clip_id})"{}{}{}>{}</text>"#,
            color.to_hex(),
            if attributes.bold { " font-weight=\"700\"" } else { "" },
            if attributes.italic { " font-style=\"italic\"" } else { "" },
            if attributes.dim { " opacity=\".6\"" } else { "" },
            escape_xml(text)
        )
        .unwrap();
    }

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
                use fmt::Write as _;
                write!(svg, r#"<path d="M{x} {} q {} {} {} 0 t {} 0" fill="none" stroke="{}" stroke-width="{}"/>"#, y + height - 2 * scale, 2 * scale, -2 * scale, 4 * scale, width - 4 * scale, underline.to_hex(), scale).unwrap();
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
    /// The bundled font could not be parsed. This indicates corrupt package data.
    InvalidBundledFont(&'static str),
    /// The configured fallback font could not be parsed.
    InvalidFallbackFont(&'static str),
    /// Reading a configured fallback font failed.
    Io(std::io::Error),
    /// Output dimensions exceeded the supported integer range.
    DimensionOverflow,
    /// PNG encoding failed.
    Png(png::EncodingError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(message) => write!(formatter, "invalid render options: {message}"),
            Self::InvalidBundledFont(message) => {
                write!(formatter, "bundled font is invalid: {message}")
            }
            Self::InvalidFallbackFont(message) => {
                write!(formatter, "fallback font is invalid: {message}")
            }
            Self::Io(error) => write!(formatter, "could not read fallback font: {error}"),
            Self::DimensionOverflow => formatter.write_str("rendered image dimensions overflowed"),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}
impl Error for RenderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Png(error) => Some(error),
            _ => None,
        }
    }
}
impl From<std::io::Error> for RenderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
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
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Reports the crate version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
