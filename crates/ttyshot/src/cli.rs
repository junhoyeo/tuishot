use crate::parse::{Format, parse_duration, parse_env, parse_key, parse_size};

fn parse_nonempty_wait(value: &str) -> Result<String, String> {
    if value.is_empty() {
        Err("wait value must not be empty".to_owned())
    } else {
        Ok(value.to_owned())
    }
}
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{path::PathBuf, time::Duration};
use ttyshot_core::Theme;
use ttyshot_render::{FontMode, Padding, RenderOptions, WindowChrome};
use ttyshot_term::{Key, Size};

#[derive(Debug, Parser)]
#[command(name="ttyshot", version, about="Exact, scripted screenshots of terminal user interfaces", long_about=None)]
pub struct Cli {
    #[command(subcommand)]
    pub action: Option<Action>,
    #[command(flatten)]
    pub capture: CaptureArgs,
}
#[derive(Debug, Subcommand)]
pub enum Action {
    /// Run every scene in a strict TOML scene file.
    Run {
        /// Path to the version-1 scene file.
        scene_file: PathBuf,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ThemeArg {
    Dark,
    Light,
}
impl ThemeArg {
    #[must_use]
    pub fn theme(self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ChromeArg {
    None,
    Macos,
}

#[derive(Debug, Args)]
pub struct CaptureArgs {
    /// Output path or basename. A known extension is replaced for multiple formats.
    #[arg(short = 'o', long, default_value = "ttyshot.png")]
    pub output: PathBuf,
    /// Terminal size as `COLUMNSxROWS`.
    #[arg(long, value_parser=parse_size, default_value="80x24")]
    pub size: Size,
    /// Wait until this exact text appears.
    #[arg(long, value_parser=parse_nonempty_wait)]
    pub wait_text: Option<String>,
    /// Wait until this regular expression matches.
    #[arg(long, value_parser=parse_nonempty_wait)]
    pub wait_regex: Option<String>,
    /// Wait until this exact text is absent.
    #[arg(long, value_parser=parse_nonempty_wait)]
    pub wait_absent: Option<String>,
    /// Wait until the complete grid is unchanged for this duration.
    #[arg(long, value_parser=parse_duration)]
    pub wait_stable: Option<Duration>,
    /// Keys to send after waits, as a comma-separated list.
    #[arg(long, value_delimiter=',', value_parser=parse_key)]
    pub keys: Vec<Key>,
    /// Non-extendable lifetime of the child and all actions.
    #[arg(long, value_parser=parse_duration, default_value="30s")]
    pub timeout: Duration,
    /// Start the child with only ttyshot terminal variables and explicit --env entries.
    #[arg(long)]
    pub env_clear: bool,
    /// Set a child environment entry. Repeat for multiple entries.
    #[arg(long="env", value_parser=parse_env)]
    pub environment: Vec<(String, String)>,
    /// Child working directory.
    #[arg(long)]
    pub cwd: Option<PathBuf>,
    /// Child HOME without changing its working directory.
    #[arg(long)]
    pub home: Option<PathBuf>,
    /// Output formats. Repeat or use a comma-separated list.
    #[arg(long, value_delimiter = ',')]
    pub format: Vec<Format>,
    /// Terminal color palette.
    #[arg(long, value_enum, default_value = "dark")]
    pub theme: ThemeArg,
    /// Equal padding around the cell grid, in logical pixels.
    #[arg(long, default_value_t = 16)]
    pub padding: u16,
    /// Integer raster/vector scale from 1 through 4.
    #[arg(long, value_parser=clap::value_parser!(u8).range(1..=4), default_value_t=1)]
    pub scale: u8,
    /// Optional window decoration.
    #[arg(long, value_enum, default_value = "none")]
    pub chrome: ChromeArg,
    /// Add a deterministic soft shadow.
    #[arg(long)]
    pub shadow: bool,
    /// Disable platform font discovery.
    #[arg(long)]
    pub deterministic_fonts: bool,
    /// Primary font path. Append #INDEX for a collection face.
    #[arg(long)]
    pub font: Option<String>,
    /// Collection face for --font (alternative to #INDEX).
    #[arg(long, requires = "font")]
    pub font_face_index: Option<u32>,
    /// Fallback font path. Repeat; append #INDEX for a collection face.
    #[arg(long)]
    pub fallback_font: Vec<String>,
    /// Collection face paired by position with --fallback-font. Repeat as needed.
    #[arg(long)]
    pub fallback_font_index: Vec<u32>,
    /// Command and arguments. The `--` separator is required.
    #[arg(last = true)]
    pub command: Vec<String>,
}

impl CaptureArgs {
    /// Builds renderer options and validates font/index pairing.
    ///
    /// # Errors
    /// Returns a clear error for invalid or mismatched font face options.
    pub fn render_options(&self) -> Result<RenderOptions, String> {
        if self.fallback_font_index.len() > self.fallback_font.len() {
            return Err("more --fallback-font-index values than --fallback-font values".to_owned());
        }
        let font = self
            .font
            .as_deref()
            .map(|value| crate::parse::parse_font(value, self.font_face_index, None))
            .transpose()?;
        let fallback_fonts = self
            .fallback_font
            .iter()
            .enumerate()
            .map(|(index, value)| {
                crate::parse::parse_font(value, self.fallback_font_index.get(index).copied(), None)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RenderOptions {
            theme: self.theme.theme(),
            padding: Padding::all(self.padding),
            scale: self.scale,
            chrome: match self.chrome {
                ChromeArg::None => WindowChrome::None,
                ChromeArg::Macos => WindowChrome::MacOs,
            },
            shadow: self.shadow,
            font_mode: if self.deterministic_fonts {
                FontMode::Deterministic
            } else {
                FontMode::System
            },
            font,
            fallback_fonts,
            ..RenderOptions::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn parses_capture_surface() {
        let cli = Cli::try_parse_from([
            "ttyshot",
            "-o",
            "x.png",
            "--size",
            "120x40",
            "--wait-stable",
            "300ms",
            "--keys",
            "tab,ctrl-a,界",
            "--format",
            "png,svg",
            "--",
            "app",
            "arg",
        ])
        .unwrap();
        assert!(cli.action.is_none());
        assert_eq!(cli.capture.size, Size::new(120, 40));
        assert_eq!(cli.capture.keys.len(), 3);
        assert_eq!(cli.capture.format, vec![Format::Png, Format::Svg]);
        assert_eq!(cli.capture.command, ["app", "arg"]);
    }
    #[test]
    fn rejects_empty_wait_values() {
        for flag in ["--wait-text", "--wait-regex", "--wait-absent"] {
            let error = Cli::try_parse_from(["ttyshot", flag, "", "--", "app"])
                .unwrap_err()
                .to_string();
            assert!(error.contains("wait value must not be empty"), "{error}");
        }
    }
    #[test]
    fn parses_run_subcommand() {
        let cli = Cli::try_parse_from(["ttyshot", "run", "scene.toml"]).unwrap();
        assert!(matches!(cli.action, Some(Action::Run { .. })));
    }
    #[test]
    fn clap_names_bad_grammar() {
        let error = Cli::try_parse_from(["ttyshot", "--size", "0x1", "--", "x"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid size"));
        let error = Cli::try_parse_from(["ttyshot", "--keys", "ctrl-1", "--", "x"])
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid key"));
    }
}
