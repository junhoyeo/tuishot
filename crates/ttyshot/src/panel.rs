use crate::Error;
use ttyshot_core::{CellRect, CellWidth, Grid};

#[derive(Clone, Copy)]
struct Border {
    tl: &'static str,
    tr: &'static str,
    bl: &'static str,
    br: &'static str,
    h: &'static str,
    v: &'static str,
    name: &'static str,
}
const BORDERS: [Border; 5] = [
    Border {
        tl: "┌",
        tr: "┐",
        bl: "└",
        br: "┘",
        h: "─",
        v: "│",
        name: "light",
    },
    Border {
        tl: "╭",
        tr: "╮",
        bl: "╰",
        br: "╯",
        h: "─",
        v: "│",
        name: "rounded",
    },
    Border {
        tl: "┏",
        tr: "┓",
        bl: "┗",
        br: "┛",
        h: "━",
        v: "┃",
        name: "heavy",
    },
    Border {
        tl: "╔",
        tr: "╗",
        bl: "╚",
        br: "╝",
        h: "═",
        v: "║",
        name: "double",
    },
    Border {
        tl: "+",
        tr: "+",
        bl: "+",
        br: "+",
        h: "-",
        v: "|",
        name: "ascii",
    },
];
#[derive(Clone, Debug)]
struct Candidate {
    rect: CellRect,
    kind: &'static str,
}

/// Finds one complete supported border whose top edge contains `title`.
///
/// # Errors
/// Returns an error when the title is empty, missing, or ambiguous.
pub fn find_panel(grid: &Grid, title: &str) -> Result<CellRect, Error> {
    if title.is_empty() {
        return Err(Error::msg("panel title must not be empty"));
    }
    let mut candidates = Vec::new();
    for border in BORDERS {
        find_for_border(grid, title, border, &mut candidates);
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate.rect.y,
            candidate.rect.x,
            candidate.rect.width,
            candidate.rect.height,
        )
    });
    candidates.dedup_by_key(|candidate| {
        (
            candidate.rect.x,
            candidate.rect.y,
            candidate.rect.width,
            candidate.rect.height,
        )
    });
    match candidates.as_slice() {
        [candidate] => Ok(candidate.rect),
        [] => {
            let contexts = title_context(grid, title);
            let suffix = if contexts.is_empty() {
                "no screen row contains the title".to_owned()
            } else {
                format!(
                    "title text occurs on row(s) {}, but no complete supported border surrounds it",
                    contexts
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            Err(Error::msg(format!(
                "panel {title:?} was not found: {suffix}"
            )))
        }
        many => Err(Error::msg(format!(
            "panel title {title:?} is ambiguous; candidates: {}",
            many.iter()
                .map(|c| format!(
                    "{} border at {},{} {}x{}",
                    c.kind, c.rect.x, c.rect.y, c.rect.width, c.rect.height
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ))),
    }
}

fn find_for_border(grid: &Grid, title: &str, border: Border, output: &mut Vec<Candidate>) {
    for top in 0..grid.rows() {
        for left in 0..grid.columns() {
            if symbol(grid, left, top) != border.tl {
                continue;
            }
            for right in left.saturating_add(2)..grid.columns() {
                if symbol(grid, right, top) != border.tr {
                    continue;
                }
                let Some((title_start, title_end)) =
                    find_title_cells(grid, top, left + 1, right, title)
                else {
                    continue;
                };
                if !(left + 1..title_start).all(|x| is_top_fill(symbol(grid, x, top), border.h))
                    || !(title_end..right).all(|x| is_top_fill(symbol(grid, x, top), border.h))
                {
                    continue;
                }
                for bottom in top.saturating_add(1)..grid.rows() {
                    if !(top + 1..bottom).all(|y| {
                        symbol(grid, left, y) == border.v && symbol(grid, right, y) == border.v
                    }) {
                        break;
                    }
                    if symbol(grid, left, bottom) == border.bl
                        && symbol(grid, right, bottom) == border.br
                        && (left + 1..right).all(|x| symbol(grid, x, bottom) == border.h)
                    {
                        output.push(Candidate {
                            rect: CellRect {
                                x: left,
                                y: top,
                                width: right - left + 1,
                                height: bottom - top + 1,
                            },
                            kind: border.name,
                        });
                    }
                }
            }
        }
    }
}
fn is_top_fill(value: &str, horizontal: &str) -> bool {
    value == horizontal || value == " "
}
fn symbol(grid: &Grid, x: u16, y: u16) -> &str {
    &grid.get(x, y).expect("coordinates in grid").symbol
}

fn find_title_cells(grid: &Grid, y: u16, start: u16, end: u16, title: &str) -> Option<(u16, u16)> {
    for x in start..end {
        if grid.get(x, y)?.width == CellWidth::Continuation {
            continue;
        }
        let mut assembled = String::new();
        for after in x..end {
            let cell = grid.get(after, y)?;
            if cell.width != CellWidth::Continuation {
                assembled.push_str(&cell.symbol);
            }
            if assembled == title {
                return Some((x, after + 1));
            }
            if !title.starts_with(&assembled) {
                break;
            }
        }
    }
    None
}
fn title_context(grid: &Grid, title: &str) -> Vec<u16> {
    (0..grid.rows())
        .filter(|&y| {
            let mut row = String::new();
            for x in 0..grid.columns() {
                let cell = grid.get(x, y).unwrap();
                if cell.width != CellWidth::Continuation {
                    row.push_str(&cell.symbol);
                }
            }
            row.contains(title)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttyshot_core::{CellWidth, Style};
    fn grid(lines: &[&str]) -> Grid {
        let width = u16::try_from(lines.iter().map(|v| v.chars().count()).max().unwrap()).unwrap();
        let height = u16::try_from(lines.len()).unwrap();
        let mut grid = Grid::new(width, height).unwrap();
        for (y, line) in lines.iter().enumerate() {
            for (x, ch) in line.chars().enumerate() {
                grid.put(
                    u16::try_from(x).unwrap(),
                    u16::try_from(y).unwrap(),
                    ch.to_string(),
                    CellWidth::Single,
                    Style::default(),
                )
                .unwrap();
            }
        }
        grid
    }
    #[test]
    fn finds_every_supported_border() {
        for lines in [
            ["┌─ X ─┐", "│     │", "└─────┘"],
            ["╭─ X ─╮", "│     │", "╰─────╯"],
            ["┏━ X ━┓", "┃     ┃", "┗━━━━━┛"],
            ["╔═ X ═╗", "║     ║", "╚═════╝"],
            ["+- X -+", "|     |", "+-----+"],
        ] {
            assert_eq!(
                find_panel(&grid(&lines), "X").unwrap(),
                CellRect {
                    x: 0,
                    y: 0,
                    width: 7,
                    height: 3
                }
            );
        }
    }
    #[test]
    fn rejects_broken_side_and_bottom() {
        let broken = grid(&["┌─ X ─┐", "│      ", "└─────┘"]);
        let error = find_panel(&broken, "X").unwrap_err().to_string();
        assert!(error.contains("no complete supported border"));
    }
    #[test]
    fn ambiguity_lists_candidates() {
        let frame = grid(&["┌ X ┐  ┌ X ┐", "└───┘  └───┘"]);
        let error = find_panel(&frame, "X").unwrap_err().to_string();
        assert!(error.contains("ambiguous"));
        assert!(error.contains("0,0"));
        assert!(error.contains("7,0"));
    }
    #[test]
    fn cropped_wide_boundary_is_repaired() {
        let source = grid(&["┌ X ┐", "│界  │", "└───┘"]);
        let rect = find_panel(&source, "X").unwrap();
        let cropped = source.crop(rect).unwrap();
        assert_eq!(cropped.columns(), 5);
    }
}
