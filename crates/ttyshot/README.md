# ttyshot CLI

`ttyshot` drives a command in a real PTY, waits on screen state instead of fixed sleeps, and writes PNG, SVG, rectangular text, or a validated grid as JSON.

```console
ttyshot -o screenshots/app.png --size 120x40 --wait-text Ready --wait-stable 300ms -- ./my-tui
ttyshot --format png,svg,txt,json -o screenshots/app --deterministic-fonts -- ./my-tui
ttyshot run ttyshot.toml
```

A one-shot capture supports exact-text, regex, absence, and stability waits. It can send named keys, one Unicode scalar, or `ctrl-a` through `ctrl-z`. `--timeout` is a hard session deadline and cannot be extended by later waits.

Scene files use `version = 1`, `[defaults]`, one or more `[[scene]]` tables, and ordered `[[scene.step]]` actions. See [`examples/scenes.toml`](examples/scenes.toml) for the full format.

For reproducible assets, pass `--deterministic-fonts`. The default uses system fallback fonts for better local CJK coverage. Emoji is best-effort. The PTY driver cannot expose blink, frame/encircle, overline, or ideogram SGR state because Alacritty Terminal 0.26 does not retain those fields.
