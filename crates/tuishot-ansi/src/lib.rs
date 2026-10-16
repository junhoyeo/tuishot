//! Platform-neutral ANSI/VT input to [`tuishot_core::Grid`] conversion.
//!
//! The default native backend uses Alacritty Terminal. The optional `vte`
//! backend is an explicit offline terminal state that also compiles for
//! `wasm32-unknown-unknown`.

use std::{error::Error, fmt};

pub use tuishot_core as core;

/// A static ANSI-emulation configuration error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnsiError {
    /// Terminal dimensions must be non-zero.
    EmptyDimensions,
}

impl fmt::Display for AnsiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDimensions => {
                formatter.write_str("ANSI terminal dimensions must be non-zero")
            }
        }
    }
}

impl Error for AnsiError {}

#[cfg(feature = "alacritty")]
pub mod alacritty;
#[cfg(feature = "vte")]
pub mod vte;

// Preserve the historical root API whenever Alacritty is selected, including
// `--all-features`. A vte-only consumer gets the same convenient aliases.
#[cfg(feature = "alacritty")]
pub use alacritty::{AnsiParser, ansi_to_grid, term_to_grid};
#[cfg(all(not(feature = "alacritty"), feature = "vte"))]
pub use vte::{AnsiParser, ansi_to_grid};

/// Reports the crate version.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
