use ab_glyph_rasterizer::{Point, Rasterizer, point};
use fontdue::{Font, FontSettings};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use ttf_parser::{GlyphId, OutlineBuilder};

use crate::RenderError;

/// Controls whether installed operating-system fallback fonts are considered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FontMode {
    /// Use configured fonts, the bundled font, then ordered platform CJK fallbacks.
    #[default]
    System,
    /// Use only configured fonts and the bundled font for reproducible output.
    Deterministic,
}

/// A font file and a face index within a TTC/OTC collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontSource {
    /// Path to a TTF, OTF, TTC, or OTC file.
    pub path: PathBuf,
    /// Zero-based face index. Use zero for a normal TTF/OTF.
    pub collection_index: u32,
}

impl FontSource {
    /// Selects the first face in a font file.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            collection_index: 0,
        }
    }

    /// Selects a specific face in a font collection.
    #[must_use]
    pub fn collection(path: impl Into<PathBuf>, collection_index: u32) -> Self {
        Self {
            path: path.into(),
            collection_index,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LoadedFont {
    pub(crate) raster: Arc<Font>,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) collection_index: u32,
    pub(crate) source: String,
    uses_cff_outlines: bool,
}

impl LoadedFont {
    pub(crate) fn load(
        bytes: Arc<Vec<u8>>,
        collection_index: u32,
        source: String,
    ) -> Result<Self, RenderError> {
        let raster = Font::from_bytes(
            bytes.as_slice(),
            FontSettings {
                collection_index,
                ..FontSettings::default()
            },
        )
        .map_err(|message| RenderError::InvalidFont {
            source: source.clone(),
            message,
        })?;
        // Parse separately because SVG output depends on vector outlines too.
        let face =
            ttf_parser::Face::parse(bytes.as_slice(), collection_index).map_err(|error| {
                RenderError::InvalidFont {
                    source: source.clone(),
                    message: font_error(error),
                }
            })?;
        let uses_cff_outlines = face.tables().cff.is_some() || face.tables().cff2.is_some();
        Ok(Self {
            raster: Arc::new(raster),
            bytes,
            collection_index,
            source,
            uses_cff_outlines,
        })
    }

    pub(crate) fn from_source(source: &FontSource) -> Result<Self, RenderError> {
        let bytes = fs::read(&source.path).map_err(|error| RenderError::FontIo {
            path: source.path.clone(),
            error,
        })?;
        Self::load(
            Arc::new(bytes),
            source.collection_index,
            source.path.display().to_string(),
        )
    }

    pub(crate) fn glyph_index(&self, character: char) -> u16 {
        let face = ttf_parser::Face::parse(self.bytes.as_slice(), self.collection_index)
            .expect("font was validated when loaded");
        face.glyph_index(character).unwrap_or(GlyphId(0)).0
    }

    pub(crate) fn glyph_metrics(&self, character: char, size: f32) -> GlyphMetrics {
        let face = ttf_parser::Face::parse(self.bytes.as_slice(), self.collection_index)
            .expect("font was validated when loaded");
        let glyph = face.glyph_index(character).unwrap_or(GlyphId(0));
        let font_scale = size / f32::from(face.units_per_em());
        let advance_width = face
            .glyph_hor_advance(glyph)
            .map_or(0.0, |advance| f32::from(advance) * font_scale);
        GlyphMetrics {
            glyph: glyph.0,
            advance_width,
            font_scale,
        }
    }

    pub(crate) fn has_glyph(&self, character: char) -> bool {
        self.glyph_index(character) != 0
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GlyphMetrics {
    pub(crate) glyph: u16,
    pub(crate) advance_width: f32,
    pub(crate) font_scale: f32,
}

pub(crate) struct RasterizedGlyph {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) xmin: i32,
    pub(crate) ymin: i32,
    pub(crate) advance_width: f32,
    pub(crate) bitmap: Vec<u8>,
}

impl LoadedFont {
    pub(crate) fn rasterize(&self, character: char, size: f32) -> RasterizedGlyph {
        let glyph_metrics = self.glyph_metrics(character, size);
        if !self.uses_cff_outlines {
            let (metrics, bitmap) = self.raster.rasterize_indexed(glyph_metrics.glyph, size);
            return RasterizedGlyph {
                width: metrics.width,
                height: metrics.height,
                xmin: metrics.xmin,
                ymin: metrics.ymin,
                advance_width: glyph_metrics.advance_width,
                bitmap,
            };
        }
        self.rasterize_outline(glyph_metrics)
    }

    /// `fontdue` mis-rasterizes the CID-keyed CFF outlines in Apple's CJK
    /// fonts. Rasterizing the already-selected `ttf-parser` outline also keeps
    /// PNG and SVG on the exact same glyph ID.
    fn rasterize_outline(&self, metrics: GlyphMetrics) -> RasterizedGlyph {
        let face = ttf_parser::Face::parse(self.bytes.as_slice(), self.collection_index)
            .expect("font was validated when loaded");
        let glyph = GlyphId(metrics.glyph);
        let scale = metrics.font_scale;
        let Some(bounds) = face.glyph_bounding_box(glyph) else {
            return RasterizedGlyph {
                width: 0,
                height: 0,
                xmin: 0,
                ymin: 0,
                advance_width: metrics.advance_width,
                bitmap: Vec::new(),
            };
        };
        let xmin = (f32::from(bounds.x_min) * scale).floor() as i32;
        let ymin = (f32::from(bounds.y_min) * scale).floor() as i32;
        let xmax = (f32::from(bounds.x_max) * scale).ceil() as i32;
        let ymax = (f32::from(bounds.y_max) * scale).ceil() as i32;
        let width = usize::try_from((xmax - xmin).max(0)).unwrap_or(0);
        let height = usize::try_from((ymax - ymin).max(0)).unwrap_or(0);
        let mut rasterizer = Rasterizer::new(width, height);
        let mut builder = GlyphRasterBuilder {
            rasterizer: &mut rasterizer,
            scale,
            x_offset: -(xmin as f32),
            y_offset: ymax as f32,
            start: point(0.0, 0.0),
            current: point(0.0, 0.0),
        };
        let _ = face.outline_glyph(glyph, &mut builder);
        let mut bitmap = vec![0; width.saturating_mul(height)];
        rasterizer.for_each_pixel(|index, alpha| {
            bitmap[index] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        });
        RasterizedGlyph {
            width,
            height,
            xmin,
            ymin,
            advance_width: metrics.advance_width,
            bitmap,
        }
    }
}

struct GlyphRasterBuilder<'a> {
    rasterizer: &'a mut Rasterizer,
    scale: f32,
    x_offset: f32,
    y_offset: f32,
    start: Point,
    current: Point,
}

impl GlyphRasterBuilder<'_> {
    fn point(&self, x: f32, y: f32) -> Point {
        point(
            x * self.scale + self.x_offset,
            self.y_offset - y * self.scale,
        )
    }
}

impl OutlineBuilder for GlyphRasterBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let target = self.point(x, y);
        self.start = target;
        self.current = target;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let target = self.point(x, y);
        self.rasterizer.draw_line(self.current, target);
        self.current = target;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control = self.point(x1, y1);
        let target = self.point(x, y);
        self.rasterizer.draw_quad(self.current, control, target);
        self.current = target;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let first = self.point(x1, y1);
        let second = self.point(x2, y2);
        let target = self.point(x, y);
        self.rasterizer
            .draw_cubic(self.current, first, second, target);
        self.current = target;
    }

    fn close(&mut self) {
        self.rasterizer.draw_line(self.current, self.start);
        self.current = self.start;
    }
}

pub(crate) fn system_fallbacks() -> Vec<LoadedFont> {
    static SYSTEM_FONTS: OnceLock<Vec<LoadedFont>> = OnceLock::new();
    SYSTEM_FONTS
        .get_or_init(|| {
            system_font_paths()
                .into_iter()
                .filter_map(|path| load_cjk_collection_face(&path))
                .collect()
        })
        .clone()
}

/// Loads at most one CJK-capable face from each ordered system font file.
///
/// Parsing every face of Apple's large TTC collections for each `Renderer`
/// consumed hundreds of MiB. A system file only needs one representative CJK
/// face because the next ordered file supplies the next language family.
fn load_cjk_collection_face(path: &Path) -> Option<LoadedFont> {
    let bytes = Arc::new(fs::read(path).ok()?);
    let face_count = ttf_parser::fonts_in_collection(bytes.as_slice()).unwrap_or(1);
    let index = (0..face_count).find(|index| {
        ttf_parser::Face::parse(bytes.as_slice(), *index).is_ok_and(|face| {
            ['한', '中', 'あ']
                .into_iter()
                .any(|character| face.glyph_index(character).is_some())
        })
    })?;
    LoadedFont::load(bytes, index, format!("{}#{index}", path.display())).ok()
}

#[cfg(target_os = "macos")]
fn system_font_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
        PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
    ];
    // Japanese Hiragino file names vary by macOS release and Unicode normalization.
    if let Ok(entries) = fs::read_dir("/System/Library/Fonts") {
        let mut hiragino = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("ヒラギノ角ゴシック"))
            })
            .collect::<Vec<_>>();
        hiragino.sort();
        paths.extend(hiragino);
    }
    paths
}

#[cfg(target_os = "linux")]
fn system_font_paths() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKkr-Regular.otf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn system_font_paths() -> Vec<PathBuf> {
    Vec::new()
}

fn font_error(error: ttf_parser::FaceParsingError) -> &'static str {
    match error {
        ttf_parser::FaceParsingError::MalformedFont => "malformed font",
        ttf_parser::FaceParsingError::UnknownMagic => "unknown font format",
        ttf_parser::FaceParsingError::FaceIndexOutOfBounds => "collection index out of bounds",
        ttf_parser::FaceParsingError::NoHeadTable => "missing head table",
        ttf_parser::FaceParsingError::NoHheaTable => "missing hhea table",
        ttf_parser::FaceParsingError::NoMaxpTable => "missing maxp table",
    }
}
