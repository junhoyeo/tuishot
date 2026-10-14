//! Headless terminal driver backed by a PTY and Alacritty's VT state machine.
//!
//! [`Session`] owns the child process group. Dropping a session or reaching its
//! hard deadline kills the complete group, including descendants.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty::{self, Shell};
use alacritty_terminal::vte::ansi::{
    Color as AlacrittyColor, CursorShape as AlacrittyCursorShape, NamedColor, Rgb as AlacrittyRgb,
};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use regex::Regex;
use ttyshot_core::{
    Attributes, CellWidth, Color, Cursor, CursorShape, Grid, Rgb, Style, Theme, Underline,
};

pub use ttyshot_core as core;

/// PTY dimensions in character cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Size {
    /// Number of columns.
    pub columns: u16,
    /// Number of rows.
    pub rows: u16,
}

impl Size {
    /// Creates terminal dimensions.
    #[must_use]
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }

    fn validate(self) -> Result<Self, DriverError> {
        if self.columns < 2 || self.rows == 0 {
            return Err(DriverError::Configuration(
                "terminal size must have at least 2 columns and 1 row".to_owned(),
            ));
        }
        Ok(self)
    }

    fn window_size(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.columns,
            cell_width: 1,
            cell_height: 1,
        }
    }
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.columns)
    }
}

/// A command and its arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    program: String,
    args: Vec<String>,
}

impl Command {
    /// Creates a command.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    /// Appends one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Appends arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

/// Deterministic child process environment and working directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Environment {
    inherit: bool,
    overrides: BTreeMap<String, String>,
    cwd: Option<PathBuf>,
    home: Option<PathBuf>,
}

impl Environment {
    /// Starts with the parent environment and applies explicit overrides.
    #[must_use]
    pub fn inherit() -> Self {
        Self {
            inherit: true,
            overrides: BTreeMap::new(),
            cwd: None,
            home: None,
        }
    }

    /// Starts with an empty environment and applies only deterministic terminal
    /// defaults plus explicit overrides.
    #[must_use]
    pub fn clear() -> Self {
        Self {
            inherit: false,
            overrides: BTreeMap::new(),
            cwd: None,
            home: None,
        }
    }

    /// Adds or replaces an environment variable.
    #[must_use]
    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.overrides.insert(key.into(), value.into());
        self
    }

    /// Sets the child working directory.
    #[must_use]
    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    /// Sets the child's `HOME` variable without changing its working directory.
    #[must_use]
    pub fn home(mut self, path: impl Into<PathBuf>) -> Self {
        self.home = Some(path.into());
        self
    }

    fn variables(&self) -> BTreeMap<String, String> {
        let mut variables = BTreeMap::from([
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("TERM_PROGRAM".to_owned(), "ttyshot".to_owned()),
            (
                "TERM_PROGRAM_VERSION".to_owned(),
                env!("CARGO_PKG_VERSION").to_owned(),
            ),
        ]);
        variables.extend(self.overrides.clone());
        if let Some(home) = &self.home {
            variables.insert("HOME".to_owned(), home.to_string_lossy().into_owned());
        }
        variables
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::inherit()
    }
}

/// Builder for a headless terminal session.
#[derive(Clone, Debug)]
pub struct SessionBuilder {
    command: Command,
    size: Size,
    environment: Environment,
    timeout: Duration,
    theme: Theme,
}

impl SessionBuilder {
    /// Creates a builder for `command`.
    #[must_use]
    pub fn new(command: Command) -> Self {
        Self {
            command,
            size: Size::new(80, 24),
            environment: Environment::default(),
            timeout: Duration::from_secs(30),
            theme: Theme::dark(),
        }
    }

    /// Sets the initial PTY and emulator dimensions.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Sets the child environment, working directory, and optional home.
    #[must_use]
    pub fn environment(mut self, environment: Environment) -> Self {
        self.environment = environment;
        self
    }

    /// Sets the non-extendable lifetime of the session.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the theme used to answer dynamic color queries.
    #[must_use]
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Spawns the command in a new PTY session and process group.
    ///
    /// # Errors
    /// Returns an error for invalid configuration, PTY creation, or process
    /// spawning failures.
    pub fn spawn(self) -> Result<Session, DriverError> {
        Session::spawn(self)
    }
}

/// A keyboard action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    /// Enter/return.
    Enter,
    /// Escape.
    Escape,
    /// Horizontal tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Function key F1 through F12.
    Function(u8),
    /// A Unicode scalar value.
    Char(char),
}

/// A running terminal session.
pub struct Session {
    terminal: Arc<FairMutex<Term<DriverEventProxy>>>,
    sender: EventLoopSender,
    control: Arc<Control>,
    process_group: Pid,
    deadline: Instant,
    loop_thread: Option<
        JoinHandle<(
            EventLoop<tty::Pty, DriverEventProxy>,
            alacritty_terminal::event_loop::State,
        )>,
    >,
    watchdog_thread: Option<JoinHandle<()>>,
}

impl Session {
    fn spawn(builder: SessionBuilder) -> Result<Self, DriverError> {
        let size = builder.size.validate()?;
        if builder.command.program.is_empty() {
            return Err(DriverError::Configuration(
                "command program must not be empty".to_owned(),
            ));
        }
        if builder.timeout.is_zero() {
            return Err(DriverError::Configuration(
                "overall timeout must be greater than zero".to_owned(),
            ));
        }
        validate_cwd(builder.environment.cwd.as_deref())?;

        let mut variables = builder.environment.variables();
        let shell = if builder.environment.inherit {
            Shell::new(builder.command.program, builder.command.args)
        } else {
            let mut args = Vec::with_capacity(variables.len() + builder.command.args.len() + 2);
            args.push("-i".to_owned());
            args.extend(
                variables
                    .iter()
                    .map(|(key, value)| format!("{key}={value}")),
            );
            args.push(builder.command.program);
            args.extend(builder.command.args);
            Shell::new("/usr/bin/env".to_owned(), args)
        };

        let options = tty::Options {
            shell: Some(shell),
            working_directory: builder.environment.cwd,
            drain_on_exit: true,
            env: if builder.environment.inherit {
                variables.into_iter().collect::<HashMap<_, _>>()
            } else {
                variables.clear();
                HashMap::new()
            },
        };

        let pty = tty::new(&options, size.window_size(), 0)
            .map_err(|error| DriverError::Spawn(error.to_string()))?;
        let process_group = Pid::from_raw(
            i32::try_from(pty.child().id())
                .map_err(|_| DriverError::Spawn("child PID does not fit i32".to_owned()))?,
        );

        let control = Arc::new(Control::default());
        let sender_slot = Arc::new(Mutex::new(None));
        let event_proxy = DriverEventProxy {
            sender: Arc::clone(&sender_slot),
            control: Arc::clone(&control),
            theme: builder.theme,
        };
        let term_config = TermConfig {
            scrolling_history: 0,
            kitty_keyboard: true,
            ..TermConfig::default()
        };
        let terminal = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            event_proxy.clone(),
        )));
        let event_loop = EventLoop::new(
            Arc::clone(&terminal),
            event_proxy,
            pty,
            options.drain_on_exit,
            false,
        )
        .map_err(|error| DriverError::Spawn(error.to_string()))?;
        let sender = event_loop.channel();
        *sender_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender.clone());
        let loop_thread = event_loop.spawn();

        let deadline = Instant::now()
            .checked_add(builder.timeout)
            .ok_or_else(|| DriverError::Configuration("overall timeout is too large".to_owned()))?;
        let watchdog_control = Arc::clone(&control);
        let watchdog_thread = thread::Builder::new()
            .name("ttyshot timeout".to_owned())
            .spawn(move || watchdog(deadline, process_group, &watchdog_control))
            .map_err(|error| DriverError::Spawn(error.to_string()))?;

        Ok(Self {
            terminal,
            sender,
            control,
            process_group,
            deadline,
            loop_thread: Some(loop_thread),
            watchdog_thread: Some(watchdog_thread),
        })
    }

    /// Sends one key to the child.
    ///
    /// # Errors
    /// Returns an error after the child exits or the session deadline expires.
    pub fn key(&self, key: Key) -> Result<(), DriverError> {
        let app_cursor = self.terminal.lock().mode().contains(TermMode::APP_CURSOR);
        let bytes = key_bytes(key, app_cursor)?;
        self.send(bytes)
    }

    /// Sends UTF-8 text exactly as supplied.
    ///
    /// # Errors
    /// Returns an error after the child exits or the session deadline expires.
    pub fn type_text(&self, text: impl Into<String>) -> Result<(), DriverError> {
        self.send(text.into().into_bytes())
    }

    /// Resizes both the VT state and the kernel PTY.
    ///
    /// # Errors
    /// Returns an error for invalid dimensions or after the child exits.
    pub fn resize(&self, size: Size) -> Result<(), DriverError> {
        let size = size.validate()?;
        self.ensure_active("resize the terminal")?;
        self.terminal.lock().resize(size);
        self.sender
            .send(Msg::Resize(size.window_size()))
            .map_err(|error| DriverError::Io(error.to_string()))?;
        self.control.changed();
        Ok(())
    }

    /// Captures the current screen as the shared cell-grid model.
    #[must_use]
    pub fn snapshot(&self) -> Grid {
        term_to_grid(&self.terminal.lock())
    }

    /// Alias used by scene runners for a screenshot grid action.
    #[must_use]
    pub fn shot_grid(&self) -> Grid {
        self.snapshot()
    }

    /// Waits until the visible screen contains `text`.
    ///
    /// # Errors
    /// Returns a deadline or child-exit error with the last screen.
    pub fn wait_text(&self, text: &str, timeout: Duration) -> Result<Grid, DriverError> {
        self.wait_until(format!("text {text:?}"), timeout, |screen| {
            screen.contains(text)
        })
    }

    /// Waits until the visible screen matches `pattern`.
    ///
    /// # Errors
    /// Returns a regex syntax, deadline, or child-exit error with the last screen.
    pub fn wait_regex(&self, pattern: &str, timeout: Duration) -> Result<Grid, DriverError> {
        let regex = Regex::new(pattern).map_err(|error| DriverError::Regex(error.to_string()))?;
        self.wait_until(format!("regex {pattern:?}"), timeout, |screen| {
            regex.is_match(screen)
        })
    }

    /// Waits until `text` is absent from the visible screen.
    ///
    /// # Errors
    /// Returns a deadline or child-exit error with the last screen.
    pub fn wait_absent(&self, text: &str, timeout: Duration) -> Result<Grid, DriverError> {
        self.wait_until(format!("absence of text {text:?}"), timeout, |screen| {
            !screen.contains(text)
        })
    }

    /// Waits until the complete cell grid stays unchanged for `stable_for`.
    ///
    /// `timeout` is the maximum time spent on this action and is still bounded
    /// by the session's hard overall deadline.
    ///
    /// # Errors
    /// Returns a deadline or child-exit error with the last screen.
    pub fn wait_stable(
        &self,
        stable_for: Duration,
        timeout: Duration,
    ) -> Result<Grid, DriverError> {
        let action_deadline = self.action_deadline(timeout);
        let mut candidate = self.snapshot();
        let mut stable_since = Instant::now();

        loop {
            let generation = self.control.generation();
            let now = Instant::now();
            if now.duration_since(stable_since) >= stable_for {
                return Ok(candidate);
            }
            if now >= action_deadline {
                return Err(Self::timeout_error(
                    format!("screen stability for {stable_for:?}"),
                    &candidate,
                ));
            }

            let wake_at = action_deadline.min(
                stable_since
                    .checked_add(stable_for)
                    .unwrap_or(action_deadline),
            );
            self.control
                .wait_for_change(generation, wake_at, "screen stability", &candidate)?;
            let current = self.snapshot();
            if current != candidate {
                candidate = current;
                stable_since = Instant::now();
            }
        }
    }

    fn wait_until<F>(
        &self,
        awaited: String,
        timeout: Duration,
        predicate: F,
    ) -> Result<Grid, DriverError>
    where
        F: Fn(&str) -> bool,
    {
        let action_deadline = self.action_deadline(timeout);
        loop {
            let generation = self.control.generation();
            let grid = self.snapshot();
            let screen = grid.plain_text();
            if predicate(&screen) {
                return Ok(grid);
            }
            if Instant::now() >= action_deadline {
                return Err(Self::timeout_error(awaited, &grid));
            }
            self.control
                .wait_for_change(generation, action_deadline, &awaited, &grid)?;
        }
    }

    fn action_deadline(&self, timeout: Duration) -> Instant {
        Instant::now()
            .checked_add(timeout)
            .unwrap_or(self.deadline)
            .min(self.deadline)
    }

    fn timeout_error(awaited: String, grid: &Grid) -> DriverError {
        DriverError::Timeout {
            awaited,
            last_screen: grid.plain_text(),
        }
    }

    fn send(&self, bytes: Vec<u8>) -> Result<(), DriverError> {
        self.ensure_active("send terminal input")?;
        self.sender
            .send(Msg::Input(Cow::Owned(bytes)))
            .map_err(|error| DriverError::Io(error.to_string()))
    }

    fn ensure_active(&self, awaited: &str) -> Result<(), DriverError> {
        let state = self
            .control
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(state.process, ProcessState::TimedOut) || Instant::now() >= self.deadline {
            drop(state);
            return Err(Self::timeout_error(awaited.to_owned(), &self.snapshot()));
        }
        if let ProcessState::Exited(status) = state.process {
            drop(state);
            return Err(DriverError::Exited {
                awaited: awaited.to_owned(),
                status,
                last_screen: self.snapshot().plain_text(),
            });
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        kill_process_group(self.process_group);
        self.control.close();
        let _ = self.sender.send(Msg::Shutdown);

        if let Some(thread) = self.watchdog_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.loop_thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Control {
    state: Mutex<ControlState>,
    condvar: Condvar,
}

#[derive(Default)]
struct ControlState {
    generation: u64,
    process: ProcessState,
}

#[derive(Clone, Copy, Default)]
enum ProcessState {
    #[default]
    Running,
    ExitPending(Option<i32>),
    Exited(Option<i32>),
    TimedOut,
    Closed,
}

impl Control {
    fn changed(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.generation = state.generation.wrapping_add(1);
        self.condvar.notify_all();
    }

    // Alacritty emits ChildExit before it drains the final PTY bytes. Deferring
    // the observable exit until the following Wakeup guarantees that waiters
    // can inspect the final screen before receiving an exit error.
    fn child_exited(&self, status: ExitStatus) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.process = ProcessState::ExitPending(status.code());
    }

    fn wakeup(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let ProcessState::ExitPending(status) = state.process {
            state.process = ProcessState::Exited(status);
        }
        state.generation = state.generation.wrapping_add(1);
        self.condvar.notify_all();
    }

    fn generation(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .generation
    }

    fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.process = ProcessState::Closed;
        self.condvar.notify_all();
    }

    fn wait_for_change(
        &self,
        generation: u64,
        deadline: Instant,
        awaited: &str,
        grid: &Grid,
    ) -> Result<(), DriverError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if state.generation != generation {
                return Ok(());
            }
            if matches!(state.process, ProcessState::TimedOut) {
                return Err(DriverError::Timeout {
                    awaited: awaited.to_owned(),
                    last_screen: grid.plain_text(),
                });
            }
            if let ProcessState::Exited(status) = state.process {
                return Err(DriverError::Exited {
                    awaited: awaited.to_owned(),
                    status,
                    last_screen: grid.plain_text(),
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(());
            }
            let (new_state, _) = self
                .condvar
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = new_state;
        }
    }
}

#[derive(Clone)]
struct DriverEventProxy {
    sender: Arc<Mutex<Option<EventLoopSender>>>,
    control: Arc<Control>,
    theme: Theme,
}

impl EventListener for DriverEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_reply(text.into_bytes()),
            Event::ColorRequest(index, format) => {
                let color = query_color(&self.theme, index);
                self.write_reply(format(color).into_bytes());
            }
            Event::Wakeup => self.control.wakeup(),
            Event::ChildExit(status) => self.control.child_exited(status),
            _ => {}
        }
    }
}

impl DriverEventProxy {
    fn write_reply(&self, bytes: Vec<u8>) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(Msg::Input(Cow::Owned(bytes)));
        }
    }
}

fn watchdog(deadline: Instant, process_group: Pid, control: &Control) {
    let mut state = control
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        if matches!(
            state.process,
            ProcessState::Closed | ProcessState::Exited(_)
        ) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            state.process = ProcessState::TimedOut;
            state.generation = state.generation.wrapping_add(1);
            control.condvar.notify_all();
            drop(state);
            kill_process_group(process_group);
            return;
        }
        let (new_state, _) = control
            .condvar
            .wait_timeout(state, deadline.saturating_duration_since(now))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state = new_state;
    }
}

fn kill_process_group(process_group: Pid) {
    let _ = killpg(process_group, Signal::SIGKILL);
}

fn validate_cwd(cwd: Option<&Path>) -> Result<(), DriverError> {
    if let Some(cwd) = cwd
        && !cwd.is_dir()
    {
        return Err(DriverError::Configuration(format!(
            "working directory {} does not exist or is not a directory",
            cwd.display()
        )));
    }
    Ok(())
}

fn query_color(theme: &Theme, index: usize) -> AlacrittyRgb {
    let color = match index {
        0..=15 => theme.palette[index],
        257 => theme.background,
        258 => theme.cursor,
        _ => theme.foreground,
    };
    AlacrittyRgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

fn key_bytes(key: Key, app_cursor: bool) -> Result<Vec<u8>, DriverError> {
    let bytes = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Backspace => b"\x7f".to_vec(),
        Key::Up => cursor_key(app_cursor, b'A'),
        Key::Down => cursor_key(app_cursor, b'B'),
        Key::Right => cursor_key(app_cursor, b'C'),
        Key::Left => cursor_key(app_cursor, b'D'),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Function(number @ 1..=4) => vec![0x1b, b'O', b'P' + number - 1],
        Key::Function(number @ 5..=12) => {
            const CODES: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            format!("\x1b[{}~", CODES[usize::from(number - 5)]).into_bytes()
        }
        Key::Function(number) => {
            return Err(DriverError::Configuration(format!(
                "function key F{number} is outside F1 through F12"
            )));
        }
        Key::Char(character) => character.to_string().into_bytes(),
    };
    Ok(bytes)
}

fn cursor_key(app_cursor: bool, final_byte: u8) -> Vec<u8> {
    vec![0x1b, if app_cursor { b'O' } else { b'[' }, final_byte]
}

fn term_to_grid(term: &Term<DriverEventProxy>) -> Grid {
    let content = term.renderable_content();
    let cursor = content.cursor;
    let display_offset = content.display_offset;
    let mut grid = Grid::new(
        u16::try_from(term.columns()).expect("terminal columns fit u16"),
        u16::try_from(term.screen_lines()).expect("terminal rows fit u16"),
    )
    .expect("validated terminal dimensions produce a grid");

    for indexed in content.display_iter {
        let y = indexed.point.line.0 + i32::try_from(display_offset).unwrap_or(i32::MAX);
        let Ok(y) = u16::try_from(y) else {
            continue;
        };
        let Ok(x) = u16::try_from(indexed.point.column.0) else {
            continue;
        };
        if indexed.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let mut symbol = indexed.c.to_string();
        if let Some(zerowidth) = indexed.zerowidth() {
            symbol.extend(zerowidth);
        }
        let width = if indexed.flags.contains(Flags::WIDE_CHAR) {
            CellWidth::Wide
        } else {
            CellWidth::Single
        };
        let style = cell_style(&indexed);
        if grid.put(x, y, symbol, width, style).is_err() {
            let _ = grid.put(x, y, " ", CellWidth::Single, style);
        }
    }

    let cursor_y = cursor.point.line.0 + i32::try_from(display_offset).unwrap_or(i32::MAX);
    if let (Ok(x), Ok(y)) = (
        u16::try_from(cursor.point.column.0),
        u16::try_from(cursor_y),
    ) && x < grid.columns()
        && y < grid.rows()
    {
        let (shape, visible) = match cursor.shape {
            AlacrittyCursorShape::Block | AlacrittyCursorShape::HollowBlock => {
                (CursorShape::Block, true)
            }
            AlacrittyCursorShape::Underline => (CursorShape::Underline, true),
            AlacrittyCursorShape::Beam => (CursorShape::Bar, true),
            AlacrittyCursorShape::Hidden => (CursorShape::Block, false),
        };
        grid.cursor = Some(Cursor {
            x,
            y,
            shape,
            visible,
        });
    }
    grid
}

fn cell_style(cell: &alacritty_terminal::term::cell::Cell) -> Style {
    let flags = cell.flags;
    let underline = if flags.contains(Flags::DOUBLE_UNDERLINE) {
        Underline::Double
    } else if flags.contains(Flags::UNDERCURL) {
        Underline::Curly
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        Underline::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        Underline::Dashed
    } else if flags.contains(Flags::UNDERLINE) {
        Underline::Single
    } else {
        Underline::None
    };
    Style {
        foreground: convert_color(cell.fg),
        background: convert_color(cell.bg),
        underline_color: cell.underline_color().map_or(Color::Default, convert_color),
        attributes: Attributes {
            bold: flags.contains(Flags::BOLD),
            dim: flags.contains(Flags::DIM),
            italic: flags.contains(Flags::ITALIC),
            underline,
            reverse: flags.contains(Flags::INVERSE),
            hidden: flags.contains(Flags::HIDDEN),
            strikethrough: flags.contains(Flags::STRIKEOUT),
            ..Attributes::default()
        },
    }
}

fn convert_color(color: AlacrittyColor) -> Color {
    match color {
        AlacrittyColor::Spec(color) => Color::Rgb(Rgb::new(color.r, color.g, color.b)),
        AlacrittyColor::Indexed(index) => Color::Indexed(index),
        AlacrittyColor::Named(
            named @ (NamedColor::Black
            | NamedColor::Red
            | NamedColor::Green
            | NamedColor::Yellow
            | NamedColor::Blue
            | NamedColor::Magenta
            | NamedColor::Cyan
            | NamedColor::White
            | NamedColor::BrightBlack
            | NamedColor::BrightRed
            | NamedColor::BrightGreen
            | NamedColor::BrightYellow
            | NamedColor::BrightBlue
            | NamedColor::BrightMagenta
            | NamedColor::BrightCyan
            | NamedColor::BrightWhite),
        ) => Color::Indexed(named_color_index(named)),
        AlacrittyColor::Named(_) => Color::Default,
    }
}

fn named_color_index(color: NamedColor) -> u8 {
    match color {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        _ => unreachable!("only ANSI named colors are converted to palette indices"),
    }
}

/// Terminal driver failure.
#[derive(Debug)]
pub enum DriverError {
    /// Invalid builder or action input.
    Configuration(String),
    /// PTY or child process setup failed.
    Spawn(String),
    /// Runtime terminal I/O failed.
    Io(String),
    /// A regular expression was invalid.
    Regex(String),
    /// An explicit action or overall deadline expired.
    Timeout {
        /// Description of the condition being awaited.
        awaited: String,
        /// Complete visible screen at failure time.
        last_screen: String,
    },
    /// The child exited before the awaited condition became true.
    Exited {
        /// Description of the condition being awaited.
        awaited: String,
        /// Platform exit code, when one was available.
        status: Option<i32>,
        /// Complete visible screen at failure time.
        last_screen: String,
    },
}

impl fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => {
                write!(formatter, "invalid terminal configuration: {message}")
            }
            Self::Spawn(message) => write!(formatter, "failed to spawn terminal child: {message}"),
            Self::Io(message) => write!(formatter, "terminal I/O failed: {message}"),
            Self::Regex(message) => write!(formatter, "invalid wait regex: {message}"),
            Self::Timeout {
                awaited,
                last_screen,
            } => write!(
                formatter,
                "timed out while waiting for {awaited}\nlast screen:\n{last_screen}"
            ),
            Self::Exited {
                awaited,
                status,
                last_screen,
            } => write!(
                formatter,
                "child exited with status {status:?} while waiting for {awaited}\nlast screen:\n{last_screen}"
            ),
        }
    }
}

impl Error for DriverError {}
