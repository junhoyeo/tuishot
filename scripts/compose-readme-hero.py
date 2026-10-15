#!/usr/bin/env python3
"""Compose the README collage and geometry comparison."""

from __future__ import annotations

import argparse
import gzip
import json
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont, ImageOps

LABEL_FONT = Path(__file__).parents[1] / "crates/ttyshot-render/assets/JetBrainsMono-Regular.ttf"


def gradient(size: tuple[int, int]) -> Image.Image:
    width, height = size
    image = Image.new("RGBA", size, (7, 10, 15, 255))
    draw = ImageDraw.Draw(image)
    for y in range(height):
        t = y / max(1, height - 1)
        draw.line((0, y, width, y), fill=(int(7 + 8 * t), int(10 + 10 * t), int(15 + 14 * t), 255))
    return image


def add_card(canvas: Image.Image, box: tuple[int, int, int, int], source: Path, label: str, accent: tuple[int, int, int], font: ImageFont.FreeTypeFont) -> None:
    x, y, width, height = box
    fitted = ImageOps.fit(Image.open(source).convert("RGB"), (width, height), method=Image.Resampling.LANCZOS).convert("RGBA")
    shadow = Image.new("RGBA", canvas.size)
    ImageDraw.Draw(shadow).rounded_rectangle((x + 4, y + 10, x + width + 4, y + height + 10), radius=18, fill=(0, 0, 0, 180))
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(15)))
    mask = Image.new("L", (width, height))
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, width - 1, height - 1), radius=16, fill=255)
    canvas.paste(fitted, (x, y), mask)
    draw = ImageDraw.Draw(canvas)
    draw.rounded_rectangle((x, y, x + width, y + height), radius=16, outline=(65, 78, 98), width=2)
    text_width = draw.textbbox((0, 0), label, font=font)[2]
    draw.rounded_rectangle((x + 18, y + 18, x + 18 + text_width + 54, y + 54), radius=18, fill=(8, 12, 18, 220), outline=(77, 91, 112))
    draw.ellipse((x + 31, y + 30, x + 41, y + 40), fill=accent)
    draw.text((x + 50, y + 24), label, font=font, fill=(235, 240, 247))


def compose_collage(args: argparse.Namespace) -> None:
    canvas = gradient((1800, 1160))
    draw = ImageDraw.Draw(canvas)
    brand = ImageFont.truetype(str(args.label_font), 26)
    promise = ImageFont.truetype(str(args.label_font), 35)
    subtitle = ImageFont.truetype(str(args.label_font), 17)
    badge = ImageFont.truetype(str(args.label_font), 18)
    draw.text((58, 30), "ttyshot", font=brand, fill=(34, 211, 155))
    draw.text((58, 76), "Screenshots of any TUI that look like a real terminal — and stay up to date.", font=promise, fill=(238, 241, 247))
    draw.text((60, 134), "SCRIPTED  ·  DETERMINISTIC  ·  CI-READY  ·  ONE CELL GRID", font=subtitle, fill=(132, 150, 172))
    add_card(canvas, (50, 180, 860, 607), args.tokscale, "TOKSCALE", (34, 211, 155), badge)
    add_card(canvas, (890, 180, 860, 607), args.lazygit, "LAZYGIT", (137, 173, 217), badge)
    add_card(canvas, (110, 705, 760, 433), args.ratatui, "RATATUI", (238, 121, 231), badge)
    add_card(canvas, (930, 745, 800, 337), args.yazi, "YAZI", (240, 196, 108), badge)
    args.hero_output.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(args.hero_output, optimize=True)


def rgb(color: dict, default: tuple[int, int, int]) -> tuple[int, int, int]:
    if color.get("kind") == "rgb":
        value = color["value"]
        return value["r"], value["g"], value["b"]
    return default


def load_grid(path: Path) -> dict:
    if path.suffix == ".gz":
        with gzip.open(path, "rt", encoding="utf-8") as source:
            return json.load(source)
    with path.open(encoding="utf-8") as source:
        return json.load(source)


def comparison_crops(args: argparse.Namespace) -> tuple[Image.Image, Image.Image]:
    frame = load_grid(args.grid_json)
    if (frame["columns"], frame["rows"]) != (120, 40):
        raise SystemExit("comparison grid must be 120x40")
    columns, rows = range(27, 39), range(5, 11)
    cell_width, cell_height = 10, 20
    glyph = Image.new("RGB", (len(columns) * cell_width, len(rows) * cell_height), (13, 17, 23))
    draw = ImageDraw.Draw(glyph)
    font = ImageFont.truetype(str(args.glyph_font), 15, index=args.glyph_font_index)
    for row_out, row in enumerate(rows):
        for column_out, column in enumerate(columns):
            cell = frame["cells"][row * 120 + column]
            style = cell["style"]
            background = rgb(style["background"], (13, 17, 23))
            foreground = rgb(style["foreground"], (216, 222, 233))
            x, y = column_out * cell_width, row_out * cell_height
            draw.rectangle((x, y, x + cell_width - 1, y + cell_height - 1), fill=background)
            if cell["width"] != "continuation" and cell["symbol"] not in ("", " "):
                draw.text((x + 1, y), cell["symbol"], font=font, fill=foreground)

    ttyshot = Image.open(args.tokscale).convert("RGB")
    if ttyshot.size != (2528, 1784):
        raise SystemExit(f"ttyshot comparison source must be 2528x1784, got {ttyshot.size}")
    left, top = 64 + 27 * 20, 120 + 5 * 40
    right, bottom = 64 + 39 * 20, 120 + 11 * 40
    geometry = ttyshot.crop((left, top, right, bottom)).resize(glyph.size, Image.Resampling.NEAREST)
    return glyph.resize((720, 720), Image.Resampling.NEAREST), geometry.resize((720, 720), Image.Resampling.NEAREST)


def compose_comparison(args: argparse.Namespace) -> None:
    glyph, geometry = comparison_crops(args)
    canvas = gradient((1600, 920))
    draw = ImageDraw.Draw(canvas)
    title = ImageFont.truetype(str(args.label_font), 30)
    label = ImageFont.truetype(str(args.label_font), 22)
    small = ImageFont.truetype(str(args.label_font), 16)
    draw.text((48, 24), "Menlo glyphs leave gaps here. ttyshot geometry stays seam-free.", font=title, fill=(235, 239, 246))
    draw.text((50, 70), "SYNTHETIC TOKSCALE · SAME 12×6 CELLS + COLORS · 6× NEAREST-NEIGHBOUR", font=small, fill=(132, 149, 170))

    def panel(x: int, heading: str, note: str, accent: tuple[int, int, int], image: Image.Image) -> None:
        shadow = Image.new("RGBA", canvas.size)
        ImageDraw.Draw(shadow).rounded_rectangle((x + 3, 112, x + 723, 890), radius=18, fill=(0, 0, 0, 165))
        canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(12)))
        d = ImageDraw.Draw(canvas)
        d.rounded_rectangle((x, 106, x + 720, 884), radius=18, fill=(14, 19, 27), outline=(53, 65, 82))
        d.ellipse((x + 18, 124, x + 31, 137), fill=accent)
        d.text((x + 44, 116), heading, font=label, fill=(236, 240, 247))
        width = d.textbbox((0, 0), note, font=small)[2]
        d.text((x + 720 - width - 18, 122), note, font=small, fill=(145, 159, 178))
        canvas.alpha_composite(image.convert("RGBA"), (x, 164))

    panel(48, "FONT-GLYPH EXPORT", "Menlo glyphs", (232, 133, 104), glyph)
    panel(832, "TTYSHOT GEOMETRY", "shared cell edges", (36, 203, 154), geometry)
    args.comparison_output.parent.mkdir(parents=True, exist_ok=True)
    canvas.convert("RGB").save(args.comparison_output, optimize=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tokscale", type=Path, required=True)
    parser.add_argument("--lazygit", type=Path, required=True)
    parser.add_argument("--ratatui", type=Path, required=True)
    parser.add_argument("--yazi", type=Path, required=True)
    parser.add_argument("--grid-json", type=Path, required=True, help="120x40 styled-grid JSON, optionally gzip-compressed")
    parser.add_argument("--glyph-font", type=Path, required=True)
    parser.add_argument("--glyph-font-index", type=int, default=0)
    parser.add_argument("--hero-output", type=Path, required=True)
    parser.add_argument("--comparison-output", type=Path, required=True)
    parser.add_argument("--label-font", type=Path, default=LABEL_FONT)
    args = parser.parse_args()
    compose_collage(args)
    compose_comparison(args)


if __name__ == "__main__":
    main()
