# tuishot-ratatui

Render Ratatui `Buffer`s to tuishot's version-independent `Grid`, PNG, or SVG in process. No PTY or terminal process is required. Text and JSON snapshot helpers remain available.

## Ratatui versions

Ratatui 0.29 is enabled by default:

```toml
[dependencies]
ratatui = "0.29"
tuishot-ratatui = "0.1"
```

Select Ratatui 0.30 by disabling the default feature:

```toml
[dependencies]
ratatui = "0.30"
tuishot-ratatui = { version = "0.1", default-features = false, features = ["ratatui-0-30"] }
```

The `ratatui-0-29` and `ratatui-0-30` features may be enabled together. The crate uses aliased dependencies, so `--all-features` accepts both buffer types in one build.

## Grid, text, and JSON snapshots

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

let grid = tuishot_ratatui::to_grid(&buffer)?;
insta::assert_snapshot!(tuishot_ratatui::snapshot_text(&buffer)?);
insta::assert_snapshot!(tuishot_ratatui::snapshot_json(&buffer)?);
# let _ = grid;
# Ok::<(), tuishot_ratatui::AdapterError>(())
```

The text helper keeps the full rectangular frame, including trailing spaces. The JSON helper pretty-prints tuishot-core's explicit cell model in a stable field order, including symbols, width roles, colors, and attributes.

Cells marked as skipped for Ratatui's terminal diff are rejected with `AdapterError::SkippedCell`. Such cells can sit behind Sixel, iTerm, or Kitty graphics. A `Buffer` alone does not contain the missing pixels, so silently rendering text would not be lossless.

## In-process PNG and SVG

Use the general functions when the application needs full renderer control:

```rust
use tuishot_ratatui::{FontMode, Padding, RenderOptions};
# use ratatui::buffer::Buffer;
# use ratatui::layout::Rect;
# let buffer = Buffer::empty(Rect::new(0, 0, 5, 1));

let options = RenderOptions {
    scale: 2,
    padding: Padding::all(12),
    font_mode: FontMode::System,
    ..RenderOptions::default()
};
let png: Vec<u8> = tuishot_ratatui::render_png(&buffer, options.clone())?;
let svg: String = tuishot_ratatui::render_svg(&buffer, options)?;
# let _ = (png, svg);
# Ok::<(), tuishot_ratatui::AdapterError>(())
```

`RenderOptions::default()` uses `FontMode::System`. It can discover installed CJK fallback fonts and is suitable for normal local images. The selected system font can differ by OS.

Snapshot helpers instead default to `FontMode::Deterministic`. They use only the bundled font unless explicit fallback paths are added through `snapshot_render_options()`. This makes ASCII and tuishot's procedural box, block, Braille, sextant, octant, and Powerline output byte-stable. A CJK symbol still occupies its explicit two-cell footprint, but its shape is not promised unless the snapshot pins a fallback font. Color emoji and platform-specific ZWJ composition are best-effort and should not be used in portable goldens.

## insta-friendly image snapshots

`insta` is not a runtime dependency. Pass returned values to its macros from your own dev-dependencies:

```toml
[dev-dependencies]
insta = "1"
```

```rust
# use ratatui::buffer::Buffer;
# use ratatui::layout::Rect;
# let buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
let png = tuishot_ratatui::snapshot_png(&buffer)?;
let svg = tuishot_ratatui::snapshot_svg(&buffer)?;
insta::assert_binary_snapshot!("dashboard.png", png);
insta::assert_snapshot!("dashboard.svg", svg);
# Ok::<(), tuishot_ratatui::AdapterError>(())
```

Or atomically replace deterministic files at caller-supplied paths. The helpers create missing parent directories, sync a temporary file in the same directory, and then rename it over the destination, so a failed update does not truncate an accepted snapshot:

```rust
# use ratatui::buffer::Buffer;
# use ratatui::layout::Rect;
# let buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
tuishot_ratatui::write_snapshot_png(&buffer, "tests/snapshots/dashboard.png")?;
tuishot_ratatui::write_snapshot_svg(&buffer, "tests/snapshots/dashboard.svg")?;
# Ok::<(), tuishot_ratatui::AdapterError>(())
```
