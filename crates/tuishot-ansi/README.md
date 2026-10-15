# tuishot-ansi

`tuishot-ansi` converts ANSI/VT byte streams into the validated
[`tuishot-core`](https://crates.io/crates/tuishot-core) cell-grid model without
starting a PTY or child process. It is the platform-neutral emulator seam shared
by `tuishot-term` and in-process language bindings.

The default `alacritty` feature uses Alacritty Terminal's VT state machine. The
feature is optional so a future web/wasm adapter can use
`default-features = false` and select a web-compatible backend without pulling
in Alacritty.

## Newline policy

Static strings commonly come from renderers such as Ink's `renderToString`,
which emit bare LF (`\n`). A terminal normally treats LF as “move down while
keeping the current column”. `AnsiParser` deliberately normalizes each bare LF
to CRLF so it starts a new line at column zero. Existing CRLF is preserved
exactly, including when CR and LF arrive in separate `feed` calls. Other bytes
are sent unchanged to the VT parser.

```rust
use tuishot_ansi::ansi_to_grid;

let grid = ansi_to_grid("\x1b[31mhello\x1b[0m\nworld", 80, 24)?;
# Ok::<(), tuishot_ansi::AnsiError>(())
```

Alacritty Terminal 0.26 does not expose blink, frame/encircle, overline, or
ideogram decoration in its public cell state. Those attributes therefore remain
at their `tuishot-core` defaults.

Licensed under MIT.
