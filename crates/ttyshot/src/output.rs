use crate::{Error, parse::Format};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use ttyshot_core::Grid;
use ttyshot_render::{RenderOptions, Renderer};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputTarget {
    pub format: Format,
    pub path: PathBuf,
}

/// Resolves an output basename into one unique path per requested format.
///
/// # Errors
/// Returns an error when a format cannot be inferred or paths collide.
pub fn resolve_targets(output: &Path, requested: &[Format]) -> Result<Vec<OutputTarget>, Error> {
    let inferred = requested.is_empty();
    let formats = if inferred {
        let extension = output.extension().and_then(|v| v.to_str()).ok_or_else(|| {
            Error::msg(format!(
                "cannot infer output format from {}; pass --format",
                output.display()
            ))
        })?;
        vec![Format::from_extension(extension).ok_or_else(|| {
            Error::msg(format!(
                "cannot infer output format from extension {extension:?}; pass --format"
            ))
        })?]
    } else {
        let mut unique = Vec::new();
        for &format in requested {
            if !unique.contains(&format) {
                unique.push(format);
            }
        }
        unique
    };
    let known_extension = output
        .extension()
        .and_then(|v| v.to_str())
        .and_then(Format::from_extension)
        .is_some();
    let base = if known_extension {
        let mut base = output.to_path_buf();
        base.set_extension("");
        base
    } else {
        output.to_path_buf()
    };
    let targets = formats
        .into_iter()
        .map(|format| {
            let path = if inferred {
                output.to_path_buf()
            } else if known_extension {
                let mut path = base.clone();
                path.set_extension(format.extension());
                path
            } else {
                let mut name = output.as_os_str().to_os_string();
                name.push(format!(".{}", format.extension()));
                PathBuf::from(name)
            };
            OutputTarget { format, path }
        })
        .collect::<Vec<_>>();
    let mut paths = HashSet::new();
    for target in &targets {
        let key = absolute_lexical(&target.path)?;
        if !paths.insert(key) {
            return Err(Error::msg(format!(
                "multiple outputs resolve to {}",
                target.path.display()
            )));
        }
    }
    Ok(targets)
}

/// Renders all requested formats, then writes each through a same-directory temporary file.
///
/// # Errors
/// Returns an error when rendering, serialization, directory creation, or atomic replacement fails.
pub fn write_outputs(
    grid: &Grid,
    targets: &[OutputTarget],
    options: &RenderOptions,
) -> Result<(), Error> {
    let needs_renderer = targets
        .iter()
        .any(|target| matches!(target.format, Format::Png | Format::Svg));
    let renderer = needs_renderer
        .then(|| Renderer::new(options.clone()))
        .transpose()?;
    let mut encoded_outputs = Vec::with_capacity(targets.len());
    for target in targets {
        let bytes = match target.format {
            Format::Png => renderer
                .as_ref()
                .ok_or_else(|| Error::msg("internal error: PNG renderer was not initialized"))?
                .render_png(grid)?,
            Format::Svg => renderer
                .as_ref()
                .ok_or_else(|| Error::msg("internal error: SVG renderer was not initialized"))?
                .render_svg(grid)?
                .into_bytes(),
            Format::Txt => grid.plain_text().into_bytes(),
            Format::Json => {
                let mut bytes = serde_json::to_vec_pretty(grid)?;
                bytes.push(b'\n');
                bytes
            }
        };
        encoded_outputs.push((target.path.clone(), bytes));
    }
    for (path, bytes) in encoded_outputs {
        atomic_write(&path, &bytes)?;
    }
    Ok(())
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| {
        Error::msg(format!(
            "failed to create output directory {}: {e}",
            parent.display()
        ))
    })?;
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::msg(format!("output path {} has no file name", path.display())))?
        .to_string_lossy();
    let mut selected = None;
    for _ in 0..100 {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.ttyshot-{}-{serial}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                selected = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(Error::msg(format!(
                    "failed to create temporary output beside {}: {error}",
                    path.display()
                )));
            }
        }
    }
    let (temporary, mut file) = selected.ok_or_else(|| {
        Error::msg(format!(
            "could not reserve a unique temporary output beside {}",
            path.display()
        ))
    })?;
    let result = (|| {
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(Error::msg(format!(
            "failed to atomically write {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn absolute_lexical(path: &Path) -> Result<PathBuf, Error> {
    use std::path::Component;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttyshot_core::Grid;
    #[test]
    fn infers_and_replaces_known_extensions() {
        let inferred = resolve_targets(Path::new("out.png"), &[]).unwrap();
        assert_eq!(
            inferred,
            [OutputTarget {
                format: Format::Png,
                path: "out.png".into()
            }]
        );
        let all = resolve_targets(
            Path::new("out.png"),
            &[Format::Png, Format::Svg, Format::Txt, Format::Json],
        )
        .unwrap();
        assert_eq!(
            all.iter()
                .map(|v| v.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["out.png", "out.svg", "out.txt", "out.json"]
        );
    }
    #[test]
    fn normalizes_dot_segments_for_collision_keys() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolute_lexical(Path::new("shots/a/../x.png")).unwrap(),
            cwd.join("shots/x.png")
        );
    }
    #[test]
    fn preserves_unknown_extension_as_basename() {
        let targets =
            resolve_targets(Path::new("out.capture"), &[Format::Png, Format::Svg]).unwrap();
        assert_eq!(targets[0].path, Path::new("out.capture.png"));
    }
    #[test]
    fn writes_all_formats_and_rectangular_text() {
        let dir = tempfile::tempdir().unwrap();
        let grid = Grid::new(3, 2).unwrap();
        let targets = resolve_targets(
            &dir.path().join("frame"),
            &[Format::Png, Format::Svg, Format::Txt, Format::Json],
        )
        .unwrap();
        let options = RenderOptions {
            font_mode: ttyshot_render::FontMode::Deterministic,
            ..RenderOptions::default()
        };
        write_outputs(&grid, &targets, &options).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("frame.txt")).unwrap(),
            "   \n   "
        );
        let json = fs::read_to_string(dir.path().join("frame.json")).unwrap();
        assert!(json.starts_with("{\n  \"columns\": 3,"));
        assert!(
            fs::read(dir.path().join("frame.png"))
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        assert!(
            fs::read_to_string(dir.path().join("frame.svg"))
                .unwrap()
                .starts_with("<svg")
        );
    }
}
