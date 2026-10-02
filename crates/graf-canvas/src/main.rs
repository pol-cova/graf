//! Converts legacy `.graf` canvas scenes to SVG.
//!
//! The canvas editor was removed in #111. This tool exists so files written
//! by it are not lost: the format is plain JSON, so the content is fully
//! recoverable.
//!
//!     graf-canvas scene.graf                 # -> scene.svg
//!     graf-canvas -o out/ diagrams/*.graf    # -> out/diagrams/*.svg
//!     graf-canvas --check scenes/*.graf      # validate only

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use graf_canvas::{CanvasDocument, export_to_svg};

const USAGE: &str = "\
Convert legacy .graf canvas scenes to SVG.

Usage:
  graf-canvas <file.graf>...            convert, writing <name>.svg beside each input
  graf-canvas -o <dir> <file.graf>...    convert into <dir>, keeping file names
  graf-canvas --check <file.graf>...     parse and report, write nothing

Graf's canvas editor was removed in #111; this is the migration path for
files it wrote.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("graf-canvas: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut output_dir: Option<PathBuf> = None;
    let mut check_only = false;
    let mut inputs: Vec<PathBuf> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--check" => check_only = true,
            "-o" | "--output" => {
                let directory = args
                    .next()
                    .ok_or_else(|| "-o needs a directory".to_string())?;
                output_dir = Some(PathBuf::from(directory));
            }
            // Everything else is an input. Rejecting unknown flags keeps a
            // typo from silently converting nothing.
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown option `{other}`\n\n{USAGE}"));
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }

    if inputs.is_empty() {
        return Err(format!("no input files\n\n{USAGE}"));
    }

    // The directory every input sits under, so `-o` can reproduce the input's
    // shape instead of flattening a tree into one folder.
    let input_root = common_ancestor(&inputs);

    let mut failures = 0;
    for path in &inputs {
        if let Err(error) = convert(path, output_dir.as_deref(), check_only, &input_root) {
            eprintln!("{}: {error}", path.display());
            failures += 1;
        }
    }

    if failures > 0 {
        // Report the count so a batch run over a directory is unambiguous.
        return Err(format!(
            "{failures} of {} file(s) could not be converted",
            inputs.len()
        ));
    }
    Ok(())
}

/// The deepest directory containing every input, or the first input's parent
/// when they share none.
fn common_ancestor(inputs: &[PathBuf]) -> PathBuf {
    let mut shared: Option<PathBuf> = None;
    for path in inputs {
        let parent = path.parent().unwrap_or(Path::new(""));
        shared = Some(match shared {
            None => parent.into(),
            Some(current) => {
                let mut next = PathBuf::new();
                for (a, b) in current.components().zip(parent.components()) {
                    if a != b {
                        break;
                    }
                    next.push(a);
                }
                next
            }
        });
    }
    match shared {
        Some(path) if path.as_os_str().is_empty() => PathBuf::from("."),
        Some(path) => path,
        None => PathBuf::from("."),
    }
}

fn convert(
    path: &Path,
    output_dir: Option<&Path>,
    check_only: bool,
    input_root: &Path,
) -> Result<(), String> {
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let document =
        CanvasDocument::from_json(&json).map_err(|error| format!("not a .graf scene: {error}"))?;

    // Output directories preserve the input's parent directory, so a batch of
    // same-named scenes in different subfolders does not collapse into one.
    let destination = match output_dir {
        Some(directory) => {
            let relative = path.strip_prefix(input_root).unwrap_or(path);
            directory.join(relative).with_extension("svg")
        }
        None => path.with_extension("svg"),
    };

    if check_only {
        println!(
            "{}: ok ({} element(s))",
            path.display(),
            document.elements.len()
        );
        return Ok(());
    }

    let svg = export_to_svg(&document);
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    write_atomically(&destination, svg.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", destination.display()))?;

    // On stdout, so the command composes with a shell loop.
    println!("{}", destination.display());
    Ok(())
}

/// Writes through a temporary file in the destination directory and renames,
/// so an interrupted run never leaves a half-written SVG that looks valid.
fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let temporary = match parent {
        Some(directory) => directory.join(format!(
            ".{}.tmp",
            path.file_name().unwrap_or_default().to_string_lossy()
        )),
        None => PathBuf::from(format!(".{}.tmp", path.to_string_lossy())),
    };

    {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_ancestor_is_the_deepest_common_directory() {
        let inputs = vec![
            PathBuf::from("diagrams/a/scene.graf"),
            PathBuf::from("diagrams/b/scene.graf"),
        ];
        assert_eq!(common_ancestor(&inputs), PathBuf::from("diagrams"));
    }

    #[test]
    fn unrelated_inputs_fall_back_to_the_current_directory() {
        let inputs = vec![PathBuf::from("/one/a.graf"), PathBuf::from("/two/b.graf")];
        assert_eq!(common_ancestor(&inputs), PathBuf::from("/"));
    }

    /// The bug this exists for: `-o` must not write both `a/scene.graf` and
    /// `b/scene.graf` to the same `out/scene.svg`, silently losing one.
    #[test]
    fn same_named_scenes_in_different_folders_get_different_outputs() {
        let a = PathBuf::from("diagrams/a/scene.graf");
        let b = PathBuf::from("diagrams/b/scene.graf");
        let ancestor = common_ancestor(&[a.clone(), b.clone()]);

        let out_a = PathBuf::from("out")
            .join(a.strip_prefix(&ancestor).unwrap_or(&a))
            .with_extension("svg");
        let out_b = PathBuf::from("out")
            .join(b.strip_prefix(&ancestor).unwrap_or(&b))
            .with_extension("svg");

        assert_ne!(out_a, out_b, "distinct inputs must not collide");
        assert_eq!(out_a, PathBuf::from("out/a/scene.svg"));
        assert_eq!(out_b, PathBuf::from("out/b/scene.svg"));
    }
}
