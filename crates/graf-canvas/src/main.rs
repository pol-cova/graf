//! Converts legacy `.graf` canvas scenes to SVG.
//!
//! The canvas editor was removed in #111. This tool exists so files written
//! by it are not lost: the format is plain JSON, so the content is fully
//! recoverable. See `docs/adr/0001-native-swift-front-end.md`.
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

    let mut failures = 0;
    for path in &inputs {
        if let Err(error) = convert(path, output_dir.as_deref(), check_only) {
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

fn convert(path: &Path, output_dir: Option<&Path>, check_only: bool) -> Result<(), String> {
    let json = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let document =
        CanvasDocument::from_json(&json).map_err(|error| format!("not a .graf scene: {error}"))?;

    let destination = match output_dir {
        Some(directory) => directory.join(with_svg_extension(path)),
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

/// `scene.graf` -> `scene.svg`, keeping any directories in the input name so
/// a batch keeps its structure when writing into an output directory.
fn with_svg_extension(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("scene.svg"));
    name.set_extension("svg");
    name
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
