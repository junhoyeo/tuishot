# ttyshot-render

Deterministic PNG and standalone SVG rendering for `ttyshot-core::Grid`.

- Box drawing, block elements, Braille, sextants, octants, Powerline
  separators U+E0B0–U+E0B7, and common scrollbar glyphs `▴▮▾` use integer
  procedural geometry.
- JetBrains Mono Regular 2.304 is bundled under the exact SIL Open Font
  License 1.1 text in `assets/OFL-JetBrainsMono.txt`. Its release URL and
  SHA-256 are pinned in `assets/FONT-LICENSES.toml` and checked by tests.
- `FontMode::System` adds ordered installed CJK fallbacks. macOS tries Apple SD
  Gothic Neo, PingFang, then Hiragino. Linux tries the standard Noto CJK
  locations. `FontSource::collection` selects a face from TTC/OTC files.
- Use `FontMode::Deterministic` for goldens. This mode uses only the bundled
  font plus explicit primary and fallback paths. It never discovers system
  fonts. The repository golden tests always use this mode.
- The macOS Korean integration check is explicitly platform-gated and is not a
  portable golden. It verifies the default local system fallback only.
- Emoji is best-effort. Color emoji and platform-specific ZWJ composition are
  not deterministic and are not part of the renderer's compatibility promise.
- SGR blink attributes are preserved by the model but a still image shows the
  visible phase.
- `cargo deny check` audits Rust dependency licenses separately from the font
  manifest and exact bundled OFL text.

Generate synthetic QA files without using terminal history or user data:

```sh
cargo run -p ttyshot-render --example render_fixture -- target/ttyshot-render-qa
```

This writes dark and light 2x PNG/SVG variants plus box, style, and CJK crops.
