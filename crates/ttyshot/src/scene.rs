use crate::{
    Error,
    output::{OutputTarget, absolute_lexical, resolve_targets, write_outputs},
    panel::find_panel,
    parse::{Format, parse_duration, parse_env, parse_font, parse_key, parse_size},
};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use ttyshot_core::{CellRect, Grid, Theme};
use ttyshot_render::{FontMode, Padding, RenderOptions, WindowChrome};
use ttyshot_term::{Command, Environment, Key, SessionBuilder, Size};

#[derive(Debug)]
pub struct SceneFile {
    pub scenes: Vec<Scene>,
}
#[derive(Debug)]
pub struct Scene {
    pub name: String,
    pub command: Vec<String>,
    pub size: Size,
    pub timeout: Duration,
    pub environment: EnvironmentSpec,
    pub render: RenderOptions,
    pub steps: Vec<Step>,
}
#[derive(Debug)]
pub struct EnvironmentSpec {
    pub clear: bool,
    pub vars: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub home: Option<PathBuf>,
}
#[derive(Debug)]
pub enum Step {
    WaitText {
        value: String,
        timeout: Duration,
    },
    WaitRegex {
        value: String,
        timeout: Duration,
    },
    WaitAbsent {
        value: String,
        timeout: Duration,
    },
    WaitStable {
        value: Duration,
        timeout: Duration,
    },
    Key(Key),
    Type(String),
    Resize(Size),
    Shot {
        targets: Vec<OutputTarget>,
        crop: Option<CellRect>,
        panel: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    version: u32,
    #[serde(default)]
    defaults: RawConfig,
    scene: Vec<RawScene>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    size: Option<String>,
    timeout: Option<String>,
    env_clear: Option<bool>,
    formats: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    theme: Option<String>,
    scale: Option<u8>,
    padding: Option<u16>,
    chrome: Option<String>,
    shadow: Option<bool>,
    deterministic_fonts: Option<bool>,
    font: Option<String>,
    font_face_index: Option<u32>,
    fallback_fonts: Option<Vec<String>>,
    fallback_font_indices: Option<Vec<u32>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScene {
    name: String,
    command: Vec<String>,
    cwd: Option<PathBuf>,
    home: Option<PathBuf>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    env_clear: Option<bool>,
    size: Option<String>,
    timeout: Option<String>,
    formats: Option<Vec<String>>,
    output_dir: Option<PathBuf>,
    theme: Option<String>,
    scale: Option<u8>,
    padding: Option<u16>,
    chrome: Option<String>,
    shadow: Option<bool>,
    deterministic_fonts: Option<bool>,
    font: Option<String>,
    font_face_index: Option<u32>,
    fallback_fonts: Option<Vec<String>>,
    fallback_font_indices: Option<Vec<u32>>,
    #[serde(default)]
    step: Vec<RawStep>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    wait_text: Option<String>,
    wait_regex: Option<String>,
    wait_absent: Option<String>,
    wait_stable: Option<String>,
    key: Option<String>,
    #[serde(rename = "type")]
    type_text: Option<String>,
    resize: Option<String>,
    shot: Option<String>,
    timeout: Option<String>,
    formats: Option<Vec<String>>,
    crop: Option<RawRect>,
    panel: Option<String>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl RawScene {
    fn config(&self) -> RawConfig {
        RawConfig {
            size: self.size.clone(),
            timeout: self.timeout.clone(),
            env_clear: self.env_clear,
            formats: self.formats.clone(),
            output_dir: self.output_dir.clone(),
            theme: self.theme.clone(),
            scale: self.scale,
            padding: self.padding,
            chrome: self.chrome.clone(),
            shadow: self.shadow,
            deterministic_fonts: self.deterministic_fonts,
            font: self.font.clone(),
            font_face_index: self.font_face_index,
            fallback_fonts: self.fallback_fonts.clone(),
            fallback_font_indices: self.fallback_font_indices.clone(),
        }
    }
}
fn merged(base: &RawConfig, overlay: &RawConfig) -> RawConfig {
    RawConfig {
        size: overlay.size.clone().or_else(|| base.size.clone()),
        timeout: overlay.timeout.clone().or_else(|| base.timeout.clone()),
        env_clear: overlay.env_clear.or(base.env_clear),
        formats: overlay.formats.clone().or_else(|| base.formats.clone()),
        output_dir: overlay
            .output_dir
            .clone()
            .or_else(|| base.output_dir.clone()),
        theme: overlay.theme.clone().or_else(|| base.theme.clone()),
        scale: overlay.scale.or(base.scale),
        padding: overlay.padding.or(base.padding),
        chrome: overlay.chrome.clone().or_else(|| base.chrome.clone()),
        shadow: overlay.shadow.or(base.shadow),
        deterministic_fonts: overlay.deterministic_fonts.or(base.deterministic_fonts),
        font: overlay.font.clone().or_else(|| base.font.clone()),
        font_face_index: overlay.font_face_index.or(base.font_face_index),
        fallback_fonts: overlay
            .fallback_fonts
            .clone()
            .or_else(|| base.fallback_fonts.clone()),
        fallback_font_indices: overlay
            .fallback_font_indices
            .clone()
            .or_else(|| base.fallback_font_indices.clone()),
    }
}

/// Loads and fully validates a version-1 scene file.
///
/// # Errors
/// Returns an error for I/O, TOML, schema, or cross-scene validation failures.
pub fn load(path: &Path) -> Result<SceneFile, Error> {
    let contents = fs::read_to_string(path)
        .map_err(|e| Error::msg(format!("failed to read scene file {}: {e}", path.display())))?;
    let raw: RawFile = toml::from_str(&contents)
        .map_err(|e| Error::msg(format!("invalid scene file {}: {e}", path.display())))?;
    validate(raw, path)
}
/// Parses scene contents using `source` as the base for relative paths.
///
/// # Errors
/// Returns an error for TOML, schema, or cross-scene validation failures.
pub fn parse(contents: &str, source: &Path) -> Result<SceneFile, Error> {
    let raw: RawFile = toml::from_str(contents)
        .map_err(|e| Error::msg(format!("invalid scene file {}: {e}", source.display())))?;
    validate(raw, source)
}

#[allow(clippy::too_many_lines)] // Strict cross-field validation stays in file order.
fn validate(raw: RawFile, source: &Path) -> Result<SceneFile, Error> {
    if raw.version != 1 {
        return Err(Error::msg(format!(
            "unsupported scene file version {}; expected version = 1",
            raw.version
        )));
    }
    if raw.scene.is_empty() {
        return Err(Error::msg("scene file must contain at least one [[scene]]"));
    }
    let base = source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut names = HashSet::new();
    for scene in &raw.scene {
        if scene.name.is_empty() {
            return Err(Error::msg("scene name must not be empty"));
        }
        if !names.insert(scene.name.clone()) {
            return Err(Error::msg(format!("duplicate scene name {:?}", scene.name)));
        }
    }
    let mut paths = HashSet::new();
    let mut scenes = Vec::new();
    for scene in raw.scene {
        if scene.command.is_empty() || scene.command[0].is_empty() {
            return Err(Error::msg(format!(
                "scene {:?} command must contain a non-empty program",
                scene.name
            )));
        }
        if scene.command.iter().any(|v| v.contains('\0')) {
            return Err(Error::msg(format!(
                "scene {:?} command must not contain NUL",
                scene.name
            )));
        }
        let config = merged(&raw.defaults, &scene.config());
        let size = parse_size(config.size.as_deref().unwrap_or("80x24"))
            .map_err(|e| context(&scene.name, e))?;
        let timeout = parse_duration(config.timeout.as_deref().unwrap_or("30s"))
            .map_err(|e| context(&scene.name, e))?;
        let default_formats = parse_formats(
            config.formats.as_deref().unwrap_or(&["png".to_owned()]),
            &scene.name,
        )?;
        let output_dir = resolve_path(
            base,
            config
                .output_dir
                .as_deref()
                .unwrap_or_else(|| Path::new(".")),
        );
        let render = render_options(&config, base, &scene.name)?;
        let mut vars = Vec::new();
        for (key, value) in scene.env {
            vars.push(parse_env(&format!("{key}={value}")).map_err(|e| context(&scene.name, e))?);
        }
        let environment = EnvironmentSpec {
            clear: config.env_clear.unwrap_or(false),
            vars,
            cwd: scene.cwd.map(|v| resolve_path(base, &v)),
            home: scene.home.map(|v| resolve_path(base, &v)),
        };
        for candidate in environment.cwd.iter().chain(environment.home.iter()) {
            if candidate.as_os_str().to_string_lossy().contains('\0') {
                return Err(Error::msg(format!(
                    "scene {:?} path must not contain NUL",
                    scene.name
                )));
            }
        }
        let mut steps = Vec::new();
        let mut has_shot = false;
        for (index, step) in scene.step.into_iter().enumerate() {
            let label = format!("scene {:?} step {}", scene.name, index + 1);
            let action_count = [
                step.wait_text.is_some(),
                step.wait_regex.is_some(),
                step.wait_absent.is_some(),
                step.wait_stable.is_some(),
                step.key.is_some(),
                step.type_text.is_some(),
                step.resize.is_some(),
                step.shot.is_some(),
            ]
            .into_iter()
            .filter(|v| *v)
            .count();
            if action_count != 1 {
                return Err(Error::msg(format!(
                    "{label} must contain exactly one action; found {action_count}"
                )));
            }
            if step.wait_text.as_deref() == Some("")
                || step.wait_regex.as_deref() == Some("")
                || step.wait_absent.as_deref() == Some("")
            {
                return Err(Error::msg(format!("{label}: wait value must not be empty")));
            }
            if step.panel.as_deref() == Some("") {
                return Err(Error::msg(format!(
                    "{label}: panel title must not be empty"
                )));
            }
            let is_wait = step.wait_text.is_some()
                || step.wait_regex.is_some()
                || step.wait_absent.is_some()
                || step.wait_stable.is_some();
            if step.timeout.is_some() && !is_wait {
                return Err(Error::msg(format!(
                    "{label}: timeout is valid only on wait steps"
                )));
            }
            let shot_only = step.shot.is_some();
            if !shot_only && (step.formats.is_some() || step.crop.is_some() || step.panel.is_some())
            {
                return Err(Error::msg(format!(
                    "{label}: formats, crop, and panel are valid only on shot steps"
                )));
            }
            if step.crop.is_some() && step.panel.is_some() {
                return Err(Error::msg(format!(
                    "{label}: panel and crop are mutually exclusive"
                )));
            }
            let wait_timeout = step
                .timeout
                .as_deref()
                .map(parse_duration)
                .transpose()
                .map_err(|e| Error::msg(format!("{label}: {e}")))?
                .unwrap_or(timeout);
            let parsed = if let Some(value) = step.wait_text {
                Step::WaitText {
                    value,
                    timeout: wait_timeout,
                }
            } else if let Some(value) = step.wait_regex {
                Regex::new(&value)
                    .map_err(|e| Error::msg(format!("{label}: invalid wait regex: {e}")))?;
                Step::WaitRegex {
                    value,
                    timeout: wait_timeout,
                }
            } else if let Some(value) = step.wait_absent {
                Step::WaitAbsent {
                    value,
                    timeout: wait_timeout,
                }
            } else if let Some(value) = step.wait_stable {
                Step::WaitStable {
                    value: parse_duration(&value)
                        .map_err(|e| Error::msg(format!("{label}: {e}")))?,
                    timeout: wait_timeout,
                }
            } else if let Some(value) = step.key {
                Step::Key(parse_key(&value).map_err(|e| Error::msg(format!("{label}: {e}")))?)
            } else if let Some(value) = step.type_text {
                if value.contains('\0') {
                    return Err(Error::msg(format!(
                        "{label}: typed text must not contain NUL"
                    )));
                }
                Step::Type(value)
            } else if let Some(value) = step.resize {
                Step::Resize(parse_size(&value).map_err(|e| Error::msg(format!("{label}: {e}")))?)
            } else {
                has_shot = true;
                let basename = step.shot.expect("one action");
                if basename.is_empty() || basename.contains('\0') {
                    return Err(Error::msg(format!(
                        "{label}: shot basename must not be empty or contain NUL"
                    )));
                }
                validate_shot_basename(&basename)
                    .map_err(|message| Error::msg(format!("{label}: {message}")))?;
                let formats = step
                    .formats
                    .as_deref()
                    .map(|v| parse_formats(v, &label))
                    .transpose()?
                    .unwrap_or_else(|| default_formats.clone());
                let targets = resolve_targets(&output_dir.join(&basename), &formats)?;
                for target in &targets {
                    let key = absolute(&target.path)?;
                    if !paths.insert(key) {
                        return Err(Error::msg(format!(
                            "duplicate scene output path {}",
                            target.path.display()
                        )));
                    }
                }
                let crop = step.crop.map(|v| CellRect {
                    x: v.x,
                    y: v.y,
                    width: v.width,
                    height: v.height,
                });
                if let Some(rect) = crop
                    && (rect.width == 0 || rect.height == 0)
                {
                    return Err(Error::msg(format!(
                        "{label}: crop width and height must be non-zero"
                    )));
                }
                Step::Shot {
                    targets,
                    crop,
                    panel: step.panel,
                }
            };
            steps.push(parsed);
        }
        if !has_shot {
            return Err(Error::msg(format!(
                "scene {:?} must contain at least one shot step",
                scene.name
            )));
        }
        let mut command = scene.command;
        command[0] = resolve_scene_program(base, &command[0]);
        scenes.push(Scene {
            name: scene.name,
            command,
            size,
            timeout,
            environment,
            render,
            steps,
        });
    }
    Ok(SceneFile { scenes })
}
fn validate_shot_basename(value: &str) -> Result<(), String> {
    use std::path::Component;
    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!(
            "shot {value:?} must be one relative file-name component without separators, '.' or '..'"
        )),
    }
}

fn resolve_scene_program(base: &Path, program: &str) -> String {
    use std::path::Component;
    let path = Path::new(program);
    let path_like = path.components().count() > 1
        || matches!(
            path.components().next(),
            Some(Component::CurDir | Component::ParentDir)
        );
    if path.is_relative() && path_like {
        base.join(path).to_string_lossy().into_owned()
    } else {
        program.to_owned()
    }
}

fn context(scene: &str, message: impl std::fmt::Display) -> Error {
    Error::msg(format!("scene {scene:?}: {message}"))
}
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
fn absolute(path: &Path) -> Result<PathBuf, Error> {
    absolute_lexical(path)
}
fn parse_formats(values: &[String], label: &str) -> Result<Vec<Format>, Error> {
    if values.is_empty() {
        return Err(Error::msg(format!("{label}: formats must not be empty")));
    }
    values
        .iter()
        .map(|v| {
            v.parse()
                .map_err(|e: String| Error::msg(format!("{label}: {e}")))
        })
        .collect()
}
fn render_options(config: &RawConfig, base: &Path, scene: &str) -> Result<RenderOptions, Error> {
    let theme = match config.theme.as_deref().unwrap_or("dark") {
        "dark" => Theme::dark(),
        "light" => Theme::light(),
        value => {
            return Err(context(
                scene,
                format!("invalid theme {value:?}; expected dark or light"),
            ));
        }
    };
    let chrome = match config.chrome.as_deref().unwrap_or("none") {
        "none" => WindowChrome::None,
        "macos" => WindowChrome::MacOs,
        value => {
            return Err(context(
                scene,
                format!("invalid chrome {value:?}; expected none or macos"),
            ));
        }
    };
    let scale = config.scale.unwrap_or(1);
    if !(1..=4).contains(&scale) {
        return Err(context(scene, "scale must be between 1 and 4".to_owned()));
    }
    let font = config
        .font
        .as_deref()
        .map(|v| parse_font(v, config.font_face_index, Some(base)))
        .transpose()
        .map_err(|e| context(scene, e))?;
    let fallback_values = config.fallback_fonts.as_deref().unwrap_or_default();
    let fallback_indices = config.fallback_font_indices.as_deref().unwrap_or_default();
    if fallback_indices.len() > fallback_values.len() {
        return Err(context(
            scene,
            "more fallback_font_indices than fallback_fonts".to_owned(),
        ));
    }
    let fallback_fonts = fallback_values
        .iter()
        .enumerate()
        .map(|(i, v)| parse_font(v, fallback_indices.get(i).copied(), Some(base)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| context(scene, e))?;
    Ok(RenderOptions {
        theme,
        padding: Padding::all(config.padding.unwrap_or(16)),
        scale,
        chrome,
        shadow: config.shadow.unwrap_or(false),
        font_mode: if config.deterministic_fonts.unwrap_or(false) {
            FontMode::Deterministic
        } else {
            FontMode::System
        },
        font,
        fallback_fonts,
        ..RenderOptions::default()
    })
}

/// Executes all scenes and returns output paths in shot/format order.
///
/// # Errors
/// Returns an error for terminal actions, panel/crop selection, rendering, or writes.
pub fn run(file: &SceneFile) -> Result<Vec<PathBuf>, Error> {
    let mut written = Vec::new();
    for scene in &file.scenes {
        run_scene(scene, &mut written)?;
    }
    Ok(written)
}
fn run_scene(scene: &Scene, written: &mut Vec<PathBuf>) -> Result<(), Error> {
    let command = Command::new(&scene.command[0]).args(scene.command.iter().skip(1).cloned());
    let mut environment = if scene.environment.clear {
        Environment::clear()
    } else {
        Environment::inherit()
    };
    for (key, value) in &scene.environment.vars {
        environment = environment.var(key, value);
    }
    if let Some(cwd) = &scene.environment.cwd {
        environment = environment.cwd(cwd);
    }
    if let Some(home) = &scene.environment.home {
        environment = environment.home(home);
    }
    let session = SessionBuilder::new(command)
        .size(scene.size)
        .timeout(scene.timeout)
        .environment(environment)
        .theme(scene.render.theme.clone())
        .spawn()
        .map_err(|e| Error::msg(format!("scene {:?}: {e}", scene.name)))?;
    for (index, step) in scene.steps.iter().enumerate() {
        let result: Result<Option<Grid>, Error> = match step {
            Step::WaitText { value, timeout } => session
                .wait_text(value, *timeout)
                .map(Some)
                .map_err(Error::from),
            Step::WaitRegex { value, timeout } => session
                .wait_regex(value, *timeout)
                .map(Some)
                .map_err(Error::from),
            Step::WaitAbsent { value, timeout } => session
                .wait_absent(value, *timeout)
                .map(Some)
                .map_err(Error::from),
            Step::WaitStable { value, timeout } => session
                .wait_stable(*value, *timeout)
                .map(Some)
                .map_err(Error::from),
            Step::Key(key) => session.key(*key).map(|()| None).map_err(Error::from),
            Step::Type(value) => session.type_text(value).map(|()| None).map_err(Error::from),
            Step::Resize(size) => session.resize(*size).map(|()| None).map_err(Error::from),
            Step::Shot {
                targets,
                crop,
                panel,
            } => {
                let mut grid = session.shot_grid();
                if let Some(title) = panel {
                    let rect = find_panel(&grid, title).map_err(|e| {
                        Error::msg(format!("{e}\nlast screen:\n{}", grid.plain_text()))
                    })?;
                    grid = grid.crop(rect)?;
                } else if let Some(rect) = crop {
                    grid = grid.crop(*rect).map_err(|e| {
                        Error::msg(format!(
                            "invalid crop in scene {:?} step {}: {e}",
                            scene.name,
                            index + 1
                        ))
                    })?;
                }
                write_outputs(&grid, targets, &scene.render)?;
                written.extend(targets.iter().map(|v| v.path.clone()));
                Ok(None)
            }
        };
        result
            .map_err(|e| Error::msg(format!("scene {:?} step {}: {e}", scene.name, index + 1)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_ok(body: &str) -> SceneFile {
        parse(body, Path::new("/tmp/project/scenes.toml")).unwrap()
    }
    #[test]
    fn strict_validation_rejects_schema_errors() {
        let cases = [
            "version=2\n[[scene]]\nname='x'\ncommand=['x']",
            "version=1\nwat=1\n[[scene]]\nname='x'\ncommand=['x']",
            "version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene.step]]\nkey='a'\ntype='b'",
            "version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene.step]]\nkey='a'\ntimeout='1s'",
            "version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene.step]]\nshot='x'\npanel='a'\ncrop={x=0,y=0,width=1,height=1}",
        ];
        for case in cases {
            assert!(parse(case, Path::new("scene.toml")).is_err(), "{case}");
        }
    }
    #[test]
    fn defaults_paths_and_order_are_resolved() {
        let file = parse_ok(
            "version=1\n[defaults]\nsize='10x4'\ntimeout='2s'\nformats=['txt','json']\noutput_dir='shots'\n[[scene]]\nname='x'\ncommand=['app']\ncwd='fixtures'\nhome='home'\n[[scene.step]]\nwait_text='Ready'\n[[scene.step]]\nshot='first'\n[[scene.step]]\nresize='12x5'\n[[scene.step]]\nshot='second'\n",
        );
        let scene = &file.scenes[0];
        assert_eq!(
            scene.environment.cwd.as_deref(),
            Some(Path::new("/tmp/project/fixtures"))
        );
        assert!(
            matches!(&scene.steps[0],Step::WaitText{timeout,..} if *timeout==Duration::from_secs(2))
        );
        assert!(
            matches!(&scene.steps[1],Step::Shot{targets,..} if targets[0].path==Path::new("/tmp/project/shots/first.txt"))
        );
        assert!(
            matches!(&scene.steps[3],Step::Shot{targets,..} if targets[1].path==Path::new("/tmp/project/shots/second.json"))
        );
    }
    #[test]
    fn duplicate_names_and_outputs_fail() {
        assert!(
            parse(
                "version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene]]\nname='x'\ncommand=['x']",
                Path::new("s.toml")
            )
            .unwrap_err()
            .to_string()
            .contains("duplicate scene")
        );
        assert!(parse("version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene.step]]\nshot='same'\n[[scene.step]]\nshot='same'",Path::new("s.toml")).unwrap_err().to_string().contains("duplicate scene output"));
    }
    #[test]
    fn rejects_no_shot_empty_waits_and_empty_panel() {
        let cases = [
            "version=1
[[scene]]
name='x'
command=['x']
[[scene.step]]
key='a'",
            "version=1
[[scene]]
name='x'
command=['x']
[[scene.step]]
wait_text=''
[[scene.step]]
shot='x'",
            "version=1
[[scene]]
name='x'
command=['x']
[[scene.step]]
wait_regex=''
[[scene.step]]
shot='x'",
            "version=1
[[scene]]
name='x'
command=['x']
[[scene.step]]
wait_absent=''
[[scene.step]]
shot='x'",
            "version=1
[[scene]]
name='x'
command=['x']
[[scene.step]]
shot='x'
panel=''",
        ];
        for case in cases {
            assert!(parse(case, Path::new("scene.toml")).is_err(), "{case}");
        }
    }
    #[test]
    fn rejects_shot_paths_that_escape_the_output_directory() {
        for shot in ["../escaped", "/tmp/escaped", "nested/escaped", ".", ".."] {
            let body = format!(
                "version=1\n[[scene]]\nname='x'\ncommand=['x']\n[[scene.step]]\nshot={shot:?}"
            );
            let error = parse(&body, Path::new("scene.toml"))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("one relative file-name component"),
                "{shot}: {error}"
            );
        }
    }

    #[test]
    fn resolves_path_like_programs_from_the_scene_directory() {
        let file = parse_ok(
            "version=1\n[[scene]]\nname='x'\ncommand=['./fixtures/app','arg']\n[[scene.step]]\nshot='frame'",
        );
        assert_eq!(
            Path::new(&file.scenes[0].command[0]),
            Path::new("/tmp/project/fixtures/app")
        );
        assert_eq!(file.scenes[0].command[1], "arg");

        let bare = parse_ok(
            "version=1\n[[scene]]\nname='x'\ncommand=['app']\n[[scene.step]]\nshot='frame'",
        );
        assert_eq!(bare.scenes[0].command[0], "app");
    }

    #[test]
    fn invalid_values_have_context() {
        for fragment in ["size='0x2'", "timeout='later'", "env={ '=BAD'='x' }"] {
            let body = format!("version=1\n[[scene]]\nname='bad'\ncommand=['x']\n{fragment}");
            assert!(parse(&body, Path::new("s.toml")).is_err(), "{fragment}");
        }
    }
}
