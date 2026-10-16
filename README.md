![tuishot collage showing Tokscale, LazyGit, Ratatui, and Yazi](https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/hero/tuishot-hero.png)

# tuishot

**Screenshots of any TUI that look like a real terminal — and stay up to date.**

- **Seam-free terminal graphics.** Boxes, blocks, Braille, sextants, octants, Powerline, and scrollbars use cell geometry.
- **State-based capture.** Wait for text, regex, absence, or stability instead of guessing a sleep.
- **One frame, four formats.** Export PNG, SVG, rectangular text, and styled JSON from the same grid.

## 30-second quick start

```sh
cargo install tuishot --locked
tuishot -o dashboard.png --size 120x40 --wait-text "Ready" -- my-tui
```

The command runs at exactly `120x40`, waits for `Ready`, writes `dashboard.png`, and terminates the complete child process group. A timeout names the failed condition and prints the last screen.

## Gallery

Open any image for the full-resolution tuishot capture.

<table>
  <tr>
    <td width="50%"><strong>Tokscale</strong><br><sub>75-day generated usage dataset</sub><br><a href="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/tokscale.png"><img src="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/tokscale.png" alt="Tokscale overview rendered from a generated usage dataset by tuishot"></a></td>
    <td width="50%"><strong>LazyGit</strong><br><sub>Orbit Notes working tree</sub><br><a href="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/lazygit.png"><img src="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/lazygit.png" alt="LazyGit showing the generated Orbit Notes project"></a></td>
  </tr>
  <tr>
    <td width="50%"><strong>Ratatui demo2</strong><br><sub>official showcase application</sub><br><a href="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/ratatui.png"><img src="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/ratatui.png" alt="Official Ratatui demo2 rendered by tuishot"></a></td>
    <td width="50%"><strong>Helix</strong><br><sub>Rust code in Catppuccin Mocha</sub><br><a href="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/helix.png"><img src="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/helix.png" alt="Helix showing generated syntax-highlighted Rust code"></a></td>
  </tr>
  <tr>
    <td colspan="2"><strong>Yazi</strong><br><sub>Aurora UI Kit directory</sub><br><a href="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/yazi.png"><img src="https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/gallery/yazi.png" alt="Yazi showing the generated Aurora UI Kit directory"></a></td>
  </tr>
</table>

## Scripted scenes

For a sequence with more than one frame:

```sh
tuishot run tuishot.toml
```

```toml
version = 1

[defaults]
size = "120x40"
timeout = "30s"
env_clear = true
output_dir = "screenshots"

[[scene]]
name = "dashboard"
command = ["my-tui", "--fixture", "fixtures/demo.json"]
cwd = "."
env = { TERM = "xterm-256color", LANG = "en_US.UTF-8" }

[[scene.step]]
wait_text = "Ready"

[[scene.step]]
key = "tab"

[[scene.step]]
wait_stable = "300ms"

[[scene.step]]
shot = "dashboard"
formats = ["png", "svg", "txt", "json"]
panel = "Models by Cost"
```

A shot can crop by a cell rectangle or by the title of a bordered panel. Commands can inherit or clear the environment, set exact `K=V` values, choose a working directory, and use an isolated home directory.

See the validated v1 schema and key grammar in [`docs/scene-format.md`](docs/scene-format.md).


## What makes the output exact

![Comparison showing gaps between font-rendered block glyphs and seamless tuishot cell geometry](https://raw.githubusercontent.com/junhoyeo/tuishot/main/docs/assets/hero/geometry-comparison.png)

- **One cell model.** Text, colors, SGR attributes, cursor state, and wide-cell continuations survive PTY, Ratatui, PNG, SVG, text, and JSON paths.
- **Terminal geometry.** `U+2500–257F`, `U+2580–259F`, `U+2800–28FF`, legacy sextants/octants, Powerline separators, and common scrollbar glyphs are shapes aligned to cell edges.
- **Pinned typography.** tuishot bundles JetBrains Mono under the SIL Open Font License 1.1. PNG uses the bundled bytes; SVG emits reusable paths from the same selected glyphs instead of embedding a full font.
- **Hermetic sessions.** `--env-clear`, repeatable `--env K=V`, `--cwd`, `--home`, an exact PTY size, and hard process-group timeouts remove common CI differences.
- **No fixed sleeps.** Use `wait_text`, `wait_regex`, `wait_absent`, or `wait_stable` with explicit timeouts.

PNG encoding is deterministic. The test suite renders the same golden twice, checks the final row and column, checks a two-cell CJK glyph, and scans adjacent full blocks for background-colored seams at 1x and 2x. Linux and macOS compare the same fixture bytes before release.

## One-shot capture

```sh
tuishot \
  -o screenshots/status.png \
  --size 100x30 \
  --wait-regex 'Connected|Ready' \
  --keys 'tab,enter' \
  --env-clear \
  --env TERM=xterm-256color \
  --env LANG=en_US.UTF-8 \
  --cwd ./fixtures \
  -- ./demo-tui
```

Available output formats are `png`, `svg`, `txt`, and `json`. Text is useful for assertions. JSON preserves styles and explicit wide-cell continuations for coding agents and structured diffs.

## Ratatui, without a PTY

Tokscale and other Ratatui applications can convert a `Buffer` directly:

```toml
[dev-dependencies]
tuishot-ratatui = { version = "0.2.1", features = ["ratatui-0-29"] }
```

```rust
use tuishot_ratatui::{
    snapshot_json, snapshot_png, snapshot_svg, snapshot_text, BufferExt,
};

let grid = buffer.to_tuishot_grid()?;
insta::assert_snapshot!(snapshot_text(&buffer)?);
insta::assert_snapshot!(snapshot_json(&buffer)?);
insta::assert_binary_snapshot!("dashboard.png", snapshot_png(&buffer)?);
insta::assert_snapshot!(snapshot_svg(&buffer)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Ratatui 0.29 is the default integration. Ratatui 0.30 is available with `default-features = false, features = ["ratatui-0-30"]`.

## CI recipe

Commit a scenes file and synthetic fixtures, then let the composite action update images in a pull request:

```yaml
name: Refresh terminal screenshots
on:
  workflow_dispatch:
  schedule:
    - cron: "17 4 * * 1"

permissions:
  contents: write
  pull-requests: write

jobs:
  screenshots:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false
      - uses: junhoyeo/tuishot@v0
        with:
          scenes: tuishot.toml
          paths: screenshots
          branch: tuishot/update-screenshots
```

Use fixture data in public images. Do not capture a developer's real home directory, account totals, paths, tokens, or model history.

The implemented action inputs, trusted-ref model, and branch-safety contract are documented in [`docs/github-action.md`](docs/github-action.md).

## Coding-agent recipe

Give an agent both pixels and state:

```sh
tuishot \
  -o /tmp/settings.png --format png,json,txt \
  --size 100x30 --wait-text "Settings" \
  -- my-tui
```

The agent can inspect the PNG, assert text without OCR, and read exact cell colors from JSON. If the screen never appears, tuishot exits non-zero with the failed wait and last screen instead of hanging.

## Use from Node

Two published packages cover process capture and in-process rendering. The WASM package is still in development.

| Package | Use it when | Needs a PTY | Platforms |
| --- | --- | --- | --- |
| `@tuishot/cli` | You want to drive a real TUI process and wait for state | Yes | macOS and Linux |
| `@tuishot/node` | You already have an ANSI string or a grid and want native speed | No | macOS, Linux, Windows |
| `@tuishot/wasm` (not yet published) | Planned browser, Bun, and Deno rendering | No | Pending parser-parity validation |

### Run the CLI without installing it

```sh
npx @tuishot/cli -o dashboard.png --size 120x40 --wait-text "Ready" -- my-tui
bunx @tuishot/cli -o dashboard.png --size 120x40 --wait-text "Ready" -- my-tui
```

Install `@tuishot/cli`; its executable is still named `tuishot`. npm rejected the unscoped
`tuishot` package name because it is too similar to `teashot`, so that alias is not published.
Installing the scoped package pulls exactly one prebuilt binary through `optionalDependencies`,
selected by your platform, architecture, and libc.
No compiler and no `cargo` are required. If no matching package is installed, the launcher names the
expected package instead of failing with a missing-file error.

### Capture a TUI from TypeScript

```ts
import { shoot } from "@tuishot/cli";

const { png, json } = await shoot({
  command: "my-tui",
  size: "120x40",
  waitText: "Ready",
  formats: ["png", "json"],
  envClear: true,
  env: { TERM: "xterm-256color", COLORTERM: "truecolor" },
  timeout: "10s",
});

await writeFile("dashboard.png", png!);
console.log(json!.cells[0].style.foreground);
```

`shoot` spawns the same native binary the CLI uses, so captures are identical. `runScenes(path)` runs
a scene file for multi-frame sequences. `waitRegex`, `waitStable`, `keys`, `cwd`, `home`, `envClear`,
`scale`, and `chrome` map one-to-one onto the CLI flags. Durations use the native string form
(`300ms`, `2s`). The package ships ESM, CommonJS, and type declarations, and requires Node >= 18.

### Render an ANSI string in-process with `@tuishot/node`

When the output is already a string — for example from Ink's `renderToString` — skip the PTY:

```ts
import { renderAnsi } from "@tuishot/node";

const { png, svg, txt, json } = renderAnsi("\u001b[31mhello\u001b[0m\n界", {
  cols: 80,
  rows: 24,
  scale: 2,
  theme: "dark",
});
```

`renderGrid(grid, options)` renders a grid you built or captured earlier. Both are synchronous
Node-API calls with no process spawn. A bare `\n` starts a new row at column zero, so renderer output
lands where you expect; existing CRLF is preserved. See `examples/ink` for Vitest snapshots of an Ink
component.

### Browser, Bun, and Deno rendering (planned)

`@tuishot/wasm` is not published yet. It remains on a development branch while
parser-parity tests are completed. Use `@tuishot/node` for in-process rendering
in Node.js today.

### Honest platform notes

- **The CLI drives real PTYs on macOS and Linux.** `@tuishot/cli` ships six binaries: macOS arm64 and
  x64, and Linux x64 and arm64 for both glibc and musl. The PTY driver in `tuishot-term` uses Unix
  process groups and signals, so it does not compile for `*-pc-windows-msvc` yet. A Windows ConPTY
  backend is planned post-launch. **Windows users can render in-process today** with `@tuishot/node`
  without opening a PTY.
- **`@tuishot/node` is a native addon.** It installs a prebuilt `.node` binary per platform. On an
  unlisted platform, installation succeeds but importing fails with the expected package name.
  Check the supported target list before installing; the WASM fallback is not published yet.
- **Emoji are best-effort in v0.1** in every package. Prefer text symbols for byte-stable snapshots.

## CJK, deterministic fonts, and emoji

By default, tuishot uses an ordered system CJK fallback: Apple SD Gothic Neo / PingFang / Hiragino on macOS and Noto Sans CJK on Linux when installed. CJK graphemes still occupy the explicit one- or two-cell footprint supplied by the terminal grid.

For byte-stable CI, disable system discovery and pin every font file:

```sh
tuishot --deterministic-fonts \
  --font ./fixtures/JetBrainsMono-Regular.ttf \
  --fallback-font ./fixtures/NotoSansMonoCJK-Regular.otf \
  -o screenshot.png -- my-tui
```

`--font` selects the primary face. Repeat `--fallback-font` for an ordered fallback list. Font collections can select a face index with the scene-file font settings.

Emoji are best-effort in v0.1. Color emoji fonts, platform-specific composition, and ZWJ presentation can differ. For byte-stable CI, prefer text symbols or provide a pinned monochrome fallback font.

## Known terminal-emulation limits

The shared grid and renderer represent blink, frame/encircle, overline, and ideogram decorations. `alacritty_terminal` 0.26 does not expose those SGR states after parsing, so PTY captures cannot preserve them in v0.1. Ratatui/in-process grids can still render them. This gap is documented rather than silently claimed as terminal-driver support.

## Comparison

Checked against project documentation and source on 2026-10-15. “Different focus” is not a defect; these tools solve related but distinct jobs.

| Project | Best at | Difference from tuishot |
| --- | --- | --- |
| [VHS](https://github.com/charmbracelet/vhs) | Mature scripted terminal recordings, `Wait`, and PNG screenshots | Uses ttyd, ffmpeg, and a browser/xterm.js pipeline; screenshots are PNG rather than PNG + SVG + cell JSON |
| [freeze](https://github.com/charmbracelet/freeze) | Polished images of code and static terminal output | Does not drive a multi-step interactive TUI session |
| [termshot](https://github.com/homeport/termshot) | Simple screenshots of command output | Focuses on command output rather than deterministic scripted TUI states |
| [termframe](https://github.com/pamburus/termframe) | Running one command and exporting SVG, with optional font embedding | Non-interactive single-command flow rather than step scripts and in-process Ratatui buffers |
| [termscope](https://github.com/mwunsch/termscope) | Headless VT snapshots, text/idle waits, and agent workflows using libghostty-vt | tuishot also targets deterministic PNG, shared cell JSON, and a versioned Ratatui adapter |
| [toque](https://github.com/joelgwebber/toque) | Headless Ratatui input, text frames, and SVG | Font-rendered block charts can show seams; tuishot draws terminal graphics from cell geometry |
| [ratasvg](https://github.com/sermuns/ratasvg) | Direct Ratatui-buffer-to-SVG conversion | tuishot preserves cell backgrounds/attributes and also drives arbitrary PTY applications |
| [Honeyshots](https://github.com/honeymux/honeyshots) | Component-level screenshots and named regions for OpenTUI/React | Targets OpenTUI rather than Ratatui or arbitrary terminal processes |

## License

Licensed under the [MIT License](LICENSE-MIT). The bundled JetBrains Mono font has its own SIL Open Font License 1.1 file.
