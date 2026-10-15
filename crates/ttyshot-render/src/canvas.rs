#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_arguments
)]
// Coordinates are clipped before conversion and alpha arithmetic is bounded to 0..=255.

use crate::{RenderError, geometry::Shape};
use ttyshot_core::Rgb;

pub(crate) struct Canvas {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

impl Canvas {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        let width_usize = usize::try_from(width).map_err(|_| RenderError::DimensionOverflow)?;
        let height_usize = usize::try_from(height).map_err(|_| RenderError::DimensionOverflow)?;
        let byte_len = width_usize
            .checked_mul(height_usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(RenderError::DimensionOverflow)?;
        let pixels = allocate_pixels(byte_len, width, height)?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub(crate) fn fill_rect(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        color: Rgb,
        alpha: u8,
    ) {
        let left = x.max(0).min(self.width as i32);
        let top = y.max(0).min(self.height as i32);
        let right = (x + width).max(0).min(self.width as i32);
        let bottom = (y + height).max(0).min(self.height as i32);
        for py in top..bottom {
            for px in left..right {
                self.blend(px, py, color, alpha);
            }
        }
    }

    pub(crate) fn shape(
        &mut self,
        shape: &Shape,
        origin_x: i32,
        origin_y: i32,
        color: Rgb,
        alpha: u8,
    ) {
        match shape {
            Shape::Rect {
                x,
                y,
                width,
                height,
            } => self.fill_rect(origin_x + x, origin_y + y, *width, *height, color, alpha),
            Shape::Line {
                x1,
                y1,
                x2,
                y2,
                width,
            } => self.line(
                origin_x + x1,
                origin_y + y1,
                origin_x + x2,
                origin_y + y2,
                *width,
                color,
                alpha,
            ),
            Shape::Polygon(points) => {
                let translated: Vec<_> = points
                    .iter()
                    .map(|(x, y)| (origin_x + x, origin_y + y))
                    .collect();
                self.polygon(&translated, color, alpha);
            }
        }
    }

    pub(crate) fn ellipse_outline(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        stroke: i32,
        color: Rgb,
        alpha: u8,
    ) {
        let outer_width = i64::from(width.max(1));
        let outer_height = i64::from(height.max(1));
        let inner_width = i64::from((width - 2 * stroke).max(0));
        let inner_height = i64::from((height - 2 * stroke).max(0));
        for py in 0..height {
            for px in 0..width {
                let dx = i64::from(2 * px + 1 - width);
                let dy = i64::from(2 * py + 1 - height);
                let outer = dx * dx * outer_height * outer_height
                    + dy * dy * outer_width * outer_width
                    <= outer_width * outer_width * outer_height * outer_height;
                let inner = inner_width > 0
                    && inner_height > 0
                    && dx * dx * inner_height * inner_height + dy * dy * inner_width * inner_width
                        <= inner_width * inner_width * inner_height * inner_height;
                if outer && !inner {
                    self.blend(x + px, y + py, color, alpha);
                }
            }
        }
    }

    pub(crate) fn mask(
        &mut self,
        x: i32,
        y: i32,
        width: usize,
        height: usize,
        mask: &[u8],
        color: Rgb,
        alpha: u8,
        clip: (i32, i32, i32, i32),
        bold_offset: i32,
        italic_skew: i32,
    ) {
        for row in 0..height {
            let skew = ((height - row) as i32 * italic_skew) / height.max(1) as i32;
            for column in 0..width {
                let coverage = mask[row * width + column];
                if coverage == 0 {
                    continue;
                }
                let px = x + column as i32 + skew;
                let py = y + row as i32;
                if px >= clip.0 && py >= clip.1 && px < clip.2 && py < clip.3 {
                    self.blend(px, py, color, multiply_alpha(coverage, alpha));
                    for offset in 1..=bold_offset {
                        if px + offset < clip.2 {
                            self.blend(px + offset, py, color, multiply_alpha(coverage, alpha));
                        }
                    }
                }
            }
        }
    }

    fn line(
        &mut self,
        mut x0: i32,
        mut y0: i32,
        x1: i32,
        y1: i32,
        width: i32,
        color: Rgb,
        alpha: u8,
    ) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.fill_rect(
                x0 - width / 2,
                y0 - width / 2,
                width.max(1),
                width.max(1),
                color,
                alpha,
            );
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    fn polygon(&mut self, points: &[(i32, i32)], color: Rgb, alpha: u8) {
        if points.len() < 3 {
            return;
        }
        let min_y = points.iter().map(|point| point.1).min().unwrap_or(0).max(0);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .max()
            .unwrap_or(0)
            .min(self.height as i32 - 1);
        for y in min_y..=max_y {
            let mut intersections = Vec::new();
            for index in 0..points.len() {
                let (x1, y1) = points[index];
                let (x2, y2) = points[(index + 1) % points.len()];
                if (y1 <= y && y2 > y) || (y2 <= y && y1 > y) {
                    intersections.push(x1 + (y - y1) * (x2 - x1) / (y2 - y1));
                }
            }
            intersections.sort_unstable();
            for index in (0..intersections.len().saturating_sub(1)).step_by(2) {
                self.fill_rect(
                    intersections[index],
                    y,
                    intersections[index + 1] - intersections[index] + 1,
                    1,
                    color,
                    alpha,
                );
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, color: Rgb, alpha: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let index = (y as usize * self.width as usize + x as usize) * 4;
        let destination_alpha = u32::from(self.pixels[index + 3]);
        let source_alpha = u32::from(alpha);
        let output_alpha = source_alpha + (destination_alpha * (255 - source_alpha) + 127) / 255;
        if output_alpha == 0 {
            return;
        }
        for (offset, source) in [color.r, color.g, color.b].into_iter().enumerate() {
            let destination = u32::from(self.pixels[index + offset]);
            let premultiplied = u32::from(source) * source_alpha
                + (destination * destination_alpha * (255 - source_alpha) + 127) / 255;
            self.pixels[index + offset] = ((premultiplied + output_alpha / 2) / output_alpha) as u8;
        }
        self.pixels[index + 3] = output_alpha as u8;
    }
}

fn allocate_pixels(byte_len: usize, width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(byte_len)
        .map_err(|_| RenderError::AllocationFailed { width, height })?;
    pixels.resize(byte_len, 0);
    Ok(pixels)
}

const fn multiply_alpha(first: u8, second: u8) -> u8 {
    ((first as u16 * second as u16 + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_length_overflow_is_reported_without_panicking() {
        assert!(matches!(
            Canvas::new(u32::MAX, u32::MAX),
            Err(RenderError::DimensionOverflow)
        ));
        assert!(matches!(
            allocate_pixels(usize::MAX, 1, 1),
            Err(RenderError::AllocationFailed {
                width: 1,
                height: 1
            })
        ));
    }
}
