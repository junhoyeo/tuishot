# README image provenance

The gallery uses generated or application-provided demo content. No account, host, process list, personal home path, or production data appears in the assets.

## Hero collage

`hero/ttyshot-hero.png` combines four real ttyshot captures: Tokscale, LazyGit, the official Ratatui demo2 application, and Yazi. The one-line product promise is part of the composition so it remains legible at README display width.

## Geometry comparison

`hero/geometry-comparison.png` uses columns 27–38 and rows 5–10 from a tracked 120×40 Tokscale styled grid. Both panels use the same 12×6 cell symbols and RGB colors:

- **FONT-GLYPH EXPORT** draws each block character with Menlo collection index 0 at 15 px, offset one pixel inside a 10×20 cell. It is a representative font-based export path.
- **TTYSHOT GEOMETRY** uses the corresponding cells from the checked-in ttyshot PNG.

Both sides are enlarged 6× with nearest-neighbour sampling. The headline is scoped to this Menlo fixture. It does not claim that the preview is a terminal, that every font gaps, or that the two renderers are pixel-equivalent. The committed image used Menlo from macOS 26.2 (`/System/Library/Fonts/Menlo.ttc`, SHA-256 `dc256e0b39c2a6fec947129d421fef41b8b429f58f9b6e5d1b148c87f775c1f6`).

The complete styled-grid input is tracked as `source/tokscale-4.19-grid.json.gz`. It is gzip-compressed JSON, not a custom format, and contains the generated screen data shown in the Tokscale gallery image.

## Reproduce the composed assets

Run this command from the repository root on macOS with the Menlo file identified above. CPython 3.14.0 and Pillow 12.3.0 are pinned so the PNG encoder and text rasterizer are explicit inputs.

```sh
uv run --python 3.14.0 --with-requirements scripts/requirements-readme-assets.txt \
  python scripts/compose-readme-hero.py \
  --tokscale docs/assets/gallery/tokscale.png \
  --lazygit docs/assets/gallery/lazygit.png \
  --ratatui docs/assets/gallery/ratatui.png \
  --yazi docs/assets/gallery/yazi.png \
  --grid-json docs/assets/source/tokscale-4.19-grid.json.gz \
  --glyph-font /System/Library/Fonts/Menlo.ttc \
  --hero-output docs/assets/hero/ttyshot-hero.png \
  --comparison-output docs/assets/hero/geometry-comparison.png
```

The script uses the repository's bundled JetBrains Mono file for composition labels (SHA-256 `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f`). The hashes below are the expected output and tracked-input hashes.

## Gallery

| File | Program | Version/content | Isolation |
| --- | --- | --- | --- |
| `gallery/tokscale.png` | Tokscale | 4.19.0; fixed 75-day generated dataset | cleared environment, isolated HOME/config, offline pricing |
| `gallery/lazygit.png` | LazyGit | 0.66.1; throwaway Orbit Notes repository | no remote, generic author, isolated HOME |
| `gallery/ratatui.png` | [Ratatui demo2](https://github.com/ratatui/ratatui/tree/d1ef782b48ddf0cb825c111d4d8b9adef572c0e1/examples/apps/demo2) | upstream `d1ef782b48ddf0cb825c111d4d8b9adef572c0e1`; built-in data | official showcase source, cleared environment |
| `gallery/helix.png` | Helix | 25.07.1; generated Rust dashboard module, Catppuccin Mocha | isolated HOME/config and working directory |
| `gallery/yazi.png` | Yazi | 26.9.1; generated Aurora UI Kit directory | isolated HOME/config, icons disabled, path header cropped by cells |

Captures use a real PTY, ttyshot scale 2, window chrome, a shadow, state-based waits, and no fixed sleeps. Each committed image was inspected at approximately 900 px README width, as a full frame, and as near-1:1 quadrants. README thumbnails link to the full-resolution files.

## SHA-256

- `c5d787380fcf3049b34fca846aef4ff862fbf62eb958a0045c2799dbc3612e58`  `docs/assets/gallery/helix.png`
- `7efcf54d239f23b1727b9a21bfb3c05eb9d63689411a05f9e806bf05cf2f9d0d`  `docs/assets/gallery/lazygit.png`
- `8ae086335dfdf8d53235af5a5e22dac45ac8863f9e1c2cce616a0d21cf0d87dc`  `docs/assets/gallery/ratatui.png`
- `615b7c2a566836d083cbb075bd5609b304709cd9dd6f5e7c7ee33db2d4d390db`  `docs/assets/gallery/tokscale.png`
- `69ce0710476fea66a605a824cd8c388fff78d067acd023cbf2305c3d464e2aa9`  `docs/assets/gallery/yazi.png`
- `7f5fcae96b4dcf60a709cc82db56db29179bcb4740d7320fc68228ccc9ad7db7`  `docs/assets/hero/geometry-comparison.png`
- `65c0dfe04a94085ab636c971e0a77c21fdfd14058de6536d17ebe0bb9c714c3d`  `docs/assets/hero/ttyshot-hero.png`
- `af51b7be1dd8f95e5968fa0369d16d64c098fcc8d98d26642b5f562ec7097b45`  `docs/assets/source/tokscale-4.19-grid.json.gz`
