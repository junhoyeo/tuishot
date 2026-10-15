# Launch gallery and hero plan

All launch images must come from reproducible, synthetic fixtures. Never capture a developer's real home directory, usage totals, account names, repositories, clusters, tokens, paths, or model history.

## Hero comparison

Use one 80x20 synthetic Ratatui fixture with:

- a double-line bordered panel and one rounded panel
- adjacent full and fractional blocks
- Braille and sextant plots
- bold, italic, underlined, dim, reverse, 16-color, 256-color, and truecolor cells
- one pinned-font CJK wide grapheme
- a Powerline status segment
- content in the final row and final column

Render the exact same authoritative cell JSON through ttyshot and each compatible exporter. Show one full image and three 4x nearest-neighbor crops: a block seam, a border join, and the last row/column. Label unsupported inputs as unsupported instead of recreating a visually similar frame by hand.

Planned repository paths:

```text
docs/assets/hero/fixture.json
docs/assets/hero/ttyshot.png
docs/assets/hero/ttyshot.svg
docs/assets/hero/vhs.png
docs/assets/hero/termframe.svg
docs/assets/hero/termscope.svg
docs/assets/hero/toque.svg
docs/assets/hero/ratasvg.svg
docs/assets/hero/crops/*.png
```

Do not commit an exporter output until its generating command, version, and license are recorded in `docs/assets/hero/README.md`.

## Gallery matrix

| App | Fixture rule | Stable state | Planned assets |
| --- | --- | --- | --- |
| btop | container or temporary HOME; fixed process fixture if supported | overview after screen stability | dark PNG/SVG, 120x40 |
| lazygit | temporary Git repository with generated commits and fixed author names | files/status panel | dark PNG/SVG, 120x40 |
| yazi | generated directory tree with small text/image placeholders | file list with preview | dark PNG/SVG, 120x40 |
| gitui | the same generated Git repository as lazygit | status view | light and dark PNG, 120x40 |
| k9s | local fake/test Kubernetes API only; never a real context | pods table | dark PNG/SVG, 140x45 |
| Tokscale | the committed 75-day synthetic multi-client fixture, offline pricing cache, cleared environment, UTC and fixed date range | populated Overview plus Models panel | current window chrome, 120x40 |

If an app cannot run without real credentials or a live service, replace it with an explicit fixture mode or defer the image. Never point a gallery capture at a real account.

## Capture protocol

1. Pin the application version and record `--version` output.
2. Regenerate the fixture in a new temporary directory.
3. Clear the child environment. Add only `PATH`, `HOME`, `TERM=xterm-256color`, `COLORTERM=truecolor`, locale, timezone, and fixture-specific variables.
4. Use `wait_text`, `wait_regex`, or `wait_stable`; never a fixed delay.
5. Capture PNG, SVG, text, and JSON from the same frame.
6. Repeat the run and compare PNG bytes.
7. Render at 1x and 2x. Scan block joins for background pixels.
8. Inspect every PNG directly. Check the first and last rows/columns, wide text, cursor, title bar, shadows, and crop boundaries.
9. Run the same scene on macOS, Linux arm64, and Linux amd64 before selecting a golden.
10. Commit the scene, fixture generator, version record, and chosen assets together.

## Tokscale command contract

The Tokscale gallery must use the synthetic fixture generator maintained with the integration work. The child command is equivalent to:

```sh
npx --yes tokscale@4.18.0 --no-spinner   --since 2026-08-02 --until 2026-10-15   -c claude,codex,gemini,opencode
```

The scene must clear the environment and set a generated fixture `HOME`, a generated `TOKSCALE_CONFIG_DIR`, `TOKSCALE_PRICING_CACHE_ONLY=1`, `TZ=UTC`, and a fixed English UTF-8 locale. Repository documentation must use generic fixture paths, not a local absolute path.

## Acceptance checklist

- [ ] Every value and name is synthetic.
- [ ] Every image has a reproducible scene and pinned input version.
- [ ] PNG bytes match across two same-platform runs.
- [ ] Required cross-platform byte comparisons pass.
- [ ] A human inspected the full image and zoomed crops.
- [ ] Alt text describes the UI rather than marketing claims.
- [ ] The comparison labels unsupported cases honestly.
- [ ] No image or metadata contains a developer path or username.
