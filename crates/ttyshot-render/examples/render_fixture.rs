#![allow(clippy::too_many_lines)]

use std::{env, error::Error, fs, path::PathBuf};
use ttyshot_core::{Attributes, CellWidth, Color, Grid, Rgb, Style, Underline};
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
            theme,
            cell_width: 10,
            cell_height: 20,
            padding: Padding::all(18),
            chrome: WindowChrome::MacOs,
            shadow: true,
            ..RenderOptions::default()
        })?;
        fs::write(
            output.join(format!("fixture-{name}.png")),
            renderer.render_png(&grid)?,
        )?;
        fs::write(
            output.join(format!("fixture-{name}.svg")),
            renderer.render_svg(&grid)?,
        )?;
    }
    println!(
        "wrote synthetic PNG and SVG fixtures to {}",
        output.display()
    );
    Ok(())
}

fn fixture() -> Result<Grid, ttyshot_core::GridError> {
    let mut grid = Grid::new(56, 15)?;
    put_text(
        &mut grid,
        2,
        1,
        "ttyshot renderer · synthetic QA",
        Style {
            foreground: Color::Indexed(14),
            attributes: Attributes {
                bold: true,
                ..Attributes::default()
            },
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        2,
        3,
        "┌────────── procedural geometry ──────────┐",
        Style {
            foreground: Color::Indexed(4),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        2,
        4,
        "│ blocks  ▁▂▃▄▅▆▇█  braille ⠁⠉⠛⠿⣿ │",
        Style {
            foreground: Color::Indexed(10),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        2,
        5,
        "│ mixed   ├┯╋┷┫ ╔═╦═╗  powerline  │",
        Style {
            foreground: Color::Indexed(11),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        2,
        6,
        "└────────────────────────────────────────┘",
        Style {
            foreground: Color::Indexed(4),
            ..Style::default()
        },
    )?;
    for index in 0..16_u16 {
        grid.put(
            2 + index * 3,
            8,
            " ",
            CellWidth::Single,
            Style {
                background: Color::Indexed(u8::try_from(index).unwrap()),
                ..Style::default()
            },
        )?;
        grid.put(
            3 + index * 3,
            8,
            " ",
            CellWidth::Single,
            Style {
                background: Color::Indexed(u8::try_from(index).unwrap()),
                ..Style::default()
            },
        )?;
    }
    put_text(
        &mut grid,
        2,
        10,
        "bold dim italic underline strike reverse",
        Style {
            foreground: Color::Indexed(15),
            background: Color::Indexed(17),
            attributes: Attributes {
                underline: Underline::Curly,
                ..Attributes::default()
            },
            ..Style::default()
        },
    )?;
    grid.put(
        2,
        12,
        "界",
        CellWidth::Wide,
        Style {
            foreground: Color::Rgb(Rgb::new(255, 210, 90)),
            background: Color::Indexed(17),
            ..Style::default()
        },
    )?;
    put_text(
        &mut grid,
        5,
        12,
        "wide glyph occupies exactly two cells",
        Style {
            foreground: Color::Indexed(8),
            ..Style::default()
        },
    )?;
    Ok(grid)
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
