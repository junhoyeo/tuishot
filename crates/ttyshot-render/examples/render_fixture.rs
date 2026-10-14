#![allow(clippy::too_many_lines)]

use std::{env, error::Error, fs, path::PathBuf};
use ttyshot_core::{
    Attributes, Blink, CellRect, CellWidth, Color, Frame, Grid, Ideogram, Rgb, Style, Underline,
};
use ttyshot_render::{Padding, RenderOptions, Renderer, WindowChrome};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/ttyshot-render-qa"), PathBuf::from);
    fs::create_dir_all(&output)?;
    let grid = fixture()?;
    for (name, theme) in [
        ("dark", ttyshot_core::Theme::dark()),
        ("light", ttyshot_core::Theme::light()),
    ] {
        let renderer = Renderer::new(RenderOptions {
            theme: theme.clone(),
            cell_width: 10,
            cell_height: 20,
            scale: 2,
            padding: Padding::all(18),
            chrome: WindowChrome::MacOs,
            shadow: true,
            ..RenderOptions::default()
        })?;
        let png = renderer.render_png(&grid)?;
        let svg = renderer.render_svg(&grid)?;
        fs::write(output.join(format!("fixture-{name}-2x.png")), png)?;
        fs::write(
            output.join(format!("fixture-{name}-2x.svg")),
            svg.as_bytes(),
        )?;
        println!("fixture-{name}-2x.svg: {} bytes", svg.len());

        let crop_renderer = Renderer::new(RenderOptions {
            theme,
            cell_width: 10,
            cell_height: 20,
            scale: 2,
            padding: Padding::all(8),
            ..RenderOptions::default()
        })?;
        fs::write(
            output.join(format!("box-{name}-2x.png")),
            crop_renderer.render_png(&grid.crop(CellRect {
                x: 1,
                y: 2,
                width: 76,
                height: 7,
            })?)?,
        )?;
        fs::write(
            output.join(format!("styles-{name}-2x.png")),
            crop_renderer.render_png(&grid.crop(CellRect {
                x: 1,
                y: 9,
                width: 92,
                height: 17,
            })?)?,
        )?;
        fs::write(
            output.join(format!("cjk-{name}-2x.png")),
            crop_renderer.render_png(&grid.crop(CellRect {
                x: 1,
                y: 27,
                width: 60,
                height: 3,
            })?)?,
        )?;
    }
    println!(
        "wrote synthetic PNG and SVG fixtures to {}",
        output.display()
    );
    Ok(())
}

fn fixture() -> Result<Grid, ttyshot_core::GridError> {
    let mut grid = Grid::new(96, 31)?;
    put_text(
        &mut grid,
        2,
        1,
        "ttyshot renderer · synthetic QA (2x; blink = visible phase)",
        heading(),
    )?;

    let box_style = Style {
        foreground: Color::Indexed(4),
        ..Style::default()
    };
    let inner_width = 72;
    put_text(
        &mut grid,
        2,
        3,
        &format!("┌{}┐", "─".repeat(inner_width)),
        box_style,
    )?;
    put_box_row(
        &mut grid,
        2,
        4,
        inner_width,
        "procedural geometry",
        box_style,
    )?;
    put_box_row(
        &mut grid,
        2,
        5,
        inner_width,
        "blocks ▁▂▃▄▅▆▇█  braille ⠁⠉⠛⠿⣿  powerline ",
        box_style,
    )?;
    put_box_row(
        &mut grid,
        2,
        6,
        inner_width,
        "mixed ├┯╋┷┫ ╔═╦═╗  sextant 🬜",
        box_style,
    )?;
    put_text(
        &mut grid,
        2,
        7,
        &format!("└{}┘", "─".repeat(inner_width)),
        box_style,
    )?;

    for index in 0..16_u16 {
        for offset in 0..2 {
            grid.put(
                2 + index * 3 + offset,
                8,
                " ",
                CellWidth::Single,
                Style {
                    background: Color::Indexed(u8::try_from(index).unwrap()),
                    ..Style::default()
                },
            )?;
        }
    }

    put_sample(
        &mut grid,
        2,
        10,
        "bold",
        "BOLD",
        Attributes {
            bold: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        10,
        "italic",
        "ITALIC",
        Attributes {
            italic: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        10,
        "dim",
        "DIM",
        Attributes {
            dim: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_sample(
        &mut grid,
        2,
        12,
        "reverse",
        "REVERSE",
        Attributes {
            reverse: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_hidden_sample(&mut grid, 33, 12)?;
    put_sample(
        &mut grid,
        64,
        12,
        "strike",
        "STRIKE",
        Attributes {
            strikethrough: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_sample(
        &mut grid,
        2,
        14,
        "overline",
        "OVERLINE",
        Attributes {
            overline: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        14,
        "framed",
        "FRAME",
        Attributes {
            frame: Frame::Framed,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        14,
        "encircled",
        "CIRCLE",
        Attributes {
            frame: Frame::Encircled,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_sample(
        &mut grid,
        2,
        16,
        "underline single",
        "SINGLE",
        Attributes {
            underline: Underline::Single,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        16,
        "underline double",
        "DOUBLE",
        Attributes {
            underline: Underline::Double,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        16,
        "underline curly",
        "CURLY",
        Attributes {
            underline: Underline::Curly,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_sample(
        &mut grid,
        2,
        18,
        "underline dotted",
        "DOTTED",
        Attributes {
            underline: Underline::Dotted,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        18,
        "underline dashed",
        "DASHED",
        Attributes {
            underline: Underline::Dashed,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        18,
        "underline color",
        "MAGENTA",
        Attributes {
            underline: Underline::Single,
            ..Attributes::default()
        },
        Color::Rgb(Rgb::new(255, 80, 210)),
    )?;

    put_sample(
        &mut grid,
        2,
        20,
        "blink slow (visible)",
        "SLOW",
        Attributes {
            blink: Blink::Slow,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        20,
        "blink rapid (visible)",
        "RAPID",
        Attributes {
            blink: Blink::Rapid,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        20,
        "combo",
        "BOLD ITALIC",
        Attributes {
            bold: true,
            italic: true,
            underline: Underline::Curly,
            ..Attributes::default()
        },
        Color::Indexed(14),
    )?;

    put_sample(
        &mut grid,
        2,
        22,
        "ideogram under",
        "UNDER",
        Attributes {
            ideogram: Ideogram::Underline,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        22,
        "ideogram double under",
        "DOUBLE",
        Attributes {
            ideogram: Ideogram::DoubleUnderline,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        22,
        "ideogram over",
        "OVER",
        Attributes {
            ideogram: Ideogram::Overline,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_sample(
        &mut grid,
        2,
        24,
        "ideogram double over",
        "DOUBLE",
        Attributes {
            ideogram: Ideogram::DoubleOverline,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        33,
        24,
        "ideogram stress",
        "STRESS",
        Attributes {
            ideogram: Ideogram::Stress,
            ..Attributes::default()
        },
        Color::Default,
    )?;
    put_sample(
        &mut grid,
        64,
        24,
        "combo",
        "REVERSE STRIKE",
        Attributes {
            reverse: true,
            strikethrough: true,
            bold: true,
            ..Attributes::default()
        },
        Color::Default,
    )?;

    put_text(
        &mut grid,
        2,
        27,
        "system CJK fallback:",
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    put_wide_text(
        &mut grid,
        23,
        27,
        "한글 렌더링 테스트",
        Style {
            foreground: Color::Rgb(Rgb::new(255, 210, 90)),
            background: Color::Indexed(17),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        2,
        29,
        "wide-cell contract:",
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    grid.put(
        22,
        29,
        "界",
        CellWidth::Wide,
        Style {
            foreground: Color::Indexed(10),
            background: Color::Indexed(17),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        25,
        29,
        "occupies exactly two cells",
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    Ok(grid)
}

fn heading() -> Style {
    Style {
        foreground: Color::Indexed(14),
        attributes: Attributes {
            bold: true,
            ..Attributes::default()
        },
        ..Style::default()
    }
}

fn sample_style(attributes: Attributes, underline_color: Color) -> Style {
    Style {
        foreground: Color::Indexed(15),
        background: Color::Indexed(17),
        underline_color,
        attributes,
    }
}

fn put_sample(
    grid: &mut Grid,
    x: u16,
    y: u16,
    label: &str,
    word: &str,
    attributes: Attributes,
    underline_color: Color,
) -> Result<(), ttyshot_core::GridError> {
    let label_text = format!("{label}: ");
    put_text(
        grid,
        x,
        y,
        &label_text,
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    put_text(
        grid,
        x + u16::try_from(label_text.chars().count()).unwrap(),
        y,
        word,
        sample_style(attributes, underline_color),
    )
}

fn put_hidden_sample(grid: &mut Grid, x: u16, y: u16) -> Result<(), ttyshot_core::GridError> {
    let label = "hidden: [";
    put_text(
        grid,
        x,
        y,
        label,
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    let hidden_x = x + u16::try_from(label.chars().count()).unwrap();
    put_text(
        grid,
        hidden_x,
        y,
        "HIDDEN",
        sample_style(
            Attributes {
                hidden: true,
                ..Attributes::default()
            },
            Color::Default,
        ),
    )?;
    put_text(
        grid,
        hidden_x + 6,
        y,
        "]",
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )
}

fn put_box_row(
    grid: &mut Grid,
    x: u16,
    y: u16,
    inner_width: usize,
    content: &str,
    style: Style,
) -> Result<(), ttyshot_core::GridError> {
    assert!(content.chars().count() <= inner_width);
    let row = format!("│{content:<inner_width$}│");
    put_text(grid, x, y, &row, style)
}

fn put_wide_text(
    grid: &mut Grid,
    mut x: u16,
    y: u16,
    text: &str,
    style: Style,
) -> Result<(), ttyshot_core::GridError> {
    for character in text.chars() {
        let width = if character.is_ascii() {
            CellWidth::Single
        } else {
            CellWidth::Wide
        };
        grid.put(x, y, character.to_string(), width, style)?;
        x += if width == CellWidth::Wide { 2 } else { 1 };
    }
    Ok(())
}

fn put_text(
    grid: &mut Grid,
    mut x: u16,
    y: u16,
    text: &str,
    style: Style,
) -> Result<(), ttyshot_core::GridError> {
    for character in text.chars() {
        if x >= grid.columns() {
            break;
        }
        grid.put(x, y, character.to_string(), CellWidth::Single, style)?;
        x += 1;
    }
    Ok(())
}
