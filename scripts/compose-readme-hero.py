#!/usr/bin/env python3
"""Compose the public README hero from two synthetic Tokscale PNGs."""

from __future__ import annotations

import argparse
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont, ImageOps

W, H = 1600, 1500


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ttyshot", type=Path, required=True, help="2528x1784 ttyshot 2x PNG")
    parser.add_argument("--font-preview", type=Path, required=True, help="2640x1520 font preview PNG")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--font",
        type=Path,
        default=Path(__file__).parents[1] / "crates/ttyshot-render/assets/JetBrainsMono-Regular.ttf",
    )
    args = parser.parse_args()

    ttyshot = Image.open(args.ttyshot).convert("RGBA")
    preview = Image.open(args.font_preview).convert("RGBA")
    if ttyshot.size != (2528, 1784) or preview.size != (2640, 1520):
        raise SystemExit(f"unexpected source sizes: ttyshot={ttyshot.size}, preview={preview.size}")

    f14 = ImageFont.truetype(str(args.font), 18)
    f18 = ImageFont.truetype(str(args.font), 22)
    f24 = ImageFont.truetype(str(args.font), 28)
    fvs = ImageFont.truetype(str(args.font), 16)

    hero = Image.new("RGBA", (W, H), (8, 11, 16, 255))
    draw = ImageDraw.Draw(hero)
    for y in range(H):
        t = y / (H - 1)
        draw.line((0, y, W, y), fill=(int(8 + 8 * t), int(11 + 10 * t), int(16 + 15 * t), 255))

    draw.rounded_rectangle((64, 24, 625, 56), radius=16, fill=(22, 29, 39), outline=(52, 65, 82))
    draw.text((82, 32), "SYNTHETIC TOKSCALE FIXTURE  ·  120×40  ·  2×", font=f14, fill=(166, 183, 205))

    full_w = 1440
    full_h = round(ttyshot.height * full_w / ttyshot.width)
    full = ttyshot.resize((full_w, full_h), Image.Resampling.LANCZOS)
    fx, fy = 80, 72
    shadow = Image.new("RGBA", (W, H))
    ImageDraw.Draw(shadow).rounded_rectangle((fx + 3, fy + 8, fx + full_w + 3, fy + full_h + 8), radius=20, fill=(0, 0, 0, 180))
    hero.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(16)))
    mask = Image.new("L", (full_w, full_h))
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, full_w - 1, full_h - 1), radius=18, fill=255)
    hero.paste(full, (fx, fy), mask)
    draw = ImageDraw.Draw(hero)
    draw.rounded_rectangle((fx, fy, fx + full_w, fy + full_h), radius=18, outline=(55, 69, 88), width=2)

    heading_y = fy + full_h + 28
    draw.text((64, heading_y), "WHY CELL GEOMETRY MATTERS", font=f24, fill=(231, 236, 245))
    draw.text((575, heading_y + 5), "same synthetic frame · chart detail", font=f14, fill=(119, 137, 158))

    preview_crop = preview.crop((22 * 22, 4 * 38, 58 * 22, 11 * 38)).convert("RGB")
    ttyshot_crop = ttyshot.crop((64 + 22 * 20, 120 + 4 * 40, 64 + 58 * 20, 120 + 11 * 40)).convert("RGB")
    card_y, card_w, card_h = heading_y + 52, 720, 270

    def card(x: int, label: str, subtitle: str, accent: tuple[int, int, int], image: Image.Image) -> None:
        shadow = Image.new("RGBA", (W, H))
        ImageDraw.Draw(shadow).rounded_rectangle((x + 2, card_y + 5, x + card_w + 2, card_y + card_h + 5), radius=16, fill=(0, 0, 0, 150))
        hero.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(10)))
        d = ImageDraw.Draw(hero)
        d.rounded_rectangle((x, card_y, x + card_w, card_y + card_h), radius=16, fill=(15, 20, 28), outline=(48, 60, 77))
        d.ellipse((x + 18, card_y + 17, x + 29, card_y + 28), fill=accent)
        d.text((x + 40, card_y + 12), label, font=f18, fill=(235, 239, 246))
        text_width = d.textbbox((0, 0), subtitle, font=f14)[2]
        d.text((x + card_w - text_width - 18, card_y + 16), subtitle, font=f14, fill=(137, 151, 170))
        fitted = ImageOps.fit(image, (704, 210), method=Image.Resampling.LANCZOS).convert("RGBA")
        hero.alpha_composite(fitted, (x + 8, card_y + 50))
        d.rounded_rectangle((x + 8, card_y + 50, x + 712, card_y + 260), radius=8, outline=(48, 60, 77))

    card(60, "FONT PREVIEW", "glyph-rendered joins", (232, 133, 104), preview_crop)
    card(820, "TTYSHOT", "cell-aligned shapes", (36, 203, 154), ttyshot_crop)
    draw = ImageDraw.Draw(hero)
    draw.rounded_rectangle((775, card_y + 116, 825, card_y + 166), radius=25, fill=(26, 34, 46), outline=(74, 89, 110))
    draw.text((790, card_y + 130), "VS", font=fvs, fill=(184, 196, 212))

    args.output.parent.mkdir(parents=True, exist_ok=True)
    hero.convert("RGB").save(args.output, optimize=True)


if __name__ == "__main__":
    main()
