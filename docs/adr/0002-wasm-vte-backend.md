# ADR 0002: Use a bounded raw-vte terminal for wasm

- Status: Accepted
- Date: 2026-10-15

## Context

`@tuishot/wasm` must parse static ANSI/VT strings and render a validated
`tuishot-core::Grid` inside `wasm32-unknown-unknown`. It cannot start a PTY,
access a process, discover fonts, or depend on native event-loop primitives.
The native API already uses `alacritty_terminal` 0.26.0, so reusing that backend
was evaluated first.

The published `alacritty_terminal` 0.26.0 crate is not a viable wasm
dependency. Its `src/lib.rs` declares `pub mod tty` unconditionally, `tty/mod.rs`
imports `polling` unconditionally, and `polling` is an unconditional dependency
in its manifest. The crate therefore cannot even be checked for that target:

```
$ cargo check --target wasm32-unknown-unknown \
    -p tuishot-ansi --no-default-features --features alacritty
error: polling does not support this target OS
   --> polling-3.11.0/src/lib.rs:118:9
```

Its platform split also treats every non-Windows target, including
`wasm32-unknown-unknown`, as Unix (`#[cfg(not(windows))] mod unix`), while the
Unix-only dependencies (`rustix`, `rustix-openpty`, `signal-hook`) are declared
under `cfg(unix)` and so are absent for wasm. This is a receiving-crate
target-layout limitation, not a missing feature flag in tuishot. We do not
patch, fork, or force native dependencies into wasm.

## Decision

Add a separate `vte` feature to `tuishot-ansi`. It depends on `vte` 0.15 with
default features disabled and uses only its raw incremental parser. Tuishot owns
a fixed-size offline terminal state and converts snapshots through
`tuishot-core`'s checked `Grid` API.

The native default remains Alacritty. When both features are enabled, existing
root names keep Alacritty semantics and the wasm-safe implementation is
explicitly available at `tuishot_ansi::vte`. A vte-only build aliases
`AnsiParser` and `ansi_to_grid` at the root. `term_to_grid` remains native-only.
This keeps `--all-features` valid without changing `@tuishot/node` output.

The offline state implements the bounded static-output subset documented in
`crates/tuishot-ansi/README.md`: split UTF-8 input, combining and two-cell
Unicode, delayed wrapping, cursor movement/save/restore, tabs, editing and
erase operations, scrolling regions, alternate screen, cursor state, and SGR
colors/decorations. Query sequences are parsed but intentionally produce no
reply. DCS/OSC/string payloads and interactive protocols have no observable
state.

All SGR fields represented by `tuishot-core` are retained. This is more style
state than Alacritty 0.26 exposes publicly: the Alacritty adapter cannot recover
blink, frame/encircle, overline, or ideogram decoration. Tests compare the two
backends only for the shared supported subset and test the additional vte style
fields directly.

Inside that shared subset the two backends must agree exactly, so the vte
terminal reproduces Alacritty's observable behavior rather than the strict
specification:

- background-color erase, so a cleared cell keeps only its background;
- `put_tab` writing the tab character into an otherwise blank origin cell,
  while `CSI I` only moves;
- `CSI J 1` leaving row 0 untouched unless the cursor line is greater than 1,
  and `CSI J 3`/`CSI ? 3 h` behavior for a terminal that keeps no scrollback;
- `CSI P` clamping its count against the whole row, so a large count blanks it;
- SGR 21 cancelling bold (the shared `vte` SGR table), partial 38/48/58 color
  parameters consuming only the values they read, and `CSI b` repeating the
  previous printed character;
- only mode 1049 switching the alternate screen, matching Alacritty's single
  implemented alternate-screen mode;
- reporting a cursor that sits on a wide continuation cell at the leading half.

`differential_tests` in `crates/tuishot-ansi/src/vte.rs` runs a 42-fixture
corpus through both backends, once whole and once for every byte split of the
input, and requires identical `Grid` values.

## Wasm binding and assets

`crates/tuishot-wasm` is a non-published `cdylib`/`rlib`. It depends on
`tuishot-ansi` with `default-features = false, features = ["vte"]`, plus core
and renderer. A dependency-graph gate rejects `alacritty_terminal`, `nix`,
`rustix`, `rustix-openpty`, and `polling`.

The JavaScript package is produced with exactly pinned `wasm-bindgen` Rust and
CLI 0.2.129, then pinned npm `binaryen` 132.0.0 `wasm-opt -Oz`. One generated
wasm asset is shared by thin browser, Node, Bun, and Deno loaders. The browser
loader fetches a URL; Node reads bytes with `node:fs/promises`; Deno uses
`Deno.readFile`; Bun uses `Bun.file`.

Rendering always chooses `FontMode::Deterministic` and the renderer's embedded
JetBrains Mono 2.304 bytes. The JS API exposes no `FontSource`, filesystem path,
or system discovery option. The npm tarball includes the exact OFL,
`FONT-LICENSES.toml`, and `NOTICE`.

## Consequences

- Static TUI strings render in-process in all four runtimes with one binary.
- Native PTY capture and Node output keep the mature Alacritty backend.
- The wasm terminal is intentionally not a drop-in interactive emulator.
  Unsupported sequences and width/shaping differences are public contract, not
  accidental omissions.
- Fuzz-like arbitrary-byte tests and custom Grid deserialization ensure wide
  pairs, cursor bounds, and cell counts stay valid.
- A differential corpus pins the shared subset, so a later divergence in either
  backend fails CI instead of silently changing wasm output.
- Bundle sizes are measured after the pinned optimizer. Regression gates use a
  checked-in measured baseline plus a 15% or 16 KiB allowance, rather than an
  unmeasured absolute limit. The embedded font is reported separately.
