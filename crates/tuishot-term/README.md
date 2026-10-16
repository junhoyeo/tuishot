# tuishot-term

Headless PTY driving and terminal emulation for deterministic TUI capture.

`tuishot-term` launches a command at an exact terminal size, converts its
screen into `tuishot-core::Grid`, sends named keys, resizes the real PTY, and
waits on text, regular expressions, absence, or screen stability. Sessions use
hard deadlines and clean up the complete child process group.

## Install

```toml
[dependencies]
tuishot-term = "0.2.1"
```

API documentation is available on [docs.rs](https://docs.rs/tuishot-term).
