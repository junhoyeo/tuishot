#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::killpg;
use nix::unistd::Pid;
use ttyshot_term::{Command, Environment, Key, Session, SessionBuilder, Size};

const QUERY_SCRIPT: &str = r#"
stty raw -echo
esc=$(printf '\033')
query() {
    expected=$1
    count=${#expected}
    printf "$2"
    actual=$(dd bs=1 count="$count" 2>/dev/null)
    if [ "$actual" != "$expected" ]; then
        printf '\033[H\033[2JQUERY_FAILED'
        exit 7
    fi
}
query "${esc}[?6c" '\033[c'
query "${esc}[>0;2600;1c" '\033[>c'
query "${esc}[0n" '\033[5n'
printf '\033[3;4H'
query "${esc}[3;4R" '\033[6n'
query "${esc}]10;rgb:d8d8/dede/e9e9$(printf '\007')" '\033]10;?\007'
query "${esc}]11;rgb:1717/1b1b/2424$(printf '\007')" '\033]11;?\007'
query "${esc}[?0u" '\033[?u'
printf '\033[H\033[2JQUERY_OK'
"#;

fn clear_environment() -> Environment {
    Environment::clear()
        .var("PATH", "/usr/bin:/bin")
        .var("LC_ALL", "C")
}

fn shell_session(script: &str, size: Size, timeout: Duration) -> Session {
    SessionBuilder::new(Command::new("/bin/sh").args(["-c", script]))
        .size(size)
        .environment(clear_environment())
        .timeout(timeout)
        .spawn()
        .unwrap()
}

#[test]
fn pty_starts_at_the_exact_requested_size() {
    let session = SessionBuilder::new(Command::new("/bin/stty").arg("size"))
        .size(Size::new(73, 19))
        .environment(clear_environment())
        .timeout(Duration::from_secs(2))
        .spawn()
        .unwrap();

    let grid = session.wait_text("19 73", Duration::from_secs(1)).unwrap();
    assert_eq!((grid.columns(), grid.rows()), (73, 19));
}

#[test]
fn child_receives_all_required_terminal_query_replies() {
    let session = shell_session(QUERY_SCRIPT, Size::new(80, 8), Duration::from_secs(2));

    session
        .wait_text("QUERY_OK", Duration::from_secs(1))
        .unwrap();
}

#[test]
fn waits_type_key_and_stability_are_event_driven() {
    let script = r"
stty raw -echo
printf 'loading'
dd bs=1 count=1 of=/dev/null 2>/dev/null
printf '\r\033[Kready 42'
dd bs=1 count=1 of=/dev/null 2>/dev/null
printf '\r\nKEY_OK'
dd bs=1 count=1 of=/dev/null 2>/dev/null
";
    let session = shell_session(script, Size::new(40, 5), Duration::from_secs(3));

    session
        .wait_text("loading", Duration::from_secs(1))
        .unwrap();
    session
        .wait_regex("load[a-z]+", Duration::from_secs(1))
        .unwrap();
    session
        .wait_absent("not present", Duration::from_secs(1))
        .unwrap();

    session.type_text("x").unwrap();
    session
        .wait_regex(r"ready\s+42", Duration::from_secs(1))
        .unwrap();
    session
        .wait_absent("loading", Duration::from_secs(1))
        .unwrap();

    session.key(Key::Enter).unwrap();
    session.wait_text("KEY_OK", Duration::from_secs(1)).unwrap();
    let stable = session
        .wait_stable(Duration::from_millis(20), Duration::from_secs(1))
        .unwrap();
    assert!(stable.plain_text().contains("KEY_OK"));
}

#[test]
fn resize_updates_the_emulator_and_the_kernel_pty() {
    let script = r"
stty raw -echo
printf 'SIZE1:'
stty size
dd bs=1 count=1 of=/dev/null 2>/dev/null
printf '\r\nSIZE2:'
stty size
dd bs=1 count=1 of=/dev/null 2>/dev/null
";
    let session = shell_session(script, Size::new(40, 8), Duration::from_secs(3));
    session
        .wait_text("SIZE1:8 40", Duration::from_secs(1))
        .unwrap();

    session.resize(Size::new(100, 30)).unwrap();
    session.type_text("x").unwrap();
    let grid = session
        .wait_text("SIZE2:30 100", Duration::from_secs(1))
        .unwrap();
    assert_eq!((grid.columns(), grid.rows()), (100, 30));
}

#[test]
fn clear_environment_applies_overrides_cwd_and_optional_home() {
    let temporary = std::env::temp_dir().join(format!("ttyshot-env-{}", std::process::id()));
    fs::create_dir_all(&temporary).unwrap();
    let home = temporary.join("fixture-home");
    fs::create_dir_all(&home).unwrap();

    let environment = clear_environment()
        .var("FIXTURE_VALUE", "synthetic")
        .cwd(&temporary)
        .home(&home);
    let session = SessionBuilder::new(Command::new("/bin/sh").args([
        "-c",
        "printf 'cwd='; /bin/pwd; printf 'home=%s\\nvalue=%s\\nterm=%s\\n' \"$HOME\" \"$FIXTURE_VALUE\" \"$TERM\"",
    ]))
    .size(Size::new(120, 8))
    .environment(environment)
    .timeout(Duration::from_secs(2))
    .spawn()
    .unwrap();

    let expected_cwd = format!("cwd={}", temporary.canonicalize().unwrap().display());
    let expected_home = format!("home={}", home.display());
    session
        .wait_text(&expected_cwd, Duration::from_secs(1))
        .unwrap();
    session
        .wait_text(&expected_home, Duration::from_secs(1))
        .unwrap();
    session
        .wait_text("value=synthetic", Duration::from_secs(1))
        .unwrap();
    session
        .wait_text("term=xterm-256color", Duration::from_secs(1))
        .unwrap();

    fs::remove_dir_all(temporary).unwrap();
}

#[test]
fn inherited_environment_keeps_parent_values_and_applies_overrides() {
    let session = SessionBuilder::new(Command::new("/bin/sh").args([
        "-c",
        "test -n \"$PATH\" && printf 'INHERITED:%s' \"$FIXTURE_VALUE\"",
    ]))
    .size(Size::new(40, 3))
    .environment(Environment::inherit().var("FIXTURE_VALUE", "override"))
    .timeout(Duration::from_secs(2))
    .spawn()
    .unwrap();

    session
        .wait_text("INHERITED:override", Duration::from_secs(1))
        .unwrap();
}

#[test]
fn styled_wide_output_converts_to_the_shared_grid() {
    let session = shell_session(
        "printf '\\033[1;38;2;1;2;3;48;5;4mX\\033[0m界'",
        Size::new(20, 3),
        Duration::from_secs(2),
    );
    let grid = session.wait_text("X界", Duration::from_secs(1)).unwrap();

    let styled = grid.get(0, 0).unwrap();
    assert!(styled.style.attributes.bold);
    assert_eq!(
        styled.style.foreground,
        ttyshot_term::core::Color::Rgb(ttyshot_term::core::Rgb::new(1, 2, 3))
    );
    assert_eq!(
        styled.style.background,
        ttyshot_term::core::Color::Indexed(4)
    );
    assert_eq!(
        grid.get(1, 0).unwrap().width,
        ttyshot_term::core::CellWidth::Wide
    );
    assert_eq!(
        grid.get(2, 0).unwrap().width,
        ttyshot_term::core::CellWidth::Continuation
    );
}

#[test]
fn hard_timeout_kills_the_complete_process_group_and_reports_screen() {
    let pid_file = unique_pid_file("timeout");
    let script = process_tree_script(&pid_file);
    let session = shell_session(&script, Size::new(50, 4), Duration::from_millis(250));
    session
        .wait_text("TREE_READY", Duration::from_millis(200))
        .unwrap();
    let process_group = read_process_group(&pid_file);

    let error = session
        .wait_text("NEVER", Duration::from_secs(2))
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("text \"NEVER\""), "{message}");
    assert!(message.contains("TREE_READY"), "{message}");
    assert_process_group_gone(process_group);
    drop(session);
    fs::remove_file(pid_file).unwrap();
}

#[test]
fn dropping_a_session_kills_the_complete_process_group() {
    let pid_file = unique_pid_file("drop");
    let script = process_tree_script(&pid_file);
    let session = shell_session(&script, Size::new(50, 4), Duration::from_secs(5));
    session
        .wait_text("TREE_READY", Duration::from_secs(1))
        .unwrap();
    let process_group = read_process_group(&pid_file);

    drop(session);

    assert_process_group_gone(process_group);
    fs::remove_file(pid_file).unwrap();
}

#[test]
fn timeout_names_the_wait_and_includes_the_last_screen() {
    let session = shell_session(
        "stty raw -echo; printf 'LAST_FRAME'; dd bs=1 count=1 of=/dev/null 2>/dev/null",
        Size::new(30, 3),
        Duration::from_secs(2),
    );
    session
        .wait_text("LAST_FRAME", Duration::from_secs(1))
        .unwrap();

    let error = session
        .wait_regex("missing-[0-9]+", Duration::from_millis(20))
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("regex \"missing-[0-9]+\""), "{message}");
    assert!(message.contains("LAST_FRAME"), "{message}");
}

fn unique_pid_file(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "ttyshot-{label}-{}-{}.pid",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ))
}

fn process_tree_script(path: &Path) -> String {
    format!(
        "sleep 30 & child=$!; printf '%s %s\\n' \"$$\" \"$child\" > '{}'; printf TREE_READY; wait",
        path.display()
    )
}

fn read_process_group(path: &Path) -> Pid {
    let contents = fs::read_to_string(path).unwrap();
    let leader = contents.split_whitespace().next().unwrap().parse().unwrap();
    Pid::from_raw(leader)
}

fn assert_process_group_gone(process_group: Pid) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match killpg(process_group, None) {
            Err(Errno::ESRCH) => return,
            Ok(()) | Err(Errno::EPERM) => {}
            Err(error) => panic!("unexpected process group probe error: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "process group {process_group} survived its kill deadline"
        );
        std::thread::yield_now();
    }
}
