# Scene file format (v1 draft)

A scene file is TOML with `version = 1`, optional defaults, and one or more `[[scene]]` tables. Steps execute in file order. Every wait has an explicit timeout or inherits the scene timeout. The scene hard timeout never extends.

```toml
version = 1

[defaults]
size = "120x40"
timeout = "30s"
env_clear = true
formats = ["png"]
output_dir = "screenshots"
theme = "dark"
scale = 1
padding = 16
chrome = "macos"
deterministic_fonts = true
fallback_fonts = ["fixtures/NotoSansMonoCJK-Regular.otf"]

[[scene]]
name = "overview"
command = ["my-tui", "--fixture", "fixtures/demo.json"]
cwd = "."
home = ".tuishot/home"
env = { TERM = "xterm-256color", LANG = "en_US.UTF-8", TZ = "UTC" }

[[scene.step]]
wait_text = "Ready"
timeout = "10s"

[[scene.step]]
key = "tab"

[[scene.step]]
type = "filter text"

[[scene.step]]
wait_regex = "Loaded [0-9]+ rows"

[[scene.step]]
wait_absent = "Loading"

[[scene.step]]
resize = "140x45"

[[scene.step]]
wait_stable = "300ms"
timeout = "5s"

[[scene.step]]
shot = "overview"
formats = ["png", "svg", "txt", "json"]
panel = "Models by Cost"
# Or: crop = { x = 4, y = 6, width = 70, height = 18 }
```

## Validation

- Unknown fields are errors.
- A step contains exactly one action: `wait_text`, `wait_regex`, `wait_absent`, `wait_stable`, `key`, `type`, `resize`, or `shot`.
- `timeout` is valid only on wait steps.
- `panel` and `crop` are mutually exclusive and valid only on `shot`.
- Scene names and resolved output paths are unique.
- `shot` is one basename component. Absolute paths, separators, `.` and `..` are rejected; use `output_dir` for directories.
- Sizes and crop dimensions are non-zero. Crops must fit the captured grid.
- Environment keys cannot contain `=` or NUL. Commands and paths cannot contain NUL.
- `version` must be exactly `1`.

Relative `cwd`, `home`, font, fixture, and output paths resolve from the scene file's directory, not the caller's current directory. A path-like command program such as `./bin/app` resolves from the scene directory; a bare program such as `gitui` still uses `PATH`. Command arguments are passed literally, so set `cwd` when they contain relative fixture paths.

## Keys

Names are case-insensitive: `enter`, `escape`, `tab`, `backspace`, arrow keys, `home`, `end`, `pageup`, `pagedown`, `insert`, `delete`, and `f1` through `f12`. A one-scalar string sends that character. `ctrl-a` through `ctrl-z` send the matching control byte. The one-shot `--keys` option accepts a comma-separated list with the same grammar.

## Outputs

A shot basename plus format list produces one file per format in `output_dir`, for example `overview.png`, `overview.svg`, `overview.txt`, and `overview.json`. PNG and SVG use the same cropped grid and render options. Text preserves all rows and trailing spaces. JSON serializes the validated shared grid, including styles, cursor, and explicit wide-cell continuations.

A panel crop includes its complete border. Panel lookup requires one visible bordered region whose top border contains the exact title. Missing or ambiguous titles are errors with the last screen.
