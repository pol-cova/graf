use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub struct TemporarySessionDir {
    path: PathBuf,
    _managed: Option<tempfile::TempDir>,
    /// Set when we created `path` ourselves; removal happens on drop so a
    /// fallback root is never left behind by a session.
    self_created: bool,
}

impl Drop for TemporarySessionDir {
    fn drop(&mut self) {
        if self.self_created {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

impl TemporarySessionDir {
    pub fn new(prefix: &str) -> Self {
        match tempfile::Builder::new().prefix(prefix).tempdir() {
            Ok(dir) => Self {
                path: dir.path().to_path_buf(),
                _managed: Some(dir),
                self_created: false,
            },
            Err(error) => {
                // Fallback roots must (a) actually exist downstream and
                // (b) be unique within this process; both are cheap here.
                let path = std::env::temp_dir().join(format!(
                    "{prefix}_{}_{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos())
                        .unwrap_or(0)
                ));
                if let Err(create_error) = std::fs::create_dir_all(&path) {
                    log::warn!(
                        "failed to create fallback directory {}: {create_error} (original tempfile error: {error})",
                        path.display()
                    );
                }
                Self {
                    path,
                    _managed: None,
                    self_created: true,
                }
            }
        }
    }

    /// Test-only: injects an existing path instead of allocating one.
    #[cfg(test)]
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _managed: None,
            self_created: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Keep the newest `keep` subdirectories named `{prefix}<number>` under `dir`.
/// Among the rest, delete only directories whose newest content has been idle
/// for at least `min_idle`, so a compile or render still running in another
/// thread never loses its files.
pub fn prune_numbered_dirs(dir: &Path, prefix: &str, keep: usize, min_idle: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut runs: Vec<(u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry
                .file_name()
                .to_string_lossy()
                .strip_prefix(prefix)?
                .parse()
                .ok()?;
            Some((id, entry.path()))
        })
        .collect();
    runs.sort_unstable();

    let now = SystemTime::now();
    let newest_to_keep = runs.len().saturating_sub(keep);
    for (index, (_, path)) in runs.into_iter().enumerate() {
        if index >= newest_to_keep {
            break;
        }
        // An empty run dir has no newest mtime on record; treat it as
        // idle-eligible or empty runs accumulate forever.
        match newest_mtime(&path) {
            Some(modified) if now.duration_since(modified).unwrap_or_default() >= min_idle => {
                let _ = std::fs::remove_dir_all(path);
            }
            None => {
                let _ = std::fs::remove_dir_all(path);
            }
            _ => continue,
        }
    }
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    // Only content counts. The directory's own mtime refreshes on every write
    // inside it, which would hide the age of finished work.
    std::fs::read_dir(path)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_page(dir: &Path, name: &str) -> PathBuf {
        let run = dir.join(name);
        std::fs::create_dir_all(&run).unwrap();
        let page = run.join("page-1.png");
        std::fs::write(&page, b"png").unwrap();
        page
    }

    #[test]
    fn prune_deletes_old_dirs_beyond_keep() {
        let dir = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        for id in 1..=5 {
            let page = write_page(dir.path(), &format!("render_{id}"));
            std::fs::OpenOptions::new()
                .write(true)
                .open(&page)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        std::fs::create_dir_all(dir.path().join("unrelated")).unwrap();

        prune_numbered_dirs(dir.path(), "render_", 2, Duration::from_secs(60));

        let remaining: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert!(remaining.contains(&"render_4".to_string()));
        assert!(remaining.contains(&"render_5".to_string()));
        assert!(remaining.contains(&"unrelated".to_string()));
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn prune_spares_recent_dirs_even_beyond_keep() {
        // Everything is fresh, as it would be while compiles are in flight.
        let dir = tempfile::tempdir().unwrap();
        for id in 1..=5 {
            write_page(dir.path(), &format!("render_{id}"));
        }

        prune_numbered_dirs(dir.path(), "render_", 2, Duration::from_secs(60));

        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 5);
    }

    #[test]
    fn prune_tolerates_missing_dir() {
        prune_numbered_dirs(
            Path::new("/nonexistent/graf"),
            "render_",
            2,
            Duration::from_secs(60),
        );
    }
}
