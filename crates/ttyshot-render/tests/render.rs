#![allow(clippy::too_many_lines, clippy::chunks_exact_to_as_chunks)]

use std::io::Cursor;
use ttyshot_core::{
    Attributes, Blink, CellWidth, Color, Frame, Grid, Ideogram, Rgb, Style, Underline,
};
use ttyshot_render::{Padding, RenderOptions, Renderer, WindowChrome};

fn solid_style(foreground: Rgb, background: Rgb) -> Style {
    Style {
        foreground: Color::Rgb(foreground),
        background: Color::Rgb(background),
        ..Style::default()
    }
}

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .expect("decoded PNG size is known")
    ];
    let info = reader.next_frame(&mut buffer).unwrap();
    (
        info.width,
        info.height,
        buffer[..info.buffer_size()].to_vec(),
    )
}

fn stress_grid() -> Grid {
    let mut grid = Grid::new(32, 11).unwrap();
    let mut glyphs: Vec<char> = (0x2500..=0x257f).filter_map(char::from_u32).collect();
    glyphs.extend((0x2580..=0x259f).filter_map(char::from_u32));
    glyphs.extend((0x2800..=0x28ff).step_by(17).filter_map(char::from_u32));
    glyphs.extend([
        '\u{1fb00}',
        '\u{1fb1c}',
        '\u{1fb3b}',
        '\u{1cd00}',
        '\u{1cd72}',
        '\u{1cde5}',
        '\u{e0b0}',
        '\u{e0b1}',
        '\u{e0b2}',
        '\u{e0b3}',
    ]);
    for (index, glyph) in glyphs.into_iter().enumerate() {
        let x = u16::try_from(index % 32).unwrap();
        let y = u16::try_from(index / 32).unwrap();
        let style = Style {
            foreground: Color::Indexed(u8::try_from(1 + index % 255).unwrap()),
            background: if index % 5 == 0 {
                Color::Indexed(u8::try_from((index * 7) % 256).unwrap())
            } else {
                Color::Default
            },
            ..Style::default()
        };
        grid.put(x, y, glyph.to_string(), CellWidth::Single, style)
            .unwrap();
    }
    let styles = [
        Attributes {
            bold: true,
            ..Attributes::default()
        },
        Attributes {
            dim: true,
            ..Attributes::default()
        },
        Attributes {
            italic: true,
            ..Attributes::default()
        },
        Attributes {
            underline: Underline::Single,
            ..Attributes::default()
        },
        Attributes {
            underline: Underline::Double,
            ..Attributes::default()
        },
        Attributes {
            underline: Underline::Curly,
            ..Attributes::default()
        },
        Attributes {
            underline: Underline::Dotted,
            ..Attributes::default()
        },
        Attributes {
            underline: Underline::Dashed,
            ..Attributes::default()
        },
        Attributes {
            reverse: true,
            ..Attributes::default()
        },
        Attributes {
            hidden: true,
            ..Attributes::default()
        },
        Attributes {
            strikethrough: true,
            ..Attributes::default()
        },
        Attributes {
            frame: Frame::Framed,
            ..Attributes::default()
        },
        Attributes {
            frame: Frame::Encircled,
            ..Attributes::default()
        },
        Attributes {
            overline: true,
            ..Attributes::default()
        },
        Attributes {
            blink: Blink::Slow,
            ..Attributes::default()
        },
        Attributes {
            blink: Blink::Rapid,
            ..Attributes::default()
        },
        Attributes {
            ideogram: Ideogram::Underline,
            ..Attributes::default()
        },
        Attributes {
            ideogram: Ideogram::DoubleUnderline,
            ..Attributes::default()
        },
        Attributes {
            ideogram: Ideogram::Overline,
            ..Attributes::default()
        },
        Attributes {
            ideogram: Ideogram::DoubleOverline,
            ..Attributes::default()
        },
        Attributes {
            ideogram: Ideogram::Stress,
            ..Attributes::default()
        },
    ];
    for (index, attributes) in styles.into_iter().enumerate() {
        let style = Style {
            foreground: Color::Rgb(Rgb::new(240, 210, 90)),
            background: Color::Indexed(17),
            underline_color: Color::Indexed(14),
            attributes,
        };
        grid.put(
            u16::try_from(index).unwrap(),
            10,
            char::from(b'A' + u8::try_from(index).unwrap()).to_string(),
            CellWidth::Single,
            style,
        )
        .unwrap();
    }
    grid.put(
        20,
        10,
        "界",
        CellWidth::Wide,
        solid_style(Rgb::new(240, 240, 240), Rgb::new(40, 45, 60)),
    )
    .unwrap();
    grid
}

#[test]
fn glyph_and_style_stress_matches_golden() {
    let options = RenderOptions {
        padding: Padding::all(0),
        ..RenderOptions::default()
    };
    let actual = Renderer::new(options)
        .unwrap()
        .render_png(&stress_grid())
        .unwrap();
    assert_eq!(actual, include_bytes!("goldens/glyph-style-stress.png"));
}

#[test]
fn adjacent_full_blocks_have_no_background_seam_at_one_and_two_x() {
    let foreground = Rgb::new(231, 76, 60);
    let background = Rgb::new(12, 24, 36);
    let style = solid_style(foreground, background);
    let mut grid = Grid::new(4, 3).unwrap();
    for y in 0..grid.rows() {
        for x in 0..grid.columns() {
            grid.put(x, y, "█", CellWidth::Single, style).unwrap();
        }
    }
    for scale in [1, 2] {
        let options = RenderOptions {
            cell_width: 7,
            cell_height: 13,
            scale,
            padding: Padding::all(0),
            ..RenderOptions::default()
        };
        let bytes = Renderer::new(options).unwrap().render_png(&grid).unwrap();
        let (_, _, pixels) = decode(&bytes);
        for pixel in pixels.chunks_exact(4) {
            assert_eq!(
                pixel,
                [foreground.r, foreground.g, foreground.b, 255],
                "background/seam pixel at {scale}x"
            );
        }
    }
}

#[test]
fn last_row_and_column_are_fully_visible() {
    let foreground = Rgb::new(7, 201, 91);
    let background = Rgb::new(1, 2, 3);
    let mut grid = Grid::new(3, 2).unwrap();
    grid.put(
        2,
        1,
        "█",
        CellWidth::Single,
        solid_style(foreground, background),
    )
    .unwrap();
    let options = RenderOptions {
        cell_width: 9,
        cell_height: 17,
        padding: Padding::all(0),
        ..RenderOptions::default()
    };
    let bytes = Renderer::new(options).unwrap().render_png(&grid).unwrap();
    let (width, height, pixels) = decode(&bytes);
    let last = ((height - 1) * width + width - 1) as usize * 4;
    assert_eq!(
        &pixels[last..last + 4],
        &[foreground.r, foreground.g, foreground.b, 255]
    );
}

#[test]
fn wide_glyph_is_clipped_to_exactly_two_cells() {
    let foreground = Rgb::new(255, 255, 255);
    let background = Rgb::new(9, 10, 11);
    let style = solid_style(foreground, background);
    let mut grid = Grid::new(4, 1).unwrap();
    grid.put(1, 0, "界", CellWidth::Wide, style).unwrap();
    let options = RenderOptions {
        cell_width: 10,
        cell_height: 20,
        padding: Padding::all(0),
        ..RenderOptions::default()
    };
    let bytes = Renderer::new(options).unwrap().render_png(&grid).unwrap();
    let (width, height, pixels) = decode(&bytes);
    let glyph_region_has_ink = (0..height).any(|y| {
        (10..30).any(|x| {
            let index = (y * width + x) as usize * 4;
            pixels[index..index + 3] != [background.r, background.g, background.b]
        })
    });
    assert!(
        glyph_region_has_ink,
        "the two-cell footprint contains no glyph ink"
    );
    let outside = ttyshot_core::Theme::dark().background;
    for y in 0..height {
        for x in 30..40 {
            let index = (y * width + x) as usize * 4;
            assert_eq!(
                &pixels[index..index + 3],
                &[outside.r, outside.g, outside.b]
            );
        }
    }
}

#[test]
fn repeat_render_is_byte_identical_for_png_and_svg() {
    let options = RenderOptions {
        chrome: WindowChrome::MacOs,
        shadow: true,
        ..RenderOptions::default()
    };
    let renderer = Renderer::new(options).unwrap();
    let grid = stress_grid();
    assert_eq!(
        renderer.render_png(&grid).unwrap(),
        renderer.render_png(&grid).unwrap()
    );
    assert_eq!(
        renderer.render_svg(&grid).unwrap(),
        renderer.render_svg(&grid).unwrap()
    );
}

#[test]
fn every_procedural_range_renders_at_one_and_two_x() {
    let ranges = [
        (0x2500, 0x257f),
        (0x2580, 0x259f),
        (0x2800, 0x28ff),
        (0x1fb00, 0x1fb3b),
        (0x1cd00, 0x1cde5),
        (0xe0b0, 0xe0b5),
    ];
    let characters: Vec<char> = ranges
        .into_iter()
        .flat_map(|(start, end)| (start..=end).map(|code| char::from_u32(code).unwrap()))
        .collect();
    let rows = u16::try_from(characters.len().div_ceil(32)).unwrap();
    let mut grid = Grid::new(32, rows).unwrap();
    for (index, character) in characters.into_iter().enumerate() {
        grid.put(
            u16::try_from(index % 32).unwrap(),
            u16::try_from(index / 32).unwrap(),
            character.to_string(),
            CellWidth::Single,
            solid_style(Rgb::new(240, 240, 240), Rgb::new(1, 2, 3)),
        )
        .unwrap();
    }
    for scale in [1, 2] {
        let renderer = Renderer::new(RenderOptions {
            scale,
            padding: Padding::all(0),
            ..RenderOptions::default()
        })
        .unwrap();
        assert!(!renderer.render_png(&grid).unwrap().is_empty());
        assert!(renderer.render_svg(&grid).unwrap().ends_with("</svg>"));
    }
}
