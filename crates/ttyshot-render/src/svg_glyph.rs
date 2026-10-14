use std::{collections::BTreeMap, fmt::Write as _};

use ttf_parser::{GlyphId, OutlineBuilder};
use ttyshot_core::{CellWidth, Grid};

use crate::{font::LoadedFont, geometry};

pub(crate) struct SvgGlyphCatalog {
    ids: BTreeMap<(usize, u16), String>,
    definitions: String,
}

impl SvgGlyphCatalog {
    pub(crate) fn build(grid: &Grid, fonts: &[LoadedFont]) -> Self {
        let mut paths = BTreeMap::<(usize, u16), String>::new();
        for cell in grid.cells() {
            if cell.width == CellWidth::Continuation || cell.style.attributes.hidden {
                continue;
            }
            for character in cell
                .symbol
                .chars()
                .filter(|character| !matches!(*character, '\u{200d}' | '\u{fe0f}'))
            {
                if geometry::shapes(character, 1, 1).is_some() {
                    continue;
                }
                let font_index = select_font(fonts, character);
                let glyph = fonts[font_index].glyph_index(character);
                let key = (font_index, glyph);
                paths
                    .entry(key)
                    .or_insert_with(|| glyph_path(&fonts[font_index], glyph).unwrap_or_default());
            }
        }
        let mut definitions = String::from("<defs>");
        let mut ids = BTreeMap::new();
        let mut path_ids = BTreeMap::<String, String>::new();
        for ((font_index, glyph), path) in paths {
            if path.is_empty() {
                continue;
            }
            let id = if let Some(id) = path_ids.get(&path) {
                id.clone()
            } else {
                let id = format!("g{font_index}-{glyph}");
                write!(definitions, r#"<path id="{id}" d="{path}"/>"#).unwrap();
                path_ids.insert(path, id.clone());
                id
            };
            ids.insert((font_index, glyph), id);
        }
        definitions.push_str("</defs>");
        Self { ids, definitions }
    }

    pub(crate) fn definitions(&self) -> &str {
        &self.definitions
    }

    pub(crate) fn id(&self, font_index: usize, glyph: u16) -> Option<&str> {
        self.ids.get(&(font_index, glyph)).map(String::as_str)
    }
}

pub(crate) fn select_font(fonts: &[LoadedFont], character: char) -> usize {
    fonts
        .iter()
        .position(|font| font.has_glyph(character))
        .unwrap_or(0)
}

fn glyph_path(font: &LoadedFont, glyph: u16) -> Option<String> {
    let face = ttf_parser::Face::parse(font.bytes.as_slice(), font.collection_index).ok()?;
    let mut builder = PathBuilder::default();
    face.outline_glyph(GlyphId(glyph), &mut builder)?;
    Some(builder.output)
}

#[derive(Default)]
struct PathBuilder {
    output: String,
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        write!(self.output, "M{} {}", number(x), number(y)).unwrap();
    }
    fn line_to(&mut self, x: f32, y: f32) {
        write!(self.output, "L{} {}", number(x), number(y)).unwrap();
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        write!(
            self.output,
            "Q{} {} {} {}",
            number(x1),
            number(y1),
            number(x),
            number(y)
        )
        .unwrap();
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        write!(
            self.output,
            "C{} {} {} {} {} {}",
            number(x1),
            number(y1),
            number(x2),
            number(y2),
            number(x),
            number(y)
        )
        .unwrap();
    }
    fn close(&mut self) {
        self.output.push('Z');
    }
}

fn number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}
