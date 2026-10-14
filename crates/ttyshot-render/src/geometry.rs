#![allow(clippy::items_after_test_module)]

//! Integer procedural glyph geometry.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shape {
    Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        width: i32,
    },
    Polygon(Vec<(i32, i32)>),
}

pub(crate) fn shapes(character: char, width: i32, height: i32) -> Option<Vec<Shape>> {
    match character as u32 {
        0x2500..=0x257f => Some(box_shapes(character, width, height)),
        0x2580..=0x259f => Some(block_shapes(character, width, height)),
        0x2800..=0x28ff => Some(braille_shapes(character, width, height)),
        0x1fb00..=0x1fb3b => Some(grid_shapes(
            SEXTANT_MASKS[(character as usize) - 0x1fb00],
            2,
            3,
            width,
            height,
        )),
        0x1cd00..=0x1cde5 => Some(grid_shapes(
            OCTANT_MASKS[(character as usize) - 0x1cd00],
            2,
            4,
            width,
            height,
        )),
        0xe0b0..=0xe0b7 => Some(powerline_shapes(character, width, height)),
        _ => None,
    }
}

fn box_shapes(character: char, width: i32, height: i32) -> Vec<Shape> {
    let code = character as usize;
    if (0x256d..=0x2570).contains(&code) {
        return arc_shapes(code, width, height);
    }
    if (0x2571..=0x2573).contains(&code) {
        let stroke = (width / 8).max(1);
        return match code {
            0x2571 => vec![Shape::Line {
                x1: width - 1,
                y1: 0,
                x2: 0,
                y2: height - 1,
                width: stroke,
            }],
            0x2572 => vec![Shape::Line {
                x1: 0,
                y1: 0,
                x2: width - 1,
                y2: height - 1,
                width: stroke,
            }],
            _ => vec![
                Shape::Line {
                    x1: width - 1,
                    y1: 0,
                    x2: 0,
                    y2: height - 1,
                    width: stroke,
                },
                Shape::Line {
                    x1: 0,
                    y1: 0,
                    x2: width - 1,
                    y2: height - 1,
                    width: stroke,
                },
            ],
        };
    }
    if matches!(code, 0x2504..=0x250b | 0x254c..=0x254f) {
        let count = match code {
            0x2504..=0x2507 => 3,
            0x2508..=0x250b => 4,
            _ => 2,
        };
        let vertical = matches!(code, 0x2506 | 0x2507 | 0x250a | 0x250b | 0x254e | 0x254f);
        let heavy = code % 2 == 1;
        return dashed_line(width, height, vertical, count, heavy);
    }
    let arms = BOX_ARMS[code - 0x2500];
    let mut result = Vec::new();
    for (direction, weight) in arms.into_iter().enumerate() {
        if weight == 0 {
            continue;
        }
        add_arm(&mut result, direction, weight, width, height);
    }
    result
}

fn arc_shapes(code: usize, width: i32, height: i32) -> Vec<Shape> {
    let stroke = (width / 8).max(1);
    let cx = width / 2;
    let cy = height / 2;
    let quarter_x = width / 4;
    let quarter_y = height / 4;
    let (vertical_edge, horizontal_edge, vertical_turn, horizontal_turn) = match code {
        0x256d => (
            (cx, height),
            (width, cy),
            (cx, cy + quarter_y),
            (cx + quarter_x, cy),
        ),
        0x256e => (
            (cx, height),
            (0, cy),
            (cx, cy + quarter_y),
            (cx - quarter_x, cy),
        ),
        0x256f => ((cx, 0), (0, cy), (cx, cy - quarter_y), (cx - quarter_x, cy)),
        _ => (
            (cx, 0),
            (width, cy),
            (cx, cy - quarter_y),
            (cx + quarter_x, cy),
        ),
    };
    vec![
        Shape::Line {
            x1: vertical_edge.0,
            y1: vertical_edge.1,
            x2: vertical_turn.0,
            y2: vertical_turn.1,
            width: stroke,
        },
        Shape::Line {
            x1: vertical_turn.0,
            y1: vertical_turn.1,
            x2: horizontal_turn.0,
            y2: horizontal_turn.1,
            width: stroke,
        },
        Shape::Line {
            x1: horizontal_turn.0,
            y1: horizontal_turn.1,
            x2: horizontal_edge.0,
            y2: horizontal_edge.1,
            width: stroke,
        },
    ]
}

fn dashed_line(width: i32, height: i32, vertical: bool, count: i32, heavy: bool) -> Vec<Shape> {
    let length = if vertical { height } else { width };
    let unit = (length / (count * 2 - 1)).max(1);
    let thickness = if heavy {
        (width / 4).max(2)
    } else {
        (width / 8).max(1)
    };
    let mut result = Vec::new();
    for index in 0..count {
        let start = index * unit * 2;
        let segment = if index + 1 == count {
            length - start
        } else {
            unit
        };
        if vertical {
            result.push(rect(width / 2 - thickness / 2, start, thickness, segment));
        } else {
            result.push(rect(start, height / 2 - thickness / 2, segment, thickness));
        }
    }
    result
}

fn add_arm(out: &mut Vec<Shape>, direction: usize, weight: u8, width: i32, height: i32) {
    let cx = width / 2;
    let cy = height / 2;
    let thickness = match weight {
        2 => (width / 4).max(2),
        _ => (width / 8).max(1),
    };
    if weight == 3 {
        let gap = (width / 5).max(1);
        match direction {
            0 => {
                out.push(rect(cx - gap, 0, thickness, cy));
                out.push(rect(cx + gap - thickness, 0, thickness, cy));
            }
            1 => {
                out.push(rect(cx, cy - gap, width - cx, thickness));
                out.push(rect(cx, cy + gap - thickness, width - cx, thickness));
            }
            2 => {
                out.push(rect(cx - gap, cy, thickness, height - cy));
                out.push(rect(cx + gap - thickness, cy, thickness, height - cy));
            }
            _ => {
                out.push(rect(0, cy - gap, cx + 1, thickness));
                out.push(rect(0, cy + gap - thickness, cx + 1, thickness));
            }
        }
        return;
    }
    let (x, y, w, h) = match direction {
        0 => (cx - thickness / 2, 0, thickness, cy + 1),
        1 => (cx, cy - thickness / 2, width - cx, thickness),
        2 => (cx - thickness / 2, cy, thickness, height - cy),
        _ => (0, cy - thickness / 2, cx + 1, thickness),
    };
    out.push(rect(x, y, w, h));
}

fn block_shapes(character: char, width: i32, height: i32) -> Vec<Shape> {
    let code = character as u32;
    match code {
        0x2580 => vec![rect(0, 0, width, ceil_fraction(height, 4, 8))],
        0x2581..=0x2588 => {
            let n = i32::try_from(code - 0x2580).unwrap_or(8);
            let y = height - ceil_fraction(height, n, 8);
            vec![rect(0, y, width, height - y)]
        }
        0x2589..=0x258f => {
            let n = i32::try_from(0x2590 - code).unwrap_or(1);
            vec![rect(0, 0, ceil_fraction(width, n, 8), height)]
        }
        0x2590 => vec![rect(width / 2, 0, width - width / 2, height)],
        0x2591..=0x2593 => shade_shapes(i32::try_from(code - 0x2590).unwrap_or(1), width, height),
        0x2594 => vec![rect(0, 0, width, ceil_fraction(height, 1, 8))],
        0x2595 => {
            let x = width - ceil_fraction(width, 1, 8);
            vec![rect(x, 0, width - x, height)]
        }
        0x2596..=0x259f => quadrant_shapes(code, width, height),
        _ => Vec::new(),
    }
}

fn quadrant_shapes(code: u32, width: i32, height: i32) -> Vec<Shape> {
    let mask = match code {
        0x2596 => 0b0100,
        0x2597 => 0b1000,
        0x2598 => 0b0001,
        0x2599 => 0b1101,
        0x259a => 0b1001,
        0x259b => 0b0111,
        0x259c => 0b1011,
        0x259d => 0b0010,
        0x259e => 0b0110,
        0x259f => 0b1110,
        _ => 0,
    };
    grid_shapes(mask, 2, 2, width, height)
}

fn shade_shapes(level: i32, width: i32, height: i32) -> Vec<Shape> {
    let denominator = 4;
    let mut result = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if (x + y * 3).rem_euclid(denominator) < level {
                result.push(rect(x, y, 1, 1));
            }
        }
    }
    result
}

fn braille_shapes(character: char, width: i32, height: i32) -> Vec<Shape> {
    let bits = character as u32 - 0x2800;
    let dot_map = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let dot_width = (width / 4).max(1);
    let dot_height = (height / 8).max(1);
    let mut result = Vec::new();
    for (bit, (column, row)) in dot_map.into_iter().enumerate() {
        if bits & (1 << bit) != 0 {
            let x = column * (width / 2) + width / 4 - dot_width / 2;
            let y = (row * height + height / 2) / 4 - dot_height / 2;
            result.push(rect(x, y, dot_width, dot_height));
        }
    }
    result
}

fn grid_shapes(mask: u8, columns: i32, rows: i32, width: i32, height: i32) -> Vec<Shape> {
    let mut result = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let bit = row * columns + column;
            if mask & (1 << bit) != 0 {
                let x0 = column * width / columns;
                let x1 = (column + 1) * width / columns;
                let y0 = row * height / rows;
                let y1 = (row + 1) * height / rows;
                result.push(rect(x0, y0, x1 - x0, y1 - y0));
            }
        }
    }
    result
}

fn powerline_shapes(character: char, width: i32, height: i32) -> Vec<Shape> {
    let middle = height / 2;
    let stroke = (width / 8).max(1);
    match character as u32 {
        // Hard separators are filled arrowheads.
        0xe0b0 => vec![Shape::Polygon(vec![(0, 0), (width, middle), (0, height)])],
        0xe0b2 => vec![Shape::Polygon(vec![
            (width, 0),
            (0, middle),
            (width, height),
        ])],
        // Soft separators are open chevrons.
        0xe0b1 => polyline(&[(0, 0), (width - 1, middle), (0, height - 1)], stroke),
        0xe0b3 => polyline(
            &[(width - 1, 0), (0, middle), (width - 1, height - 1)],
            stroke,
        ),
        // The Powerline extra symbols are rounded half circles, not aliases of
        // the hard triangle separators. Thick variants are filled; thin
        // variants trace the same curve with a scale-safe stroke.
        0xe0b4 => vec![Shape::Polygon(right_half_circle(width, height))],
        0xe0b5 => polyline(&right_half_circle(width - 1, height), stroke),
        0xe0b6 => vec![Shape::Polygon(left_half_circle(width, height))],
        0xe0b7 => polyline(&left_half_circle(width - 1, height), stroke),
        _ => unreachable!("powerline range is checked by shapes"),
    }
}

fn polyline(points: &[(i32, i32)], width: i32) -> Vec<Shape> {
    points
        .windows(2)
        .map(|pair| Shape::Line {
            x1: pair[0].0,
            y1: pair[0].1,
            x2: pair[1].0,
            y2: pair[1].1,
            width,
        })
        .collect()
}

fn right_half_circle(width: i32, height: i32) -> Vec<(i32, i32)> {
    vec![
        (0, 0),
        (width / 2, height / 16),
        (3 * width / 4, height / 4),
        (width, height / 2),
        (3 * width / 4, 3 * height / 4),
        (width / 2, 15 * height / 16),
        (0, height),
    ]
}

fn left_half_circle(width: i32, height: i32) -> Vec<(i32, i32)> {
    right_half_circle(width, height)
        .into_iter()
        .map(|(x, y)| (width - x, y))
        .collect()
}

const fn ceil_fraction(value: i32, numerator: i32, denominator: i32) -> i32 {
    (value * numerator + denominator - 1) / denominator
}
const fn rect(x: i32, y: i32, width: i32, height: i32) -> Shape {
    Shape::Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_triple_and_quadruple_dash_have_exact_segment_counts() {
        assert_eq!(shapes('╌', 16, 24).unwrap().len(), 2);
        assert_eq!(shapes('┄', 16, 24).unwrap().len(), 3);
        assert_eq!(shapes('┈', 16, 24).unwrap().len(), 4);
        assert_eq!(shapes('╎', 16, 24).unwrap().len(), 2);
        assert_eq!(shapes('┆', 16, 24).unwrap().len(), 3);
        assert_eq!(shapes('┊', 16, 24).unwrap().len(), 4);
    }

    #[test]
    fn every_procedural_codepoint_produces_geometry() {
        let ranges = [
            (0x2500, 0x257f),
            (0x2580, 0x259f),
            (0x2800, 0x28ff),
            (0x1fb00, 0x1fb3b),
            (0x1cd00, 0x1cde5),
            (0xe0b0, 0xe0b7),
        ];
        for (start, end) in ranges {
            for code in start..=end {
                let character = char::from_u32(code).unwrap();
                let result = shapes(character, 16, 24);
                assert!(result.is_some(), "U+{code:04X} was not procedural");
            }
        }
    }
}

const BOX_ARMS: [[u8; 4]; 128] = [
    [0, 1, 0, 1],
    [0, 2, 0, 2],
    [1, 0, 1, 0],
    [2, 0, 2, 0],
    [0, 1, 0, 1],
    [0, 2, 0, 2],
    [1, 0, 1, 0],
    [2, 0, 2, 0],
    [0, 1, 0, 1],
    [0, 2, 0, 2],
    [1, 0, 1, 0],
    [2, 0, 2, 0],
    [0, 1, 1, 0],
    [0, 2, 1, 0],
    [0, 1, 2, 0],
    [0, 2, 2, 0],
    [0, 0, 1, 1],
    [0, 0, 1, 2],
    [0, 0, 2, 1],
    [0, 0, 2, 2],
    [1, 1, 0, 0],
    [1, 2, 0, 0],
    [2, 1, 0, 0],
    [2, 2, 0, 0],
    [1, 0, 0, 1],
    [1, 0, 0, 2],
    [2, 0, 0, 1],
    [2, 0, 0, 2],
    [1, 1, 1, 0],
    [1, 2, 1, 0],
    [2, 1, 1, 0],
    [1, 1, 2, 0],
    [2, 1, 2, 0],
    [2, 2, 1, 0],
    [1, 2, 2, 0],
    [2, 2, 2, 0],
    [1, 0, 1, 1],
    [1, 0, 1, 2],
    [2, 0, 1, 1],
    [1, 0, 2, 1],
    [2, 0, 2, 1],
    [2, 0, 1, 2],
    [1, 0, 2, 2],
    [2, 0, 2, 2],
    [0, 1, 1, 1],
    [0, 1, 1, 2],
    [0, 2, 1, 1],
    [0, 2, 1, 2],
    [0, 1, 2, 1],
    [0, 1, 2, 2],
    [0, 2, 2, 1],
    [0, 2, 2, 2],
    [1, 1, 0, 1],
    [1, 1, 0, 2],
    [1, 2, 0, 1],
    [1, 2, 0, 2],
    [2, 1, 0, 1],
    [2, 1, 0, 2],
    [2, 2, 0, 1],
    [2, 2, 0, 2],
    [1, 1, 1, 1],
    [1, 1, 1, 2],
    [1, 2, 1, 1],
    [1, 2, 1, 2],
    [2, 1, 1, 1],
    [1, 1, 2, 1],
    [2, 1, 2, 1],
    [2, 1, 1, 2],
    [2, 2, 1, 1],
    [1, 1, 2, 2],
    [1, 2, 2, 1],
    [2, 2, 1, 2],
    [1, 2, 2, 2],
    [2, 1, 2, 2],
    [2, 2, 2, 1],
    [2, 2, 2, 2],
    [0, 3, 0, 3],
    [0, 3, 0, 3],
    [3, 0, 3, 0],
    [3, 0, 3, 0],
    [0, 3, 0, 3],
    [3, 0, 3, 0],
    [0, 3, 1, 0],
    [0, 1, 3, 0],
    [0, 3, 3, 0],
    [0, 0, 1, 3],
    [0, 0, 3, 1],
    [0, 0, 3, 3],
    [1, 3, 0, 0],
    [3, 1, 0, 0],
    [3, 3, 0, 0],
    [1, 0, 0, 3],
    [3, 0, 0, 1],
    [3, 0, 0, 3],
    [1, 3, 1, 0],
    [3, 1, 3, 0],
    [3, 3, 3, 0],
    [1, 0, 1, 3],
    [3, 0, 3, 1],
    [3, 0, 3, 3],
    [0, 3, 1, 3],
    [0, 1, 3, 1],
    [0, 3, 3, 3],
    [1, 3, 0, 3],
    [3, 1, 0, 1],
    [3, 3, 0, 3],
    [1, 3, 1, 3],
    [3, 1, 3, 1],
    [3, 3, 3, 3],
    [0, 1, 1, 0],
    [0, 0, 1, 1],
    [1, 0, 0, 1],
    [1, 1, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 1],
    [1, 0, 0, 0],
    [0, 1, 0, 0],
    [0, 0, 1, 0],
    [0, 0, 0, 2],
    [2, 0, 0, 0],
    [0, 2, 0, 0],
    [0, 0, 2, 0],
    [0, 2, 0, 1],
    [1, 0, 2, 0],
    [0, 1, 0, 2],
    [2, 0, 1, 0],
];
const SEXTANT_MASKS: [u8; 60] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 23, 24, 25, 26, 27,
    28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
    53, 54, 55, 56, 57, 58, 59, 60, 61, 62,
];
const OCTANT_MASKS: [u8; 230] = [
    4, 6, 7, 8, 9, 11, 12, 13, 14, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 36, 37, 38, 39, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
    58, 59, 60, 61, 62, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 81, 82, 83, 84,
    86, 87, 88, 89, 91, 92, 93, 94, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108,
    109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
    129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147,
    148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 161, 162, 163, 164, 166, 167, 168,
    169, 171, 172, 173, 174, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189,
    190, 191, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209,
    210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228,
    229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 241, 242, 243, 244, 246, 247, 248, 249,
    251, 253, 254,
];
