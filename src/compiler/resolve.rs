use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// Where a resolved engine executable came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineSource {
    /// `GRAF_TECTONIC_PATH` or `GRAF_TYPST_PATH`.
    EnvOverride,
    /// Copied into `graf.app/Contents/Resources/bin` at build time.
    Bundled,
    /// An existing installation on this machine.
    System,
}

impl std::fmt::Display for EngineSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EngineSource::EnvOverride => "env override",
            EngineSource::Bundled => "bundled",
            EngineSource::System => "system",
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedEngine {
    pub path: PathBuf,
    pub source: EngineSource,
}

/// Resolve a compiler executable: env override first, then the copy bundled
/// in the app, then an existing system installation.
pub fn resolve(name: &str, env_var: &str, common_paths: &[&str]) -> Option<ResolvedEngine> {
    let env_value = std::env::var_os(env_var);
    let home = crate::util::home_dir();
    resolve_with(
        name,
        env_value.as_deref(),
        home.as_deref(),
        common_paths,
        bundled_dir().as_deref(),
        &which_system,
    )
}

/// Directory holding compiler binaries shipped inside graf.app, when running
/// from a bundle layout. Dev builds (`target/debug/graf`) return `None`.
pub fn bundled_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos_dir = exe.parent()?;
    Some(macos_dir.parent()?.join("Resources").join("bin"))
}

pub(crate) fn resolve_with(
    name: &str,
    env_value: Option<&OsStr>,
    home: Option<&Path>,
    common_paths: &[&str],
    bundled_dir: Option<&Path>,
    which: &dyn Fn(&str) -> Option<PathBuf>,
) -> Option<ResolvedEngine> {
    if let Some(value) = env_value {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(ResolvedEngine {
                path,
                source: EngineSource::EnvOverride,
            });
        }
    }

    if let Some(dir) = bundled_dir {
        let path = dir.join(name);
        if path.is_file() {
            return Some(ResolvedEngine {
                path,
                source: EngineSource::Bundled,
            });
        }
    }

    if let Some(path) = which(name) {
        return Some(ResolvedEngine {
            path,
            source: EngineSource::System,
        });
    }

    for entry in common_paths {
        let path = expand_home(entry, home);
        if path.is_file() {
            return Some(ResolvedEngine {
                path,
                source: EngineSource::System,
            });
        }
    }

    None
}

fn which_cache() -> &'static Mutex<HashMap<String, Option<PathBuf>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached `which` lookup. The underlying subprocess spawn blocks, so callers
/// must only invoke this from a background thread (engine resolution is lazy
/// for exactly this reason). Results are cached per binary name so repeated
/// compiles do not re-spawn `which`.
fn which_system(name: &str) -> Option<PathBuf> {
    if let Ok(cache) = which_cache().lock()
        && let Some(cached) = cache.get(name)
    {
        return cached.clone();
    }

    let result = which_system_uncached(name);

    if let Ok(mut cache) = which_cache().lock() {
        cache.insert(name.to_string(), result.clone());
    }
    result
}

fn which_system_uncached(name: &str) -> Option<PathBuf> {
    let output = Command::new("which").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if path.is_file() { Some(path) } else { None }
}

fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    fn which_none(_name: &str) -> Option<PathBuf> {
        None
    }

    fn which_system_dir(name: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/system/bin").join(name))
    }

    fn make_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn env_override_wins_over_bundled_and_system() {
        let bundled = tempfile::tempdir().unwrap();
        let bundled_path = make_file(bundled.path(), "tectonic");

        let resolved = resolve_with(
            "tectonic",
            Some(bundled_path.as_os_str()),
            None,
            &[],
            Some(bundled.path()),
            &which_system_dir,
        )
        .unwrap();

        assert_eq!(resolved.source, EngineSource::EnvOverride);
        assert_eq!(resolved.path, bundled_path);
    }

    #[test]
    fn bundled_wins_over_system() {
        let bundled = tempfile::tempdir().unwrap();
        let bundled_path = make_file(bundled.path(), "typst");

        let resolved = resolve_with(
            "typst",
            None,
            None,
            &[],
            Some(bundled.path()),
            &which_system_dir,
        )
        .unwrap();

        assert_eq!(resolved.source, EngineSource::Bundled);
        assert_eq!(resolved.path, bundled_path);
    }

    #[test]
    fn falls_back_to_which() {
        let resolved = resolve_with("tectonic", None, None, &[], None, &which_system_dir).unwrap();

        assert_eq!(resolved.source, EngineSource::System);
        assert_eq!(resolved.path, PathBuf::from("/system/bin/tectonic"));
    }

    #[test]
    fn falls_back_to_common_paths() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".cargo/bin")).unwrap();
        let cargo_bin = make_file(&home.path().join(".cargo/bin"), "typst");

        let resolved = resolve_with(
            "typst",
            None,
            Some(home.path()),
            &["~/.cargo/bin/typst"],
            None,
            &which_none,
        )
        .unwrap();

        assert_eq!(resolved.path, cargo_bin);
    }

    #[test]
    fn returns_none_when_nothing_found() {
        let resolved = resolve_with("tectonic", None, None, &[], None, &which_none);

        assert!(resolved.is_none());
    }

    #[test]
    fn which_system_caches_negative_results() {
        // Exercises the OnceLock+Mutex cache: the second lookup for a
        // missing binary must hit the cache and still return None.
        let first = which_system("graf-definitely-missing-binary-xyz");
        let second = which_system("graf-definitely-missing-binary-xyz");
        assert_eq!(first, None);
        assert_eq!(second, None);
    }

    #[test]
    fn expand_home_joins_home_directory() {
        assert_eq!(
            expand_home("~/bin/typst", Some(Path::new("/Users/me"))),
            PathBuf::from("/Users/me/bin/typst")
        );
        assert_eq!(
            expand_home("/opt/bin/typst", Some(Path::new("/Users/me"))),
            PathBuf::from("/opt/bin/typst")
        );
        assert_eq!(
            expand_home("~/bin/typst", None),
            PathBuf::from("~/bin/typst")
        );
    }
}
