#![cfg(unix)]
use std::{
    fs,
    path::{Path, PathBuf},
};
use ttyshot::scene;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/session.sh")
}
fn quoted(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}
fn write_scene(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("scenes.toml");
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn synthetic_pty_scene_writes_multiple_shots_in_order() {
    let temp = tempfile::tempdir().unwrap();
    let body = format!(
        r#"version=1
[defaults]
size="20x4"
timeout="3s"
formats=["txt"]
output_dir="shots"
[[scene]]
name="flow"
command=["/bin/sh",{},"two"]
[[scene.step]]
wait_text="READY FIRST"
[[scene.step]]
shot="first"
[[scene.step]]
type="next"
[[scene.step]]
key="enter"
[[scene.step]]
wait_text="SECOND:next"
[[scene.step]]
shot="second"
"#,
        quoted(&fixture())
    );
    let path = write_scene(temp.path(), &body);
    let file = scene::load(&path).unwrap();
    let written = scene::run(&file).unwrap();
    assert_eq!(
        written,
        [
            temp.path().join("shots/first.txt"),
            temp.path().join("shots/second.txt")
        ]
    );
    assert!(
        fs::read_to_string(&written[0])
            .unwrap()
            .contains("READY FIRST")
    );
    assert!(
        fs::read_to_string(&written[1])
            .unwrap()
            .contains("SECOND:next")
    );
}

#[test]
fn env_clear_cwd_and_home_are_applied() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("work")).unwrap();
    fs::create_dir(temp.path().join("home")).unwrap();
    let body = format!(
        r#"version=1
[defaults]
timeout="3s"
formats=["txt"]
[[scene]]
name="environment"
command=["/bin/sh",{},"env"]
env_clear=true
cwd="work"
home="home"
[[scene.step]]
wait_text="LEAK=unset"
[[scene.step]]
shot="env"
"#,
        quoted(&fixture())
    );
    let path = write_scene(temp.path(), &body);
    let file = scene::load(&path).unwrap();
    let written = scene::run(&file).unwrap();
    let text = fs::read_to_string(&written[0]).unwrap();
    let canonical_work = fs::canonicalize(temp.path().join("work")).unwrap();
    assert!(
        text.contains(&format!("CWD={}", canonical_work.display())),
        "{text}"
    );
    assert!(
        text.contains(&format!("HOME={}", temp.path().join("home").display())),
        "{text}"
    );
    assert!(text.contains("LEAK=unset"), "{text}");
}

#[test]
fn resize_reaches_the_real_pty() {
    let temp = tempfile::tempdir().unwrap();
    let body = format!(
        r#"version=1
[defaults]
size="10x4"
timeout="3s"
formats=["txt"]
[[scene]]
name="resize"
command=["/bin/sh",{},"resize"]
[[scene.step]]
wait_text="READY"
[[scene.step]]
resize="12x5"
[[scene.step]]
type="go"
[[scene.step]]
key="enter"
[[scene.step]]
wait_text="SIZE=12x5"
[[scene.step]]
shot="resized"
"#,
        quoted(&fixture())
    );
    let path = write_scene(temp.path(), &body);
    let written = scene::run(&scene::load(&path).unwrap()).unwrap();
    let text = fs::read_to_string(&written[0]).unwrap();
    assert!(text.contains("SIZE=12x5"), "{text}");
    assert_eq!(text.split('\n').count(), 5);
}

#[test]
fn timeout_keeps_last_screen_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let body = format!(
        r#"version=1
[[scene]]
name="timeout"
command=["/bin/sh",{},"timeout"]
timeout="2s"
[[scene.step]]
wait_text="NEVER"
timeout="100ms"
[[scene.step]]
shot="unreachable"
"#,
        quoted(&fixture())
    );
    let path = write_scene(temp.path(), &body);
    let error = scene::run(&scene::load(&path).unwrap())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("timed out while waiting for text \"NEVER\""),
        "{error}"
    );
    assert!(error.contains("last screen:\nLAST FRAME"), "{error}");
}
