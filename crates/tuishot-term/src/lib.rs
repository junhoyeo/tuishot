//! Headless terminal driver backed by a PTY and Alacritty's VT state machine.
//!
//! [`Session`] owns the child process group. Dropping a session or reaching its
//! hard deadline kills the complete group, including descendants.
//!
//! Alacritty 0.26 does not preserve SGR blink (5/6), frame/encircle (51/52),
//! overline (53), or ideogram decoration (60-64) in its public cell state.
//! Captures therefore leave those shared-grid attributes at their defaults.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
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
use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{Pid, getpgid};
use regex::Regex;
use tuishot_core::{
    Attributes, Blink, CellWidth, Color, ColorRole, Cursor, CursorShape, Grid, Rgb, Style, Theme,
    Underline,
};

pub use tuishot_core as core;

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
        if self.columns == 0 || self.rows == 0 {
            return Err(DriverError::Configuration(
                "terminal size must have at least 1 column and 1 row".to_owned(),
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
            ("TERM_PROGRAM".to_owned(), "tuishot".to_owned()),
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
    cleanup: Arc<ProcessCleanup>,
    deadline: Instant,
    loop_thread: Option<JoinHandle<()>>,
    query_sender: Sender<QueryMessage>,
    query_thread: Option<JoinHandle<()>>,
    watchdog_thread: Option<JoinHandle<()>>,
}

impl Session {
    fn spawn(builder: SessionBuilder) -> Result<Self, DriverError> {
        let size = builder.size.validate()?;
        let deadline = Instant::now()
            .checked_add(builder.timeout)
            .ok_or_else(|| DriverError::Configuration("overall timeout is too large".to_owned()))?;
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
        let options = tty_options(builder.command, builder.environment);

        let pty = tty::new(&options, size.window_size(), 0)
            .map_err(|error| DriverError::Spawn(error.to_string()))?;
        let child_pid = Pid::from_raw(
            i32::try_from(pty.child().id())
                .map_err(|_| DriverError::Spawn("child PID does not fit i32".to_owned()))?,
        );
        let cleanup = Arc::new(ProcessCleanup::new(child_pid));
        wait_for_process_group(&cleanup, deadline)?;

        let control = Arc::new(Control::default());
        let sender_slot = Arc::new(Mutex::new(None));
        let (query_sender, query_receiver) = mpsc::channel();
        let event_proxy = DriverEventProxy {
            sender: Arc::clone(&sender_slot),
            control: Arc::clone(&control),
            query_sender: query_sender.clone(),
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
        let query_terminal = Arc::clone(&terminal);
        let query_sender_slot = Arc::clone(&sender_slot);
        let query_theme = builder.theme;
        let query_thread = thread::Builder::new()
            .name("tuishot color queries".to_owned())
            .spawn(move || {
                color_query_loop(
                    &query_receiver,
                    &query_terminal,
                    &query_sender_slot,
                    &query_theme,
                );
            })
            .map_err(|error| DriverError::Spawn(error.to_string()))?;
        let pty_thread = event_loop.spawn();
        let loop_control = Arc::clone(&control);
        let loop_thread = thread::Builder::new()
            .name("tuishot PTY completion".to_owned())
            .spawn(move || {
                let _ = pty_thread.join();
                loop_control.finish_exit();
            })
            .map_err(|error| DriverError::Spawn(error.to_string()))?;

        let watchdog_control = Arc::clone(&control);
        let watchdog_cleanup = Arc::clone(&cleanup);
        let watchdog_thread = thread::Builder::new()
            .name("tuishot timeout".to_owned())
            .spawn(move || watchdog(deadline, &watchdog_cleanup, &watchdog_control))
            .map_err(|error| DriverError::Spawn(error.to_string()))?;

        Ok(Self {
            terminal,
            sender,
            control,
            cleanup,
            deadline,
            loop_thread: Some(loop_thread),
            query_sender,
            query_thread: Some(query_thread),
            watchdog_thread: Some(watchdog_thread),
        })
    }

    /// Sends one unmodified key press to the child.
    ///
    /// Legacy mode honors application-cursor mode. When the child enables the
    /// kitty keyboard protocol, this method emits Kitty's modifier-1 key-press
    /// forms and honors report-all-keys and associated-text flags.
    ///
    /// # Errors
    /// Returns an error after the child exits or the session deadline expires.
    pub fn key(&self, key: Key) -> Result<(), DriverError> {
        let mode = *self.terminal.lock().mode();
        let bytes = key_bytes(key, mode)?;
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
        let awaited = format!("screen stability for {stable_for:?}");
        let mut generation = self.control.generation();
        let mut candidate = self.snapshot();
        let mut stable_since = Instant::now();

        loop {
            let now = Instant::now();
            if now >= action_deadline {
                return Err(self.timeout_error_fresh(awaited));
            }
            if now.duration_since(stable_since) >= stable_for {
                match self.control.guard_success(generation, action_deadline) {
                    SuccessGuard::Ready => return Ok(candidate),
                    SuccessGuard::TimedOut => {
                        return Err(self.timeout_error_fresh(awaited));
                    }
                    SuccessGuard::Changed => {
                        generation = self.control.generation();
                        let current = self.snapshot();
                        if current != candidate {
                            candidate = current;
                            stable_since = Instant::now();
                        }
                        continue;
                    }
                }
            }

            let wake_at = action_deadline.min(
                stable_since
                    .checked_add(stable_for)
                    .unwrap_or(action_deadline),
            );
            match self.control.wait_for_change(generation, wake_at) {
                ControlWake::Changed => {
                    generation = self.control.generation();
                    let current = self.snapshot();
                    if current != candidate {
                        candidate = current;
                        stable_since = Instant::now();
                    }
                }
                ControlWake::DeadlineReached => {}
                ControlWake::TimedOut => return Err(self.timeout_error_fresh(awaited)),
                ControlWake::Exited(status) => {
                    return Err(self.exit_error_fresh(awaited, status));
                }
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
                match self.control.guard_success(generation, action_deadline) {
                    SuccessGuard::Ready => return Ok(grid),
                    SuccessGuard::Changed => continue,
                    SuccessGuard::TimedOut => {
                        return Err(self.timeout_error_fresh(awaited));
                    }
                }
            }
            match self.control.wait_for_change(generation, action_deadline) {
                ControlWake::Changed => {}
                ControlWake::DeadlineReached | ControlWake::TimedOut => {
                    return Err(self.timeout_error_fresh(awaited));
                }
                ControlWake::Exited(status) => {
                    return Err(self.exit_error_fresh(awaited, status));
                }
            }
        }
    }

    fn action_deadline(&self, timeout: Duration) -> Instant {
        Instant::now()
            .checked_add(timeout)
            .unwrap_or(self.deadline)
            .min(self.deadline)
    }

    fn timeout_error_fresh(&self, awaited: String) -> DriverError {
        DriverError::Timeout {
            awaited,
            last_screen: self.snapshot().plain_text(),
        }
    }

    fn exit_error_fresh(&self, awaited: String, status: Option<i32>) -> DriverError {
        DriverError::Exited {
            awaited,
            status,
            last_screen: self.snapshot().plain_text(),
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
            return Err(self.timeout_error_fresh(awaited.to_owned()));
        }
        if let ProcessState::Exited(status) = state.process {
            drop(state);
            return Err(self.exit_error_fresh(awaited.to_owned(), status));
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cleanup.kill();
        self.control.close();
        let _ = self.query_sender.send(QueryMessage::Shutdown);
        let _ = self.sender.send(Msg::Shutdown);

        if let Some(thread) = self.watchdog_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.loop_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.query_thread.take() {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SuccessGuard {
    Ready,
    Changed,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlWake {
    Changed,
    DeadlineReached,
    TimedOut,
    Exited(Option<i32>),
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
        if matches!(
            state.process,
            ProcessState::Running | ProcessState::ExitPending(_)
        ) {
            state.process = ProcessState::ExitPending(status.code());
        }
    }

    fn finish_exit(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let ProcessState::ExitPending(status) = state.process {
            state.process = ProcessState::Exited(status);
            state.generation = state.generation.wrapping_add(1);
            self.condvar.notify_all();
        }
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

    fn guard_success(&self, generation: u64, deadline: Instant) -> SuccessGuard {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(state.process, ProcessState::TimedOut) || Instant::now() >= deadline {
            SuccessGuard::TimedOut
        } else if state.generation != generation {
            SuccessGuard::Changed
        } else {
            SuccessGuard::Ready
        }
    }

    fn wait_for_change(&self, generation: u64, deadline: Instant) -> ControlWake {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if matches!(state.process, ProcessState::TimedOut) {
                return ControlWake::TimedOut;
            }
            let now = Instant::now();
            if now >= deadline {
                return ControlWake::DeadlineReached;
            }
            if state.generation != generation {
                return ControlWake::Changed;
            }
            if let ProcessState::Exited(status) = state.process {
                return ControlWake::Exited(status);
            }
            if matches!(state.process, ProcessState::Closed) {
                return ControlWake::Exited(None);
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
    query_sender: Sender<QueryMessage>,
}

impl EventListener for DriverEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_reply(text.into_bytes()),
            Event::ColorRequest(index, format) => {
                let _ = self
                    .query_sender
                    .send(QueryMessage::Color { index, format });
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

type ColorFormatter = Arc<dyn Fn(AlacrittyRgb) -> String + Send + Sync + 'static>;

enum QueryMessage {
    Color {
        index: usize,
        format: ColorFormatter,
    },
    Shutdown,
}

fn color_query_loop(
    receiver: &Receiver<QueryMessage>,
    terminal: &FairMutex<Term<DriverEventProxy>>,
    sender_slot: &Mutex<Option<EventLoopSender>>,
    theme: &Theme,
) {
    while let Ok(message) = receiver.recv() {
        let QueryMessage::Color { index, format } = message else {
            return;
        };
        let active = terminal.lock().colors()[index];
        let color = query_color(theme, index, active);
        let sender = sender_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(Msg::Input(Cow::Owned(format(color).into_bytes())));
        }
    }
}

struct ProcessCleanup {
    child_pid: Pid,
    group_ready: AtomicBool,
    killed: AtomicBool,
}

impl ProcessCleanup {
    fn new(child_pid: Pid) -> Self {
        Self {
            child_pid,
            group_ready: AtomicBool::new(false),
            killed: AtomicBool::new(false),
        }
    }

    fn mark_group_ready(&self) {
        self.group_ready.store(true, Ordering::Release);
    }

    fn kill(&self) {
        if self.killed.swap(true, Ordering::AcqRel) {
            return;
        }

        let group_result = killpg(self.child_pid, Signal::SIGKILL);
        if !self.group_ready.load(Ordering::Acquire) && group_result == Err(Errno::ESRCH) {
            let _ = kill(self.child_pid, Signal::SIGKILL);
            // The child can win the setsid race between the first group probe
            // and the direct kill. Retry the group after SIGKILL is pending.
            let _ = killpg(self.child_pid, Signal::SIGKILL);
        }
    }
}

impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        self.kill();
    }
}

fn wait_for_process_group(cleanup: &ProcessCleanup, deadline: Instant) -> Result<(), DriverError> {
    let mut group_ready = false;
    loop {
        if Instant::now() >= deadline {
            cleanup.kill();
            return Err(DriverError::Spawn(
                "session deadline expired before the child process group was ready".to_owned(),
            ));
        }

        if !group_ready {
            match getpgid(Some(cleanup.child_pid)) {
                Ok(process_group) if process_group == cleanup.child_pid => {
                    cleanup.mark_group_ready();
                    group_ready = true;
                }
                Ok(_) => {
                    thread::yield_now();
                    continue;
                }
                Err(Errno::ESRCH) => {
                    cleanup.kill();
                    return Err(DriverError::Spawn(
                        "child exited before its process group was ready".to_owned(),
                    ));
                }
                Err(error) => {
                    cleanup.kill();
                    return Err(DriverError::Spawn(format!(
                        "failed to inspect child process group: {error}"
                    )));
                }
            }
        }

        match waitpid(
            cleanup.child_pid,
            Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED),
        ) {
            Ok(WaitStatus::Stopped(_, Signal::SIGSTOP)) => {
                if Instant::now() >= deadline {
                    cleanup.kill();
                    return Err(DriverError::Spawn(
                        "session deadline expired before the child process group was ready"
                            .to_owned(),
                    ));
                }
                if let Err(error) = kill(cleanup.child_pid, Signal::SIGCONT) {
                    cleanup.kill();
                    return Err(DriverError::Spawn(format!(
                        "failed to release child process group startup barrier: {error}"
                    )));
                }
                return Ok(());
            }
            Ok(WaitStatus::StillAlive) | Err(Errno::EINTR) => thread::yield_now(),
            Ok(status) => {
                cleanup.kill();
                return Err(DriverError::Spawn(format!(
                    "child changed state before its process group was ready: {status:?}"
                )));
            }
            Err(error) => {
                cleanup.kill();
                return Err(DriverError::Spawn(format!(
                    "failed to wait for child process group readiness: {error}"
                )));
            }
        }
    }
}

fn watchdog(deadline: Instant, cleanup: &ProcessCleanup, control: &Control) {
    let mut state = control
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        if matches!(state.process, ProcessState::Closed) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            state.process = ProcessState::TimedOut;
            state.generation = state.generation.wrapping_add(1);
            control.condvar.notify_all();
            drop(state);
            cleanup.kill();
            return;
        }
        let (new_state, _) = control
            .condvar
            .wait_timeout(state, deadline.saturating_duration_since(now))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state = new_state;
    }
}

fn tty_options(command: Command, environment: Environment) -> tty::Options {
    let variables = environment.variables();
    let (program, args, env) = if environment.inherit {
        (
            command.program,
            command.args,
            variables.into_iter().collect::<HashMap<_, _>>(),
        )
    } else {
        let mut args = Vec::with_capacity(variables.len() + command.args.len() + 2);
        args.push("-i".to_owned());
        args.extend(
            variables
                .iter()
                .map(|(key, value)| format!("{key}={value}")),
        );
        args.push(command.program);
        args.extend(command.args);
        ("/usr/bin/env".to_owned(), args, HashMap::new())
    };

    // Alacritty calls setsid before executing this shell. Stop at that boundary
    // so the parent can observe PGID readiness before the target can exit.
    let mut barrier_args = Vec::with_capacity(args.len() + 4);
    barrier_args.extend([
        "-c".to_owned(),
        r#"kill -STOP $$; exec "$@""#.to_owned(),
        "tuishot-session".to_owned(),
        program,
    ]);
    barrier_args.extend(args);

    tty::Options {
        shell: Some(Shell::new("/bin/sh".to_owned(), barrier_args)),
        working_directory: environment.cwd,
        drain_on_exit: true,
        env,
    }
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

fn query_color(theme: &Theme, index: usize, active: Option<AlacrittyRgb>) -> AlacrittyRgb {
    if let Some(active) = active {
        return active;
    }
    let color = match index {
        0..=255 => theme.resolve(
            Color::Indexed(u8::try_from(index).expect("palette index is at most 255")),
            ColorRole::Foreground,
        ),
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

fn key_bytes(key: Key, mode: TermMode) -> Result<Vec<u8>, DriverError> {
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
        return kitty_key_bytes(key, mode);
    }

    let app_cursor = mode.contains(TermMode::APP_CURSOR);
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
            format!("\x1b[{}~", function_key_code(number)?).into_bytes()
        }
        Key::Function(number) => return Err(invalid_function_key(number)),
        Key::Char(character) => character.to_string().into_bytes(),
    };
    Ok(bytes)
}

// tuishot currently exposes unmodified key presses only. Kitty mode therefore
// uses modifier parameter 1 and no event suffix. Literal text remains literal
// unless REPORT_ALL_KEYS_AS_ESC is active; functional keys use Kitty's enhanced
// legacy forms. REPORT_ASSOCIATED_TEXT adds the generated scalar for characters.
fn kitty_key_bytes(key: Key, mode: TermMode) -> Result<Vec<u8>, DriverError> {
    let report_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let associated_text = mode.contains(TermMode::REPORT_ASSOCIATED_TEXT);
    let sequence = match key {
        Key::Char(character) if !report_all => return Ok(character.to_string().into_bytes()),
        Key::Char(character) => {
            let text = if associated_text {
                format!(";{}", u32::from(character))
            } else {
                String::new()
            };
            format!("\x1b[{};1{text}u", u32::from(character))
        }
        Key::Enter => "\x1b[13;1u".to_owned(),
        Key::Escape => "\x1b[27;1u".to_owned(),
        Key::Tab => "\x1b[9;1u".to_owned(),
        Key::Backspace => "\x1b[127;1u".to_owned(),
        Key::Up => "\x1b[1;1A".to_owned(),
        Key::Down => "\x1b[1;1B".to_owned(),
        Key::Right => "\x1b[1;1C".to_owned(),
        Key::Left => "\x1b[1;1D".to_owned(),
        Key::Home => "\x1b[1;1H".to_owned(),
        Key::End => "\x1b[1;1F".to_owned(),
        Key::PageUp => "\x1b[5;1~".to_owned(),
        Key::PageDown => "\x1b[6;1~".to_owned(),
        Key::Insert => "\x1b[2;1~".to_owned(),
        Key::Delete => "\x1b[3;1~".to_owned(),
        Key::Function(number @ 1..=4) => {
            format!("\x1b[1;1{}", char::from(b'P' + number - 1))
        }
        Key::Function(number @ 5..=12) => {
            format!("\x1b[{};1~", function_key_code(number)?)
        }
        Key::Function(number) => return Err(invalid_function_key(number)),
    };
    Ok(sequence.into_bytes())
}

fn function_key_code(number: u8) -> Result<u8, DriverError> {
    const CODES: [u8; 12] = [11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24];
    CODES
        .get(usize::from(number.saturating_sub(1)))
        .copied()
        .ok_or_else(|| invalid_function_key(number))
}

fn invalid_function_key(number: u8) -> DriverError {
    DriverError::Configuration(format!("function key F{number} is outside F1 through F12"))
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
            blink: Blink::None,
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn child_exit_is_finalized_by_wakeup() {
        let control = Control::default();
        control.child_exited(ExitStatus::from_raw(0));
        assert!(matches!(
            control
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .process,
            ProcessState::ExitPending(Some(0))
        ));

        control.wakeup();

        assert!(matches!(
            control
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .process,
            ProcessState::Exited(Some(0))
        ));
    }

    #[test]
    fn event_loop_completion_finalizes_a_pending_exit_without_wakeup() {
        let control = Control::default();
        control.child_exited(ExitStatus::from_raw(0));

        control.finish_exit();

        assert!(matches!(
            control
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .process,
            ProcessState::Exited(Some(0))
        ));
    }

    #[test]
    fn hard_timeout_cause_survives_child_exit_events() {
        let control = Control::default();
        control
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process = ProcessState::TimedOut;

        control.child_exited(ExitStatus::from_raw(9));
        control.wakeup();
        control.finish_exit();

        assert!(matches!(
            control
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .process,
            ProcessState::TimedOut
        ));
    }

    #[test]
    fn kitty_function_keys_are_byte_exact_in_all_relevant_modes() {
        let expected: [&[u8]; 12] = [
            b"\x1b[1;1P",
            b"\x1b[1;1Q",
            b"\x1b[1;1R",
            b"\x1b[1;1S",
            b"\x1b[15;1~",
            b"\x1b[17;1~",
            b"\x1b[18;1~",
            b"\x1b[19;1~",
            b"\x1b[20;1~",
            b"\x1b[21;1~",
            b"\x1b[23;1~",
            b"\x1b[24;1~",
        ];
        let kitty = TermMode::KITTY_KEYBOARD_PROTOCOL;
        let modes = [
            kitty,
            kitty | TermMode::REPORT_ALL_KEYS_AS_ESC,
            kitty | TermMode::REPORT_ASSOCIATED_TEXT,
            kitty | TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_ASSOCIATED_TEXT,
        ];

        for mode in modes {
            for (index, expected_bytes) in expected.iter().enumerate() {
                let number = u8::try_from(index + 1).unwrap();
                assert_eq!(
                    key_bytes(Key::Function(number), mode).unwrap(),
                    *expected_bytes,
                    "F{number} in mode {mode:?}"
                );
            }
        }
    }

    #[test]
    fn pre_group_cleanup_falls_back_to_the_known_child_pid_and_is_idempotent() {
        use std::process::Command as StdCommand;

        let mut child = StdCommand::new("/bin/sh")
            .args(["-c", "trap '' HUP; exec sleep 30"])
            .spawn()
            .unwrap();
        let child_pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
        assert_ne!(getpgid(Some(child_pid)).unwrap(), child_pid);
        let cleanup = ProcessCleanup::new(child_pid);

        cleanup.kill();
        cleanup.kill();
        let status = child.wait().unwrap();

        assert!(!status.success());
        assert_eq!(kill(child_pid, None), Err(Errno::ESRCH));
    }

    #[test]
    fn cleanup_drop_kills_a_pre_group_child() {
        use std::process::Command as StdCommand;

        let mut child = StdCommand::new("/bin/sh")
            .args(["-c", "trap '' HUP; exec sleep 30"])
            .spawn()
            .unwrap();
        let child_pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
        assert_ne!(getpgid(Some(child_pid)).unwrap(), child_pid);

        drop(ProcessCleanup::new(child_pid));
        let status = child.wait().unwrap();

        assert!(!status.success());
        assert_eq!(kill(child_pid, None), Err(Errno::ESRCH));
    }

    #[test]
    fn immediate_drop_stress_leaves_no_process_group() {
        for _ in 0..16 {
            let session =
                shell_test_session("trap '' HUP; (trap '' HUP; sleep 30) & child=$!; wait $child");
            let child_pid = session.cleanup.child_pid;

            drop(session);

            assert_process_and_group_gone(child_pid);
        }
    }

    #[test]
    fn tiny_hard_timeout_stress_leaves_no_process_group() {
        // Leave enough startup budget for loaded CI hosts while keeping this a
        // short-deadline race/cleanup stress test.
        for _ in 0..8 {
            let session = shell_test_session_with_timeout(
                "trap '' HUP; (trap '' HUP; sleep 30) & child=$!; printf READY; wait $child",
                Duration::from_millis(500),
            );
            let child_pid = session.cleanup.child_pid;
            session.wait_text("READY", Duration::from_secs(1)).unwrap();

            let error = session
                .wait_text("NEVER", Duration::from_secs(1))
                .unwrap_err();
            assert!(matches!(error, DriverError::Timeout { .. }), "{error}");
            drop(session);

            assert_process_and_group_gone(child_pid);
        }
    }

    #[test]
    fn elapsed_deadline_beats_a_generation_change() {
        let control = Control::default();
        let generation = control.generation();
        control.changed();

        assert_eq!(
            control.wait_for_change(generation, Instant::now()),
            ControlWake::DeadlineReached
        );
    }

    #[test]
    fn timed_out_state_beats_a_generation_change() {
        let control = Control::default();
        let generation = control.generation();
        {
            let mut state = control
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.process = ProcessState::TimedOut;
            state.generation = state.generation.wrapping_add(1);
        }

        assert_eq!(
            control.wait_for_change(generation, Instant::now() + Duration::from_secs(1)),
            ControlWake::TimedOut
        );
    }

    #[test]
    fn already_matching_screen_cannot_succeed_after_expiry() {
        let session = shell_test_session("printf MATCH; dd bs=1 count=1 of=/dev/null 2>/dev/null");
        session.wait_text("MATCH", Duration::from_secs(1)).unwrap();

        let error = session.wait_text("MATCH", Duration::ZERO).unwrap_err();
        assert!(matches!(error, DriverError::Timeout { .. }), "{error}");
    }

    #[test]
    fn stable_screen_cannot_succeed_after_expiry() {
        let session = shell_test_session("printf STABLE; dd bs=1 count=1 of=/dev/null 2>/dev/null");
        session.wait_text("STABLE", Duration::from_secs(1)).unwrap();

        let error = session
            .wait_stable(Duration::ZERO, Duration::ZERO)
            .unwrap_err();
        assert!(matches!(error, DriverError::Timeout { .. }), "{error}");
    }

    #[test]
    fn timeout_error_snapshots_the_final_screen_after_the_wait_started() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let session = shell_test_session(
            r"stty raw -echo; printf BEFORE; dd bs=1 count=1 of=/dev/null 2>/dev/null; printf '\r\033[KTIMEOUT_FINAL'; dd bs=1 count=1 of=/dev/null 2>/dev/null",
        );
        session.wait_text("BEFORE", Duration::from_secs(1)).unwrap();
        let released = AtomicBool::new(false);

        let error = session
            .wait_until("missing text".to_owned(), Duration::ZERO, |screen| {
                if screen.contains("BEFORE") && !released.swap(true, Ordering::SeqCst) {
                    session.type_text("x").unwrap();
                    session
                        .wait_text("TIMEOUT_FINAL", Duration::from_secs(1))
                        .unwrap();
                }
                false
            })
            .unwrap_err();
        let DriverError::Timeout { last_screen, .. } = error else {
            panic!("expected timeout error, got {error}");
        };
        assert!(last_screen.contains("TIMEOUT_FINAL"), "{last_screen:?}");
        assert!(!last_screen.contains("BEFORE"), "{last_screen:?}");
    }

    #[test]
    fn exit_error_snapshots_the_final_screen_after_the_wait_started() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let session = shell_test_session(
            r"stty raw -echo; printf BEFORE; dd bs=1 count=1 of=/dev/null 2>/dev/null; printf '\r\033[KFINAL_SCREEN'; exit 7",
        );
        session.wait_text("BEFORE", Duration::from_secs(1)).unwrap();
        let released = AtomicBool::new(false);

        let error = session
            .wait_until(
                "missing text".to_owned(),
                Duration::from_secs(1),
                |screen| {
                    if screen.contains("BEFORE") && !released.swap(true, Ordering::SeqCst) {
                        session.type_text("x").unwrap();
                    }
                    false
                },
            )
            .unwrap_err();
        let DriverError::Exited { last_screen, .. } = error else {
            panic!("expected exit error, got {error}");
        };
        assert!(last_screen.contains("FINAL_SCREEN"), "{last_screen:?}");
        assert!(!last_screen.contains("BEFORE"), "{last_screen:?}");
    }

    fn shell_test_session(script: &str) -> Session {
        shell_test_session_with_timeout(script, Duration::from_secs(2))
    }

    fn shell_test_session_with_timeout(script: &str, timeout: Duration) -> Session {
        SessionBuilder::new(Command::new("/bin/sh").args(["-c", script]))
            .size(Size::new(40, 4))
            .environment(
                Environment::clear()
                    .var("PATH", "/usr/bin:/bin")
                    .var("LC_ALL", "C"),
            )
            .timeout(timeout)
            .spawn()
            .unwrap()
    }

    fn assert_process_and_group_gone(child_pid: Pid) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if kill(child_pid, None) == Err(Errno::ESRCH)
                && killpg(child_pid, None) == Err(Errno::ESRCH)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "process or process group {child_pid} survived cleanup"
            );
            thread::yield_now();
        }
    }
}
