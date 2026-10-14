//! Terminal cell-grid model shared by ttyshot crates.

/// Reports the crate version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
