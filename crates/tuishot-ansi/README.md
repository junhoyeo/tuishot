# tuishot-ansi

`tuishot-ansi` converts ANSI/VT byte streams into the validated
[`tuishot-core`](https://crates.io/crates/tuishot-core) cell-grid model without
starting a PTY or child process.

## Backends

- The default `alacritty` feature uses Alacritty Terminal on native targets and
  preserves the existing root `AnsiParser`, `ansi_to_grid`, and `term_to_grid`
  API.
- The optional `vte` feature uses `vte` 0.15's raw streaming parser plus an
  explicit fixed-size offline terminal state. It is wasm-safe. Enable it with
  `default-features = false, features = ["vte"]`.
- With both features, the historical root names still select Alacritty and the
  second backend is available as `tuishot_ansi::vte`. With only `vte`, its
  parser and one-shot helper are also aliased at the crate root.

```rust
use tuishot_ansi::ansi_to_grid;

let grid = ansi_to_grid("\x1b[31mhello\x1b[0m\nworld", 80, 24)?;
# Ok::<(), tuishot_ansi::AnsiError>(())
```

## Newline policy

Static strings commonly come from renderers such as Ink's `renderToString`,
which emit bare LF (`\n`). Both backends normalize each bare LF to CRLF so it
starts a new row at column zero. Existing CRLF is preserved, including when CR
and LF arrive in separate `feed` calls. Parser UTF-8 state also survives
arbitrary feed boundaries.

## VTE fidelity boundary

The offline backend supports the subset used by common TUI and Ink snapshots:

- printable UTF-8, combining marks, Unicode two-cell width, delayed wrap,
  newline, backspace, tabs, and DEC special graphics;
- absolute and relative cursor movement, save/restore, visibility, and
  block/bar/underline shape;
- erase, insert, and delete for characters and lines;
- scrolling margins, index/reverse-index, basic scroll commands, origin and
  insert modes, and alternate screen modes 47/1047/1049;
- SGR ANSI/bright/indexed/RGB foreground and background, underline color,
  five underline forms, and every remaining `tuishot-core` attribute: bold,
  dim, italic, blink, reverse, hidden, strike, frame/encircle, overline, and
  ideogram decorations.

It intentionally does **not** claim full terminal compatibility. Unsupported or
ignored behavior includes sixel/ReGIS/kitty graphics, OSC 8 hyperlinks and OSC
clipboard/title state, mouse/focus/keyboard protocols, synchronized-update
buffering, bidi/full grapheme shaping, protected/selective erase, scrollback
history/reflow, DECCOLM/resizing, and most DEC private modes. DCS, OSC, APC, PM,
and SOS payloads are ignored. Device, status, terminal-parameter, and window
queries produce no replies because the terminal is offline.

The raw parser replaces malformed UTF-8 as defined by `vte`. Width comes from
`unicode-width`; ambiguous-width policy and complex emoji/ZWJ shaping can differ
from a user's interactive terminal. The backend preserves all style fields it
understands rather than silently dropping fields. Native Alacritty 0.26 does
not expose blink, frame/encircle, overline, or ideogram state, so differential
tests deliberately compare only their shared supported subset.

Inside that shared subset the two backends agree cell for cell. A differential
test feeds a 42-fixture corpus through both backends, whole and at every byte
split, and requires identical grids. To reach that, the offline backend copies
Alacritty's observable behavior where it differs from the strict specification:
background-color erase, HT writing a tab character into a blank cell while
`CSI I` only moves, `CSI J 1` not clearing row 0 unless the cursor line is past
1, `CSI P` clamping against the whole row, SGR 21 cancelling bold, partial
38/48/58 color parameters consuming only what they read, `CSI b` repeating the
previous character, mode 1049 as the only alternate-screen mode, and reporting a
cursor on a wide continuation cell at its leading half.

Every snapshot is rebuilt through `Grid::put`, and arbitrary-byte tests
round-trip through `Grid`'s custom deserializer. A malformed wide/continuation
pair cannot escape the backend.

See [`docs/adr/0002-wasm-vte-backend.md`](../../docs/adr/0002-wasm-vte-backend.md)
for the wasm decision and exact Alacritty incompatibility.

Licensed under MIT.
