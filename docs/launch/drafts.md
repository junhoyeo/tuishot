# Launch post drafts

These are drafts only. Do not post, publish a package, create a release, or make the repository public without Junho's approval. Replace every bracketed field with verified release evidence. Remove any sentence whose claim has not been measured on the released artifact.

## Show HN

**Title**

Show HN: ttyshot – deterministic PNG/SVG screenshots of interactive terminal UIs

**Body**

I built ttyshot because terminal UI screenshots tend to be either manual and stale or automated but visually different from a real terminal.

ttyshot runs any TUI in an exact-size PTY, waits for screen conditions instead of sleeping, and exports PNG, SVG, text, and structured cell JSON. It draws box characters, blocks, Braille, sextants/octants, and Powerline separators from cell geometry, so adjacent charts and borders join cleanly at 1x and 2x. It also has an in-process Ratatui adapter for 0.29 and 0.30.

A scene can clear the environment, set a fixture HOME, send keys, resize, wait for text/regex/absence/stability, and take multiple cropped shots. Timeouts kill the complete process group and include the last screen.

Repository: [URL]
Hero comparison: [URL]

The main limits in v0.1 are [verified limits]. Emoji rendering is best-effort; byte-stable fixtures should use the bundled text font or a pinned monochrome fallback. I would value reports for TUIs or escape sequences that the fixture suite missed.

## r/rust

**Title**

`ttyshot`: deterministic screenshots and snapshots for Ratatui and any PTY-based TUI

**Body**

`ttyshot` is a Rust CLI and library that captures a headless terminal cell grid and renders it to PNG, SVG, text, or JSON. The renderer uses integer cell geometry for terminal drawing characters rather than trusting font glyphs. The driver uses [verified emulator] and a real PTY, with query replies for DA, DSR/CPR, OSC 10/11, and kitty keyboard mode.

For Ratatui, `ttyshot-ratatui` converts 0.29 or 0.30 `Buffer` values to the same version-independent grid used by the PTY path. This makes styled `insta` snapshots possible without a subprocess.

Links: [repository] · [docs] · [comparison]

I would especially appreciate feedback on the public grid API, procedural glyph coverage, and fixture ergonomics before 0.2.

## r/commandline

**Title**

A no-sleep CLI for reproducible screenshots of interactive terminal programs

**Body**

[One sentence about the problem, verified at release.]

Example:

```sh
ttyshot -o dashboard.png --size 120x40 --wait-text Ready -- my-tui
```

For multi-step sessions, a TOML scene can wait on text or regex, wait until the screen is stable, send keys, resize, and take multiple PNG/SVG/text/JSON shots. Every wait has a timeout; failures print the last screen and terminate the child process group.

Source and examples: [URL]

## This Week in Rust submission

[ttyshot](URL) is a CLI and Rust library for deterministic screenshots of interactive terminal UIs. It drives an exact-size PTY or accepts Ratatui 0.29/0.30 buffers, then exports PNG, SVG, text, and structured cell JSON. Terminal drawing characters use seam-free cell geometry, and scripted waits avoid fixed sleeps.

## GeekNews

**Title**

ttyshot – reproducible PNG/SVG screenshots for terminal UIs

**Body**

ttyshot captures interactive TUIs from a real PTY and keeps screenshots reproducible in CI. It supports condition-based waits, key and resize steps, cell/panel crops, deterministic terminal-glyph geometry, and Ratatui buffer snapshots. See the synthetic comparison and setup guide at [URL].

## Release-day evidence to fill in

- Release tag and served binary checksums: [pending]
- macOS architecture and verification output: [pending]
- Linux arm64 verification output: [pending]
- Linux amd64 verification output: [pending]
- Golden PNG SHA-256 values on all three targets: [pending]
- Fixture applications and pinned versions: [pending]
- Known query/rendering gaps: [pending]
- Package/repository visibility approval: [pending]
