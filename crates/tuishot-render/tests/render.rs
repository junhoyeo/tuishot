#![allow(clippy::too_many_lines)]

use sha2::{Digest, Sha256};
use std::io::Cursor;
use tuishot_core::{
    Attributes, Blink, CellWidth, Color, Frame, Grid, Ideogram, Rgb, Style, Underline,
};
use tuishot_render::{
    BUNDLED_FONT_LICENSE, BUNDLED_FONT_MANIFEST, BUNDLED_FONT_SHA256, BUNDLED_FONT_SOURCE,
    BUNDLED_FONT_VERSION, FontMode, FontSource, Padding, RenderError, RenderOptions, Renderer,
    WindowChrome,
};

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

fn ink_center_hundredths(row: &[u8]) -> Option<i64> {
    let xs = row
        .chunks(4)
        .enumerate()
        .filter(|(_, pixel)| pixel[0] > 20)
        .map(|(x, _)| i64::try_from(x).expect("test image width fits i64"))
        .collect::<Vec<_>>();
    (!xs.is_empty()).then(|| {
        xs.iter().sum::<i64>() * 100 / i64::try_from(xs.len()).expect("test ink count fits i64")
    })
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
        '\u{e0b4}',
        '\u{e0b5}',
        '\u{e0b6}',
        '\u{e0b7}',
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
        font_mode: FontMode::Deterministic,
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
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        };
        let bytes = Renderer::new(options).unwrap().render_png(&grid).unwrap();
        let (_, _, pixels) = decode(&bytes);
        for pixel in pixels.chunks(4) {
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
        font_mode: FontMode::Deterministic,
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
        font_mode: FontMode::Deterministic,
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
    let outside = tuishot_core::Theme::dark().background;
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
        font_mode: FontMode::Deterministic,
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
        (0xe0b0, 0xe0b7),
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
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap();
        assert!(!renderer.render_png(&grid).unwrap().is_empty());
        assert!(renderer.render_svg(&grid).unwrap().ends_with("</svg>"));
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = usize::try_from((y * width + x) * 4).unwrap();
    pixels[index..index + 4].try_into().unwrap()
}

#[test]
fn closed_box_has_continuous_edges_and_corners_at_one_and_two_x() {
    let foreground = Rgb::new(250, 100, 40);
    let background = Rgb::new(3, 5, 7);
    let style = solid_style(foreground, background);
    let rows = ["┌──────┐", "│      │", "│      │", "└──────┘"];
    let mut grid = Grid::new(8, 4).unwrap();
    for (y, row) in rows.into_iter().enumerate() {
        for (x, character) in row.chars().enumerate() {
            grid.put(
                u16::try_from(x).unwrap(),
                u16::try_from(y).unwrap(),
                character.to_string(),
                CellWidth::Single,
                style,
            )
            .unwrap();
        }
    }
    for scale in [1, 2] {
        let cell_width = 10 * u32::from(scale);
        let cell_height = 20 * u32::from(scale);
        let png = Renderer::new(RenderOptions {
            cell_width: 10,
            cell_height: 20,
            scale,
            padding: Padding::all(0),
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap()
        .render_png(&grid)
        .unwrap();
        let (width, _, pixels) = decode(&png);
        let left = cell_width / 2;
        let right = 7 * cell_width + cell_width / 2;
        let top = cell_height / 2;
        let bottom = 3 * cell_height + cell_height / 2;
        let ink = [foreground.r, foreground.g, foreground.b, 255];
        for x in left..=right {
            assert_eq!(
                pixel(&pixels, width, x, top),
                ink,
                "top edge gap at {x}, scale {scale}"
            );
            assert_eq!(
                pixel(&pixels, width, x, bottom),
                ink,
                "bottom edge gap at {x}, scale {scale}"
            );
        }
        for y in top..=bottom {
            assert_eq!(
                pixel(&pixels, width, left, y),
                ink,
                "left edge gap at {y}, scale {scale}"
            );
            assert_eq!(
                pixel(&pixels, width, right, y),
                ink,
                "right edge gap at {y}, scale {scale}"
            );
        }
        for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
            assert_eq!(
                pixel(&pixels, width, x, y),
                ink,
                "corner missing at {x},{y}, scale {scale}"
            );
        }
    }
}

fn render_attribute(attributes: Attributes, scale: u8) -> (u32, u32, Vec<u8>) {
    let mut grid = Grid::new(1, 1).unwrap();
    grid.put(
        0,
        0,
        "I",
        CellWidth::Single,
        Style {
            foreground: Color::Rgb(Rgb::new(255, 255, 255)),
            background: Color::Rgb(Rgb::new(0, 0, 0)),
            underline_color: Color::Rgb(Rgb::new(255, 0, 255)),
            attributes,
        },
    )
    .unwrap();
    let png = Renderer::new(RenderOptions {
        cell_width: 20,
        cell_height: 24,
        font_size: 18.0,
        scale,
        padding: Padding::all(0),
        font_mode: FontMode::Deterministic,
        ..RenderOptions::default()
    })
    .unwrap()
    .render_png(&grid)
    .unwrap();
    decode(&png)
}

#[test]
fn dim_and_synthetic_italic_are_measurably_distinct() {
    let (_, _, normal) = render_attribute(Attributes::default(), 2);
    let (_, _, dim) = render_attribute(
        Attributes {
            dim: true,
            ..Attributes::default()
        },
        2,
    );
    let normal_energy: u64 = normal.chunks(4).map(|p| u64::from(p[0])).sum();
    let dim_energy: u64 = dim.chunks(4).map(|p| u64::from(p[0])).sum();
    assert!(
        dim_energy * 10 < normal_energy * 7,
        "dim ink must be substantially darker"
    );

    let (width, height, italic) = render_attribute(
        Attributes {
            italic: true,
            ..Attributes::default()
        },
        2,
    );
    let row_stride = usize::try_from(width * 4).unwrap();
    let centers = (0..height)
        .filter_map(|y| {
            ink_center_hundredths(
                &italic[usize::try_from(y).unwrap() * row_stride
                    ..usize::try_from(y + 1).unwrap() * row_stride],
            )
        })
        .collect::<Vec<_>>();
    assert!(
        centers.first().unwrap() - centers.last().unwrap() >= 300,
        "italic top must be displaced right"
    );
}

#[test]
fn strike_and_all_underline_patterns_land_on_exact_rows() {
    for scale in [1, 2] {
        let s = u32::from(scale);
        let (width, height, strike) = render_attribute(
            Attributes {
                strikethrough: true,
                ..Attributes::default()
            },
            scale,
        );
        let strike_y = height / 2;
        assert!((0..width).all(|x| pixel(&strike, width, x, strike_y)[0] == 255));
        assert!((0..width).any(|x| pixel(&strike, width, x, strike_y - s)[0] == 0));

        let patterns = [
            Underline::Single,
            Underline::Double,
            Underline::Curly,
            Underline::Dotted,
            Underline::Dashed,
        ];
        let rendered = patterns.map(|underline| {
            render_attribute(
                Attributes {
                    underline,
                    ..Attributes::default()
                },
                scale,
            )
        });
        let magenta = [255, 0, 255, 255];
        let row = height - 2 * s;
        assert!((0..width).all(|x| pixel(&rendered[0].2, width, x, row) == magenta));
        assert!((0..width).all(|x| pixel(&rendered[1].2, width, x, row) == magenta));
        assert!((0..width).all(|x| pixel(&rendered[1].2, width, x, height - 4 * s) == magenta));
        assert!((0..width).any(|x| pixel(&rendered[1].2, width, x, height - 3 * s)[0] == 0));
        let curly_rows = (0..width)
            .map(|x| i32::from(pixel(&rendered[2].2, width, x, height - 3 * s) != magenta))
            .collect::<Vec<_>>();
        assert!(
            curly_rows
                .windows(2 * usize::try_from(s).unwrap())
                .any(|window| window[0] != window[window.len() - 1])
        );
        let dotted = (0..width)
            .map(|x| pixel(&rendered[3].2, width, x, row) == magenta)
            .collect::<Vec<_>>();
        assert!(dotted.iter().any(|value| *value) && dotted.iter().any(|value| !*value));
        let dashed = (0..width)
            .map(|x| pixel(&rendered[4].2, width, x, row) == magenta)
            .collect::<Vec<_>>();
        assert!(
            dashed
                .windows(usize::try_from(5 * s).unwrap())
                .any(|window| window.iter().filter(|v| **v).count()
                    == usize::try_from(3 * s).unwrap())
        );
    }
}

#[test]
fn svg_curly_underline_repeats_png_period_across_a_wide_cell() {
    for scale in [1, 2] {
        let mut grid = Grid::new(2, 1).unwrap();
        grid.put(
            0,
            0,
            "W",
            CellWidth::Wide,
            Style {
                foreground: Color::Rgb(Rgb::new(255, 255, 255)),
                background: Color::Rgb(tuishot_core::Theme::dark().background),
                underline_color: Color::Rgb(Rgb::new(255, 0, 255)),
                attributes: Attributes {
                    underline: Underline::Curly,
                    ..Attributes::default()
                },
            },
        )
        .unwrap();
        let svg = Renderer::new(RenderOptions {
            cell_width: 12,
            cell_height: 20,
            scale,
            padding: Padding::all(0),
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap()
        .render_svg(&grid)
        .unwrap();
        assert_eq!(
            svg.matches("fill=\"#ff00ff\"").count(),
            12,
            "two-cell span must contain twelve alternating 2-logical-pixel runs"
        );
        let s = i32::from(scale);
        assert!(svg.contains(&format!("y=\"{}\"", 17 * s)));
        assert!(svg.contains(&format!("y=\"{}\"", 18 * s)));
        assert!(
            !svg.contains(" q "),
            "curly underline must not stretch one quadratic"
        );
    }
}

#[test]
fn terminal_scrollbar_glyphs_render_as_filled_geometry_at_one_and_two_x() {
    let foreground = Rgb::new(240, 245, 250);
    let background = Rgb::new(2, 3, 5);
    for scale in [1, 2] {
        let mut grid = Grid::new(3, 1).unwrap();
        for (x, character) in ['▴', '▮', '▾'].into_iter().enumerate() {
            grid.put(
                u16::try_from(x).unwrap(),
                0,
                character.to_string(),
                CellWidth::Single,
                solid_style(foreground, background),
            )
            .unwrap();
        }
        let png = Renderer::new(RenderOptions {
            cell_width: 12,
            cell_height: 24,
            scale,
            padding: Padding::all(0),
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap()
        .render_png(&grid)
        .unwrap();
        let (width, height, pixels) = decode(&png);
        let cell_width = 12 * u32::from(scale);
        let ink = [foreground.r, foreground.g, foreground.b, 255];
        let blank = [background.r, background.g, background.b, 255];

        assert_eq!(
            pixel(&pixels, width, cell_width + cell_width / 2, height / 2),
            ink
        );
        assert_eq!(pixel(&pixels, width, cell_width, 0), blank);
        assert_eq!(pixel(&pixels, width, cell_width / 2, height / 4), ink);
        assert_eq!(
            pixel(
                &pixels,
                width,
                2 * cell_width + cell_width / 2,
                3 * height / 4 - 1
            ),
            ink
        );
    }
}

#[test]
fn every_powerline_separator_has_distinct_scale_safe_geometry() {
    let foreground = Rgb::new(247, 80, 120);
    let background = Rgb::new(2, 3, 5);
    for scale in [1, 2] {
        let mut masks = Vec::new();
        let cell_width = 12 * u32::from(scale);
        let cell_height = 24 * u32::from(scale);
        for codepoint in 0xe0b0..=0xe0b7 {
            let character = char::from_u32(codepoint).unwrap();
            let mut grid = Grid::new(1, 1).unwrap();
            grid.put(
                0,
                0,
                character.to_string(),
                CellWidth::Single,
                solid_style(foreground, background),
            )
            .unwrap();
            let png = Renderer::new(RenderOptions {
                cell_width: 12,
                cell_height: 24,
                scale,
                padding: Padding::all(0),
                font_mode: FontMode::Deterministic,
                ..RenderOptions::default()
            })
            .unwrap()
            .render_png(&grid)
            .unwrap();
            let (width, height, pixels) = decode(&png);
            assert_eq!((width, height), (cell_width, cell_height));
            let mask = pixels
                .chunks(4)
                .map(|pixel| pixel[0] == foreground.r)
                .collect::<Vec<_>>();
            assert!(
                mask.iter().any(|value| *value),
                "U+{codepoint:04X} is empty at {scale}x"
            );
            masks.push(mask);
        }
        for left in 0..masks.len() {
            for right in left + 1..masks.len() {
                assert_ne!(
                    masks[left],
                    masks[right],
                    "U+{:04X} and U+{:04X} alias at {scale}x",
                    0xe0b0 + left,
                    0xe0b0 + right
                );
            }
        }

        let is_ink = |glyph: usize, x: u32, y: u32| {
            masks[glyph][usize::try_from(y * cell_width + x).unwrap()]
        };
        // Filled right-facing separators have a seam-free left diameter.
        for glyph in [0_usize, 4] {
            assert!((1..cell_height - 1).all(|y| is_ink(glyph, 0, y)));
        }
        // Filled left-facing separators have a seam-free right diameter.
        for glyph in [2_usize, 6] {
            assert!((1..cell_height - 1).all(|y| is_ink(glyph, cell_width - 1, y)));
        }
        // All eight shapes reach their directional tip at the middle row.
        for glyph in [0_usize, 1, 4, 5] {
            assert!(
                is_ink(glyph, cell_width - 1, cell_height / 2),
                "right tip missing for U+{:04X} at {scale}x",
                0xe0b0 + glyph
            );
        }
        for glyph in [2_usize, 3, 6, 7] {
            assert!(
                is_ink(glyph, 0, cell_height / 2),
                "left tip missing for U+{:04X} at {scale}x",
                0xe0b0 + glyph
            );
        }
        // A rounded thick separator has more shoulder ink than a triangle.
        assert!(is_ink(4, 2 * cell_width / 3, cell_height / 4));
        assert!(!is_ink(0, 2 * cell_width / 3, cell_height / 4));
    }
}

#[test]
fn combining_mark_stays_within_its_cell_and_is_shared_by_png_and_svg() {
    let style = solid_style(Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
    let render = |symbol: &str| {
        let mut grid = Grid::new(1, 1).unwrap();
        grid.put(0, 0, symbol, CellWidth::Single, style).unwrap();
        let renderer = Renderer::new(RenderOptions {
            cell_width: 20,
            cell_height: 28,
            font_size: 20.0,
            padding: Padding::all(0),
            font_mode: FontMode::Deterministic,
            ..RenderOptions::default()
        })
        .unwrap();
        (
            renderer.render_png(&grid).unwrap(),
            renderer.render_svg(&grid).unwrap(),
        )
    };
    let (base_png, _) = render("e");
    let (combined_png, combined_svg) = render("e\u{301}");
    assert_ne!(
        base_png, combined_png,
        "the supported acute mark must add ink"
    );
    assert_eq!(combined_svg.matches("<use href=\"#g").count(), 2);
    assert!(!combined_svg.contains("<clipPath"));
    assert!(combined_svg.contains("overflow=\"hidden\""));
    let (width, height, pixels) = decode(&combined_png);
    assert_eq!((width, height), (20, 28));
    assert!(pixels.chunks(4).any(|pixel| pixel[0] > 20));
}

#[test]
fn synthetic_bold_and_oblique_displacement_scale_with_output_scale() {
    let mut bold_growth = Vec::new();
    let mut italic_displacement = Vec::new();
    for scale in [1, 2] {
        let (width, height, normal) = render_attribute(Attributes::default(), scale);
        let (_, _, bold) = render_attribute(
            Attributes {
                bold: true,
                ..Attributes::default()
            },
            scale,
        );
        let row_width = |pixels: &[u8], y: u32| {
            (0..width)
                .filter(|x| pixel(pixels, width, *x, y)[0] > 20)
                .count()
        };
        let normal_max = (0..height).map(|y| row_width(&normal, y)).max().unwrap();
        let bold_max = (0..height).map(|y| row_width(&bold, y)).max().unwrap();
        bold_growth.push(bold_max - normal_max);

        let (_, _, italic) = render_attribute(
            Attributes {
                italic: true,
                ..Attributes::default()
            },
            scale,
        );
        let centers = (0..height)
            .filter_map(|y| {
                let start = usize::try_from(y * width * 4).unwrap();
                let end = usize::try_from((y + 1) * width * 4).unwrap();
                ink_center_hundredths(&italic[start..end])
            })
            .collect::<Vec<_>>();
        italic_displacement.push(centers.first().unwrap() - centers.last().unwrap());
    }
    assert!(bold_growth[0] >= 1);
    assert!(bold_growth[1] >= 2);
    assert!(italic_displacement[0] >= 200);
    assert!(italic_displacement[1] >= 500);
}

fn qa_equivalent_size_grid() -> Grid {
    let mut grid = Grid::new(96, 31).unwrap();
    let foreground = Color::Rgb(Rgb::new(235, 238, 245));
    let background = Color::Rgb(tuishot_core::Theme::dark().background);
    let labels = [
        "tuishot synthetic QA: procedural geometry and full SGR style coverage",
        "box ┌────────┐ blocks ▁▂▃▄▅▆▇█ braille ⠁⠉⠛⠿⣿ powerline ",
        "bold italic dim reverse hidden strike overline framed encircled",
        "single double curly dotted dashed underline color visible blink",
        "ideogram under double-under over double-over stress combinations",
    ];
    for (row, label) in labels.into_iter().enumerate() {
        for (column, character) in label.chars().enumerate() {
            grid.put(
                u16::try_from(column).unwrap(),
                u16::try_from(row * 3).unwrap(),
                character.to_string(),
                CellWidth::Single,
                Style {
                    foreground,
                    background,
                    ..Style::default()
                },
            )
            .unwrap();
        }
    }
    let attributes = [
        Attributes {
            bold: true,
            ..Attributes::default()
        },
        Attributes {
            italic: true,
            ..Attributes::default()
        },
        Attributes {
            dim: true,
            ..Attributes::default()
        },
        Attributes {
            reverse: true,
            ..Attributes::default()
        },
        Attributes {
            strikethrough: true,
            ..Attributes::default()
        },
        Attributes {
            overline: true,
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
            frame: Frame::Framed,
            ..Attributes::default()
        },
        Attributes {
            frame: Frame::Encircled,
            ..Attributes::default()
        },
    ];
    for (index, attributes) in attributes.into_iter().enumerate() {
        let x = u16::try_from((index % 5) * 18).unwrap();
        let y = 16 + u16::try_from((index / 5) * 3).unwrap();
        grid.put(
            x,
            y,
            "STYLE",
            CellWidth::Single,
            Style {
                foreground,
                background: Color::Indexed(17),
                underline_color: Color::Rgb(Rgb::new(255, 80, 210)),
                attributes,
            },
        )
        .unwrap();
    }
    let mut x = 2;
    for character in "한글 렌더링 테스트".chars() {
        let width = if character.is_ascii() {
            CellWidth::Single
        } else {
            CellWidth::Wide
        };
        grid.put(
            x,
            27,
            character.to_string(),
            width,
            Style {
                foreground: Color::Rgb(Rgb::new(255, 210, 90)),
                background: Color::Indexed(17),
                ..Style::default()
            },
        )
        .unwrap();
        x += if width == CellWidth::Wide { 2 } else { 1 };
    }
    grid
}

#[test]
fn qa_equivalent_svg_has_a_portable_size_ceiling() {
    let renderer = Renderer::new(RenderOptions {
        scale: 2,
        font_mode: FontMode::Deterministic,
        ..RenderOptions::default()
    })
    .unwrap();
    let svg = renderer.render_svg(&qa_equivalent_size_grid()).unwrap();
    assert!(!svg.contains("data:font"));
    assert!(
        svg.len() < 400_000,
        "QA-equivalent SVG exceeded the 400 KB regression ceiling: {} bytes",
        svg.len()
    );
}

// NONPORTABLE: verifies local system CJK outline size, never a byte golden.
#[cfg(target_os = "macos")]
#[test]
fn qa_equivalent_system_cjk_svg_stays_below_original_size() {
    let renderer = Renderer::new(RenderOptions {
        scale: 2,
        ..RenderOptions::default()
    })
    .unwrap();
    let svg = renderer.render_svg(&qa_equivalent_size_grid()).unwrap();
    assert!(
        svg.len() < 400_000,
        "system-CJK QA SVG must stay well below the original 609 KB: {} bytes",
        svg.len()
    );
}

#[test]
fn svg_uses_reusable_glyph_paths_and_stays_small() {
    let renderer = Renderer::new(RenderOptions {
        font_mode: FontMode::Deterministic,
        ..RenderOptions::default()
    })
    .unwrap();
    let svg = renderer.render_svg(&stress_grid()).unwrap();
    assert!(svg.contains("<path id=\"g"));
    assert!(svg.contains("<use href=\"#g"));
    assert!(!svg.contains("data:font"));
    assert!(
        svg.len() < 300_000,
        "SVG is unexpectedly large: {} bytes",
        svg.len()
    );
}

#[test]
fn option_boundaries_fail_without_panics_or_integer_wrap() {
    for font_size in [
        0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
    ] {
        assert!(matches!(
            Renderer::new(RenderOptions {
                font_size,
                ..RenderOptions::default()
            }),
            Err(RenderError::InvalidOptions(_))
        ));
    }
    for scale in [0, 5] {
        assert!(matches!(
            Renderer::new(RenderOptions {
                scale,
                ..RenderOptions::default()
            }),
            Err(RenderError::InvalidOptions(_))
        ));
    }
    let grid = Grid::new(1, 1).unwrap();
    let renderer = Renderer::new(RenderOptions {
        padding: Padding {
            top: u16::MAX,
            right: u16::MAX,
            bottom: u16::MAX,
            left: u16::MAX,
        },
        scale: 4,
        ..RenderOptions::default()
    })
    .unwrap();
    assert!(matches!(
        renderer.render_png(&grid),
        Err(RenderError::CanvasTooLarge { .. } | RenderError::DimensionOverflow)
    ));
    let renderer = Renderer::new(RenderOptions {
        cell_width: u16::MAX,
        cell_height: u16::MAX,
        scale: 4,
        padding: Padding::all(0),
        ..RenderOptions::default()
    })
    .unwrap();
    assert!(matches!(
        renderer.render_png(&grid),
        Err(RenderError::CanvasTooLarge { .. } | RenderError::DimensionOverflow)
    ));
}

#[test]
fn bundled_font_hash_license_and_provenance_are_pinned() {
    let digest = format!(
        "{:x}",
        Sha256::digest(include_bytes!("../assets/JetBrainsMono-Regular.ttf"))
    );
    assert_eq!(digest, BUNDLED_FONT_SHA256);
    assert_eq!(BUNDLED_FONT_VERSION, "2.304");
    assert_eq!(
        BUNDLED_FONT_SOURCE,
        "https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304"
    );
    assert_eq!(
        BUNDLED_FONT_LICENSE,
        include_str!("../assets/OFL-JetBrainsMono.txt")
    );
    assert!(BUNDLED_FONT_LICENSE.starts_with("Copyright 2020 The JetBrains Mono Project Authors"));
    assert!(BUNDLED_FONT_LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert_eq!(
        BUNDLED_FONT_MANIFEST,
        include_str!("../assets/FONT-LICENSES.toml")
    );
    assert!(BUNDLED_FONT_MANIFEST.contains(&format!("sha256 = \"{digest}\"")));
    assert!(BUNDLED_FONT_MANIFEST.contains(&format!("source = \"{BUNDLED_FONT_SOURCE}\"")));
    assert!(BUNDLED_FONT_MANIFEST.contains("license = \"OFL-1.1\""));
    assert!(BUNDLED_FONT_MANIFEST.contains("license_file = \"OFL-JetBrainsMono.txt\""));
}

#[test]
fn invalid_collection_index_is_reported() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/JetBrainsMono-Regular.ttf");
    let result = Renderer::new(RenderOptions {
        font: Some(FontSource::collection(path, 1)),
        font_mode: FontMode::Deterministic,
        ..RenderOptions::default()
    });
    assert!(matches!(result, Err(RenderError::InvalidFont { .. })));
}

// NONPORTABLE: platform fallback smoke test only; never used as a golden.
#[cfg(target_os = "macos")]
#[test]
fn default_macos_fallback_selects_apple_korean_font_and_renders_ink() {
    let renderer = Renderer::new(RenderOptions {
        padding: Padding::all(0),
        ..RenderOptions::default()
    })
    .unwrap();
    assert!(
        renderer
            .font_source_for('한')
            .contains("AppleSDGothicNeo.ttc")
    );
    let mut grid = Grid::new(2, 1).unwrap();
    grid.put(
        0,
        0,
        "한",
        CellWidth::Wide,
        solid_style(
            Rgb::new(255, 255, 255),
            tuishot_core::Theme::dark().background,
        ),
    )
    .unwrap();
    let (width, height, pixels) = decode(&renderer.render_png(&grid).unwrap());
    let ink = pixels.chunks(4).filter(|pixel| pixel[0] > 20).count();
    assert!(ink > 20, "Korean glyph must produce real ink");
    let half = width / 2;
    assert!(
        (0..height).any(|y| (0..half).any(|x| pixel(&pixels, width, x, y)[0] > 20)),
        "wide Korean glyph must draw in its leading cell"
    );
    assert!(
        (0..height).any(|y| (half..width).any(|x| pixel(&pixels, width, x, y)[0] > 20)),
        "wide Korean glyph must draw in its continuation cell"
    );
    let svg = renderer.render_svg(&grid).unwrap();
    assert_eq!(svg.matches("<use href=\"#g").count(), 1);
    assert_eq!(
        svg.matches("<rect").count(),
        1,
        "only the shell background remains"
    );
    let background_end = svg.find("<use href=").expect("wide glyph use");
    assert!(
        !svg[background_end..].contains("<rect"),
        "SVG background pass must not repaint either wide-cell half"
    );
}
