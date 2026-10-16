# tuishot-wasm

Private `wasm-bindgen` boundary used to build `@tuishot/wasm`.

The crate depends on `tuishot-ansi` with `default-features = false` and only the
`vte` backend. It never links the native terminal stack. Rendering always uses
the bundled JetBrains Mono font in deterministic mode; no filesystem font path
or system font discovery is exposed to JavaScript.
