# @tuishot/wasm

Deterministic, in-process terminal rendering for browsers, Node, Bun, and Deno.
The package contains one shared WebAssembly binary and thin runtime loaders.
It starts no process, opens no PTY, and exposes no filesystem rendering API.

```js
import { init, Renderer, renderAnsi } from "@tuishot/wasm";

// One-shot helpers initialize automatically.
const result = await renderAnsi("\x1b[31mhello\x1b[0m\n界", {
  cols: 20,
  rows: 3,
});
console.log(result.png instanceof Uint8Array, result.svg, result.txt, result.json);

// Or initialize once and use the synchronous renderer.
await init();
const renderer = new Renderer({ theme: "dark", scale: 2 });
const png = renderer.ansiToPng("hello", { cols: 10, rows: 2 });
renderer.free();
```

Runtime-specific imports are available at `@tuishot/wasm/browser`, `/node`,
`/bun`, and `/deno`. The root export selects a loader through package export
conditions. Browser loading uses `fetch(new URL(..., import.meta.url))`; Node
uses `node:fs/promises`; Deno uses `Deno.readFile`; Bun uses `Bun.file`.

## Determinism and limits

Rendering always uses the JetBrains Mono 2.304 font embedded in the wasm binary
and `FontMode::Deterministic`. The JavaScript API cannot select filesystem or
system fonts. Grid input is passed through `tuishot-core`'s custom deserializer,
including exact wide/continuation-pair validation. Inputs are limited to
1,000,000 cells and 16 MiB of ANSI or aggregate grid symbol text.

`gridToJson` and `ansiToJson` return compact JSON in stable Rust struct field
order. Text keeps the complete rectangular grid. PNG output is a `Uint8Array`.

## ANSI backend fidelity

The wasm build uses `vte` 0.15 only as a streaming byte/UTF-8/escape parser.
`tuishot-ansi` owns the terminal screen state. Supported behavior includes:

- split UTF-8 feeds, combining marks, Unicode wide cells, delayed wrapping,
  CR/LF, tabs, and DEC special graphics;
- absolute/relative cursor movement, save/restore, visibility and block/bar/
  underline shape;
- erase, insert, and delete for characters and lines;
- scrolling margins, index/reverse-index, basic scrolling, and alternate screen;
- SGR 16/256/RGB colors, underline color and variants, plus every style field
  represented by `tuishot-core`.

This is a bounded static-output emulator, not an interactive terminal. It does
not implement sixel/ReGIS/kitty graphics, hyperlinks or OSC clipboard/title
state, mouse/focus/keyboard protocols, synchronized-update buffering, bidi or
full grapheme shaping, protected/selective erase, or terminal history/reflow.
DECCOLM cannot resize a fixed screen, so only its side effects apply.
DCS/OSC/APC/PM/SOS payloads and terminal/device/window queries are accepted and
ignored; query replies are explicit no-ops.

Inside the supported subset the wasm backend produces the same `Grid` as the
native Alacritty backend used by the CLI and `@tuishot/node`. A differential
test in `crates/tuishot-ansi` compares both backends over a 42-fixture corpus,
whole and at every byte split. Width comes from `unicode-width`, so ambiguous
width and complex emoji or ZWJ shaping can still differ from an interactive
terminal.

The native Alacritty backend cannot be reused here: published
`alacritty_terminal` 0.26.0 declares `pub mod tty` unconditionally, that module
imports `polling` unconditionally, and `polling` fails to compile for
`wasm32-unknown-unknown`. See `docs/adr/0002-wasm-vte-backend.md`.

Font license and provenance are included as `OFL-JetBrainsMono.txt`,
`FONT-LICENSES.toml`, and `NOTICE`.
