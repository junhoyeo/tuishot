# ttyshot-ratatui

Convert a Ratatui `Buffer` into ttyshot's version-independent `Grid` for deterministic snapshots.
The adapter preserves grapheme strings, one- and two-cell width roles, colors, underline colors,
and all Ratatui modifiers.

## Ratatui versions

Ratatui 0.29 is enabled by default because it is the version used by ttyshot's first-party
consumer. Ratatui 0.30 users should disable default features and select 0.30 explicitly:

```toml
# Ratatui 0.29 (default)
ttyshot-ratatui = "0.1"

# Ratatui 0.30
ttyshot-ratatui = { version = "0.1", default-features = false, features = ["ratatui-0-30"] }
```

The `ratatui-0-29` and `ratatui-0-30` features may also be enabled together. The crate uses
aliased dependencies, so `--all-features` supports both buffer types in the same build.

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

let mut buffer = Buffer::empty(Rect::new(0, 0, 5, 1));
buffer.set_string(0, 0, "hello", ratatui::style::Style::default());

let grid = ttyshot_ratatui::to_grid(&buffer)?;
insta::assert_snapshot!(ttyshot_ratatui::snapshot_text(&buffer)?);
insta::assert_snapshot!(ttyshot_ratatui::snapshot_json(&buffer)?);
# Ok::<(), ttyshot_ratatui::AdapterError>(())
```

The text helper keeps the full rectangular frame, including trailing spaces. The JSON helper uses
ttyshot-core's stable, explicit grid model and pretty-prints fields in a deterministic order.

This adapter intentionally stops at `Grid`. An in-process image helper will be added by stacking on
the renderer work; this crate does not duplicate renderer code.
