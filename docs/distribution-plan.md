# Binary distribution plan

This is a preparation document. Do not publish a release, Homebrew formula, crate, npm package, or make the repository public without Junho's approval.

## Launch targets

| Archive | Rust target | Runner | Linking goal |
| --- | --- | --- | --- |
| `ttyshot-aarch64-apple-darwin.tar.gz` | `aarch64-apple-darwin` | local Apple Silicon Mac | one signed CLI binary plus licenses |
| `ttyshot-x86_64-apple-darwin.tar.gz` | `x86_64-apple-darwin` | local/cross-compiled, then verified on Intel macOS | one signed CLI binary plus licenses |
| `ttyshot-x86_64-unknown-linux-musl.tar.gz` | `x86_64-unknown-linux-musl` | Linux x86_64 | static musl binary plus licenses |
| `ttyshot-aarch64-unknown-linux-musl.tar.gz` | `aarch64-unknown-linux-musl` | Linux arm64 | static musl binary plus licenses |

Every archive contains `ttyshot`, `LICENSE-MIT`, `LICENSE-APACHE`, the bundled font's OFL file, and `SHA256SUMS`. The binary reports the same v0.1 version and source commit used in the archive name.

## Release gates

1. Build from a signed tag only after explicit publication approval.
2. Use locked dependencies. Do not enable LTO unless a measured size/startup comparison justifies it.
3. Run `fmt`, clippy `-D warnings`, locked tests, cargo-deny, the synthetic scene example, and deterministic golden hashes before packaging.
4. Run the binary on its native target. Cross-compilation alone is not verification.
5. Capture `ttyshot --version`, `file`, SHA-256, and the synthetic scene output for every binary.
6. Compare the deterministic PNG golden hash across macOS and both Linux architectures.
7. Scan archives for local paths, credentials, debug fixtures, and unlisted files.
8. Upload artifacts without publishing a GitHub Release first. Junho approves the final release/publication step.

The repository remains on Linux-only hosted Actions while private. macOS verification runs locally; the public-release workflow can add billed macOS runners only after that policy changes.
