# ttyshot-render

Deterministic PNG and standalone SVG rendering for `ttyshot-core::Grid`.

- Box drawing, block elements, Braille, sextants, octants, and common
  Powerline separators use integer procedural geometry.
- JetBrains Mono Regular is bundled under the SIL Open Font License 1.1.
- Set `RenderOptions::cjk_fallback` to a stable local font file when a fixture
  needs CJK glyphs not covered by JetBrains Mono.
- Emoji is best-effort. Color emoji and platform-specific ZWJ composition are
  not deterministic and are not part of the renderer's compatibility promise.
- SGR blink attributes are preserved by the model but a still image shows the
  visible phase.

Generate synthetic QA files without using terminal history or user data:

```sh
cargo run -p ttyshot-render --example render_fixture -- target/ttyshot-render-qa
```

This writes dark and light PNG/SVG variants.
