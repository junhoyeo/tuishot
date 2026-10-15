# tuishot-core

Shared terminal cell-grid types for deterministic TUI screenshots.

`tuishot-core` defines the version-independent data model used by the tuishot
PTY driver, renderer, and Ratatui adapter. A `Grid` stores explicit cells,
width roles, colors, SGR attributes, cursor state, and deterministic themes.
The model supports serde so the same frame can back JSON, text, PNG, and SVG
outputs.

## Install

```toml
[dependencies]
tuishot-core = "0.1.0"
```

API documentation is available on [docs.rs](https://docs.rs/tuishot-core).
