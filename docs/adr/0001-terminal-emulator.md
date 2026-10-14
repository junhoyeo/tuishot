# ADR 0001: Use `alacritty_terminal` for the v0.1 emulator

- Status: accepted
- Date: 2026-10-15

## Context

The terminal driver must produce a styled cell grid and must behave like a terminal when a child asks questions. In particular, v0.1 must answer primary and secondary device attributes (DA1/DA2), device status and cursor position (DSR/CPR), dynamic foreground/background color queries (OSC 10/11), and kitty keyboard mode queries.

We evaluated `alacritty_terminal` 0.26.0 and `vt100` 0.16.2 from their published crate sources and APIs.

## Evidence

| Concern | `alacritty_terminal` 0.26.0 | `vt100` 0.16.2 |
| --- | --- | --- |
| Query replies | `Term` emits `Event::PtyWrite` for DA1, DA2, DSR, CPR, and kitty keyboard mode reports. It emits `Event::ColorRequest` with the correct OSC reply formatter for dynamic colors. | `perform.rs` implements screen mutations but routes unrecognized CSI/OSC commands to callbacks. DA, DSR/CPR, OSC 10/11, and kitty keyboard queries have no built-in reply implementation. We would need to build and maintain a second query parser beside its emulator. |
| Screen API | Public `Term::grid`, `Grid::display_iter`, cell color/flag/zero-width data, cursor state, mode, and `Term::resize`. | Public `Screen` exposes cells, attributes, cursor, text, and resize. Its extraction API is simpler, but does not offset the query gap. |
| Correctness surface | This is the terminal core used by Alacritty. It handles alternate screens, scroll regions, wide/zero-width cells, modern SGR underline forms, synchronized updates, and kitty keyboard modes. | It is a small VT100/xterm parser with a good screen/diff API. Its `Perform` dispatch is intentionally narrower; unsupported sequences go through callbacks. |
| Direct dependencies | 12 normal dependencies with default features disabled in ttyshot. Several support Alacritty's PTY/event-loop modules even though ttyshot only uses `Term`. | 3 normal dependencies (`itoa`, `unicode-width`, `vte`). |
| License / MSRV | Apache-2.0, Rust 1.85. | MIT, Rust 1.70. |

The concrete source points used above are:

- `alacritty_terminal/src/term/mod.rs`: `identify_terminal`, `device_status`, `report_keyboard_mode`, and `dynamic_color_sequence`.
- `alacritty_terminal/src/event.rs`: `Event::PtyWrite` and `Event::ColorRequest`.
- `vt100/src/perform.rs`: CSI/OSC dispatch and fallback callback paths.
- the crates' normalized `Cargo.toml` files for dependency, license, and MSRV data.

## Decision

Use `alacritty_terminal` 0.26.0 with default features disabled. ttyshot supplies a small event listener that writes terminal replies to the PTY and resolves OSC 10/11 from the selected deterministic theme.

Use a narrow Unix PTY/process helper owned by ttyshot instead of Alacritty's PTY wrapper. Alacritty's `tty::Options` always inherits the parent environment, while ttyshot requires a true environment-clear mode and exact process-group lifetime control.

## Consequences

- v0.1 gets the required query behavior from the same state machine that tracks the screen. Integration tests must run query-producing children and assert the replies they receive.
- The emulator dependency is heavier than `vt100`, but avoids a parallel escape-sequence implementation and gives better modern-TUI compatibility.
- ttyshot has a small documented `unsafe` boundary for `setsid`, controlling-terminal setup, window-size ioctls, and process-group signaling. The rest of `ttyshot-term` remains `unsafe`-forbidden.
- If dependency size becomes material, a later ADR can evaluate a focused upstream feature split or a smaller emulator only after it provides the same query contract.
