# README image provenance

Every image in this directory uses synthetic or neutral content. No account, host, process list, personal home path, or production data appears in the assets.

## Hero

`hero/ttyshot-hero.png` combines:

- the full 120×40, 2× ttyshot PNG from the synthetic Tokscale 4.19 fixture; and
- a chart detail from the same fixture's pyte/Pillow font preview beside the ttyshot geometry render.

The labels say **FONT PREVIEW · glyph-rendered joins** and **TTYSHOT · cell-aligned shapes**. The preview is intentionally described as a font preview, not as a competing terminal or a pixel-equivalent reference. The comparison illustrates the visible hairline joins in the font-rendered stacked blocks.

The composition can be regenerated after producing the two source PNGs:

```sh
python3 scripts/compose-readme-hero.py \
  --ttyshot synthetic/ttyshot-2x.png \
  --font-preview synthetic/font-preview.png \
  --output docs/assets/hero/ttyshot-hero.png
```

## Gallery

| File | Program | Version/content | Isolation |
| --- | --- | --- | --- |
| `gallery/tokscale.png` | Tokscale | 4.19.0; fixed 75-day synthetic fixture | cleared environment, isolated HOME/config, offline pricing |
| `gallery/lazygit.png` | LazyGit | 0.66.1; throwaway repository with fixed demo commits | no remote, generic author, isolated HOME |
| `gallery/yazi.png` | Yazi | 26.9.1; generated demo directory | isolated HOME/config, icons disabled, path header cropped by cells |
| `gallery/vim.png` | Vim | 9.1; read-only generated text fixture | `-u NONE -n -R`, isolated HOME and working directory |

All captures use a real 120×40 PTY, ttyshot scale 2, macOS-style chrome, a shadow, state-based waits, and no fixed sleeps. Images were inspected as a full frame and as four near-1:1 quadrants before commit.

## SHA-256

- `2f57ef901ea91dad641de1abedc7a3a48ded0c399d29c0bb334c40f091cb30c4`  `docs/assets/gallery/lazygit.png`
- `615b7c2a566836d083cbb075bd5609b304709cd9dd6f5e7c7ee33db2d4d390db`  `docs/assets/gallery/tokscale.png`
- `3cd5e3aca6469bb21c270114d41f6b7a081a9296982c45309254b0f3b0f4773c`  `docs/assets/gallery/vim.png`
- `4bd3fd7062a191fc1677752c3316903ce8905e58c3636544fbfa78d10711e719`  `docs/assets/gallery/yazi.png`
- `c7c06731964fb97897a42cb0a51b3bbdcd1e960821f0de492dcf5cd27528cacc`  `docs/assets/hero/ttyshot-hero.png`
