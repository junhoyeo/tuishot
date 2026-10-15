//! Command-line parsing and strict scene execution for `ttyshot`.

pub mod cli;
pub mod output;
pub mod panel;
pub mod parse;
pub mod scene;

use cli::{Action, CaptureArgs, Cli};
use output::{resolve_targets, write_outputs};
use std::{error::Error as StdError, fmt, time::Duration};
use ttyshot_term::{Command, DriverError, Environment, SessionBuilder};

#[derive(Debug)]
pub struct Error {
    message: String,
}
impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl StdError for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::msg(value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::msg(value.to_string())
    }
}
impl From<ttyshot_render::RenderError> for Error {
    fn from(value: ttyshot_render::RenderError) -> Self {
        Self::msg(value.to_string())
    }
}
impl From<ttyshot_core::GridError> for Error {
    fn from(value: ttyshot_core::GridError) -> Self {
        Self::msg(value.to_string())
    }
}
impl From<DriverError> for Error {
    fn from(value: DriverError) -> Self {
        Self::msg(value.to_string())
    }
}

/// Executes a parsed one-shot capture or scene-file command.
///
/// # Errors
/// Returns an error for invalid options, terminal actions, rendering, or output writes.
pub fn execute(cli: Cli) -> Result<Vec<std::path::PathBuf>, Error> {
    match cli.action {
        Some(Action::Run { scene_file }) => {
            if !cli.capture.command.is_empty() {
                return Err(Error::msg(
                    "a command cannot be supplied with `ttyshot run`",
                ));
            }
            let file = scene::load(&scene_file)?;
            scene::run(&file)
        }
        None => capture(&cli.capture),
    }
}

fn validate_wait_inputs(
    wait_text: Option<&str>,
    wait_regex: Option<&str>,
    wait_absent: Option<&str>,
) -> Result<(), Error> {
    if wait_text == Some("") || wait_regex == Some("") || wait_absent == Some("") {
        Err(Error::msg("wait value must not be empty"))
    } else {
        Ok(())
    }
}

/// Runs one command, executes waits and keys, and writes a single captured grid.
///
/// # Errors
/// Returns an error for invalid input, terminal actions, rendering, or output writes.
pub fn capture(args: &CaptureArgs) -> Result<Vec<std::path::PathBuf>, Error> {
    if args.command.is_empty() {
        return Err(Error::msg(
            "missing command; use `ttyshot [OPTIONS] -- <COMMAND> [ARGS]...` or `ttyshot run <FILE>`",
        ));
    }
    if args.command.iter().any(|value| value.contains('\0')) {
        return Err(Error::msg("command and arguments must not contain NUL"));
    }
    validate_wait_inputs(
        args.wait_text.as_deref(),
        args.wait_regex.as_deref(),
        args.wait_absent.as_deref(),
    )?;
    if let Some(pattern) = &args.wait_regex {
        regex::Regex::new(pattern)
            .map_err(|error| Error::msg(format!("invalid wait regex {pattern:?}: {error}")))?;
    }
    let mut environment = if args.env_clear {
        Environment::clear()
    } else {
        Environment::inherit()
    };
    for (key, value) in &args.environment {
        environment = environment.var(key, value);
    }
    if let Some(cwd) = &args.cwd {
        environment = environment.cwd(cwd);
    }
    if let Some(home) = &args.home {
        environment = environment.home(home);
    }
    let command = Command::new(&args.command[0]).args(args.command.iter().skip(1).cloned());
    let options = args.render_options().map_err(Error::msg)?;
    let session = SessionBuilder::new(command)
        .size(args.size)
        .timeout(args.timeout)
        .environment(environment)
        .theme(options.theme.clone())
        .spawn()?;
    let mut waited = false;
    if let Some(value) = &args.wait_text {
        session.wait_text(value, args.timeout)?;
        waited = true;
    }
    if let Some(value) = &args.wait_regex {
        session.wait_regex(value, args.timeout)?;
        waited = true;
    }
    if let Some(value) = &args.wait_absent {
        session.wait_absent(value, args.timeout)?;
        waited = true;
    }
    if let Some(value) = args.wait_stable {
        session.wait_stable(value, args.timeout)?;
        waited = true;
    }
    // The zero-configuration capture must not accept an initially blank PTY as
    // a stable frame while a slower TUI is still starting. Explicit waits take
    // precedence; otherwise wait for visible text, then for event-driven
    // stability. Color-only interfaces can select an explicit wait condition.
    if !waited {
        session.wait_regex(r"\S", args.timeout)?;
    }
    for &key in &args.keys {
        session.key(key)?;
    }
    if !args.keys.is_empty() || !waited {
        session.wait_stable(Duration::from_millis(100), args.timeout)?;
    }
    let grid = session.shot_grid();
    let targets = resolve_targets(&args.output, &args.format)?;
    write_outputs(&grid, &targets, &options)?;
    Ok(targets.into_iter().map(|target| target.path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_validation_rejects_empty_wait_values() {
        assert!(validate_wait_inputs(Some(""), None, None).is_err());
        assert!(validate_wait_inputs(None, Some(""), None).is_err());
        assert!(validate_wait_inputs(None, None, Some("")).is_err());
        assert!(validate_wait_inputs(Some("ready"), None, None).is_ok());
    }
}
