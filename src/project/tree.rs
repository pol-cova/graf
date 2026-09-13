use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileNode {
    Directory {
        path: PathBuf,
        name: String,
        is_expanded: bool,
        children: Vec<FileNode>,
    },
    File {
        path: PathBuf,
        name: String,
        kind: FileKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Latex,
    Typst,
    Bibtex,
    Style,
    Image,
    Pdf,
    GrafCanvas,
    Other,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("tex") => Self::Latex,
            Some("typ") => Self::Typst,
            Some("bib") => Self::Bibtex,
            Some("sty") | Some("cls") => Self::Style,
            Some("png") | Some("jpg") | Some("jpeg") | Some("svg") => Self::Image,
            Some("pdf") => Self::Pdf,
            Some("graf") => Self::GrafCanvas,
            _ => Self::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Latex => "TEX",
            Self::Typst => "TYP",
            Self::Bibtex => "BIB",
            Self::Style => "STY",
            Self::Image => "IMG",
            Self::Pdf => "PDF",
            Self::GrafCanvas => "GRF",
            Self::Other => "",
        }
    }
}

/// One flattened project file for QuickOpen. Prebuilt with the tree so
/// per-keystroke matching never walks nodes or clones `PathBuf` lists.
#[derive(Debug, Clone)]
pub struct QuickOpenEntry {
    /// Path relative to the project root, as displayed.
    pub relative: String,
    pub path: PathBuf,
    pub kind: FileKind,
}

impl QuickOpenEntry {
    /// Lowercased copy of `relative`, folded once at scan time.
    fn matches(&self, query_lower: &str) -> bool {
        self.relative.to_lowercase().contains(query_lower)
    }
}

impl From<&QuickOpenEntry> for QuickOpenEntry {
    fn from(e: &QuickOpenEntry) -> Self {
        e.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ProjectTree {
    root_path: PathBuf,
    root_node: FileNode,
    root_document: Option<PathBuf>,
    /// Files flattened with pre-verified data; QuickOpen serves matches from
    /// this list instead of re-walking the node tree per keystroke.
    quick_open_files: Vec<QuickOpenEntry>,
}

impl ProjectTree {
    pub fn scan(root_path: impl Into<PathBuf>) -> Self {
        let root_path = root_path.into();
        let name = root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let children = scan_directory(&root_path);
        let root_document = detect_root_document(&root_path, &children);

        let mut quick_open_files = Vec::new();
        flatten_quick_open_entries(&root_path, &children, &mut quick_open_files);

        let root_node = FileNode::Directory {
            path: root_path.clone(),
            name,
            is_expanded: true,
            children,
        };

        Self {
            root_path,
            root_node,
            root_document,
            quick_open_files,
        }
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn root_node(&self) -> &FileNode {
        &self.root_node
    }

    /// Up to `limit` entries whose relative path contains `query`
    /// (case-insensitive), in tree order.
    pub fn quick_open_matches(&self, query: &str, limit: usize) -> Vec<&QuickOpenEntry> {
        let query = query.trim();
        if query.is_empty() {
            return self.quick_open_files.iter().take(limit).collect();
        }
        let query_lower = query.to_lowercase();
        self.quick_open_files
            .iter()
            .filter(|entry| entry.matches(&query_lower))
            .take(limit)
            .collect()
    }

    pub fn root_document(&self) -> Option<&Path> {
        self.root_document.as_deref()
    }

    pub fn toggle_directory(&mut self, path: &Path) -> bool {
        toggle_directory_node(&mut self.root_node, path)
    }
}

/// Flattens the freshly scanned node tree into QuickOpen entries with
/// root-relative display paths.
fn flatten_quick_open_entries(
    root_path: &Path,
    children: &[FileNode],
    out: &mut Vec<QuickOpenEntry>,
) {
    for node in children {
        match node {
            FileNode::Directory { children, .. } => {
                flatten_quick_open_entries(root_path, children, out);
            }
            FileNode::File { path, kind, .. } => {
                let relative = path
                    .strip_prefix(root_path)
                    .unwrap_or(path)
                    .display()
                    .to_string();
                out.push(QuickOpenEntry {
                    relative,
                    path: path.clone(),
                    kind: *kind,
                });
            }
        }
    }
}

fn toggle_directory_node(node: &mut FileNode, path: &Path) -> bool {
    let FileNode::Directory {
        path: node_path,
        is_expanded,
        children,
        ..
    } = node
    else {
        return false;
    };

    if node_path == path {
        *is_expanded = !*is_expanded;
        return true;
    }

    children
        .iter_mut()
        .any(|child| toggle_directory_node(child, path))
}

/// How deep the scanner descends and how many nodes it produces in total.
/// Real writing projects stay far below; both caps keep a pathological tree
/// (symlink cycles, node_modules everywhere) from stalling startup.
const MAX_SCAN_DEPTH: usize = 12;
const MAX_SCAN_NODES: usize = 20_000;

fn scan_directory(dir: &Path) -> Vec<FileNode> {
    let mut nodes = 0;
    scan_directory_bounded(dir, 0, &mut nodes)
}

fn scan_directory_bounded(dir: &Path, depth: usize, budget: &mut usize) -> Vec<FileNode> {
    let mut entries = Vec::new();
    if depth >= MAX_SCAN_DEPTH || *budget >= MAX_SCAN_NODES {
        return entries;
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };

    let mut collected: Vec<DirEntryInfo> = Vec::new();
    struct DirEntryInfo {
        path: PathBuf,
        name: String,
        is_dir: bool,
        is_symlink: bool,
    }

    for entry in read_dir.flatten() {
        if collected.len() + *budget >= MAX_SCAN_NODES {
            break;
        }
        // file_type() avoids two extra stat() calls that `path.is_dir()` /
        // `path.is_file()` would have cost per entry on most filesystems.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if should_ignore(&name) {
            continue;
        }
        collected.push(DirEntryInfo {
            path,
            name,
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
        });
    }
    *budget += collected.len();

    for DirEntryInfo {
        path,
        name,
        is_dir,
        is_symlink,
    } in collected
    {
        // Symlinked directories can form cycles; skip them rather than
        // chasing an unbounded tree.
        if is_dir && !is_symlink {
            let children = scan_directory_bounded(&path, depth + 1, budget);
            entries.push(FileNode::Directory {
                path,
                name,
                is_expanded: false,
                children,
            });
        } else if !is_dir {
            let kind = FileKind::from_path(&path);
            entries.push(FileNode::File { path, name, kind });
        }
    }

    entries.sort_by(|a, b| match (a, b) {
        (FileNode::Directory { name: a, .. }, FileNode::Directory { name: b, .. }) => {
            a.to_lowercase().cmp(&b.to_lowercase())
        }
        (FileNode::Directory { .. }, FileNode::File { .. }) => std::cmp::Ordering::Less,
        (FileNode::File { .. }, FileNode::Directory { .. }) => std::cmp::Ordering::Greater,
        (FileNode::File { name: a, .. }, FileNode::File { name: b, .. }) => {
            a.to_lowercase().cmp(&b.to_lowercase())
        }
    });

    entries
}

fn should_ignore(name: &str) -> bool {
    name.starts_with('.')
        || name == "target"
        || name == "build"
        || name == "node_modules"
        || name.ends_with(".aux")
        || name.ends_with(".log")
        || name.ends_with(".fls")
        || name.ends_with(".fdb_latexmk")
        || name.ends_with(".synctex.gz")
}

fn detect_root_document(root_dir: &Path, children: &[FileNode]) -> Option<PathBuf> {
    let main_tex = root_dir.join("main.tex");
    if main_tex.exists() {
        return Some(main_tex);
    }
    let doc_tex = root_dir.join("document.tex");
    if doc_tex.exists() {
        return Some(doc_tex);
    }
    let paper_tex = root_dir.join("paper.tex");
    if paper_tex.exists() {
        return Some(paper_tex);
    }

    for child in children {
        if let FileNode::File {
            path,
            kind: FileKind::Latex,
            ..
        } = child
        {
            let is_root = fs::read_to_string(path).is_ok_and(|c| c.contains("\\documentclass"));
            if is_root {
                return Some(path.clone());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_kind_detection() {
        assert_eq!(FileKind::from_path(Path::new("paper.tex")), FileKind::Latex);
        assert_eq!(FileKind::from_path(Path::new("paper.typ")), FileKind::Typst);
        assert_eq!(FileKind::from_path(Path::new("refs.bib")), FileKind::Bibtex);
        assert_eq!(
            FileKind::from_path(Path::new("custom.sty")),
            FileKind::Style
        );
        assert_eq!(FileKind::from_path(Path::new("fig.png")), FileKind::Image);
        assert_eq!(FileKind::from_path(Path::new("doc.pdf")), FileKind::Pdf);
        assert_eq!(
            FileKind::from_path(Path::new("chart.graf")),
            FileKind::GrafCanvas
        );
        assert_eq!(
            FileKind::from_path(Path::new("readme.txt")),
            FileKind::Other
        );
    }

    #[test]
    fn test_project_tree_scan_and_root_detect() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        fs::create_dir_all(dir.join("sections")).unwrap();
        fs::write(
            dir.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\\input{sections/intro.tex}\\end{document}",
        )
        .unwrap();
        fs::write(dir.join("sections/intro.tex"), "Introduction section text.").unwrap();
        fs::write(dir.join("refs.bib"), "@article{key, title={Test}}").unwrap();
        fs::write(dir.join(".hidden"), "hidden").unwrap();

        let mut tree = ProjectTree::scan(dir);
        assert_eq!(tree.root_document(), Some(dir.join("main.tex").as_path()));
        assert!(tree.toggle_directory(&dir.join("sections")));

        if let FileNode::Directory { children, .. } = tree.root_node() {
            assert!(children.iter().any(|c| match c {
                FileNode::Directory {
                    name, is_expanded, ..
                } => name == "sections" && *is_expanded,
                _ => false,
            }));
            assert!(children.iter().any(|c| match c {
                FileNode::File { name, .. } => name == "main.tex",
                _ => false,
            }));
            assert!(children.iter().any(|c| match c {
                FileNode::File { name, .. } => name == "refs.bib",
                _ => false,
            }));
            assert!(!children.iter().any(|c| match c {
                FileNode::File { name, .. } => name == ".hidden",
                _ => false,
            }));
        } else {
            panic!("Expected root node to be a Directory");
        }
    }

    #[test]
    fn quick_open_matches_filters_and_caps() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join("deep/nested/path")).unwrap();
        fs::write(dir.join("intro.tex"), "x").unwrap();
        fs::write(dir.join("deep/nested/path/preview.png"), "x").unwrap();
        fs::write(dir.join("deep/nested/path/preface.typ"), "x").unwrap();

        let tree = ProjectTree::scan(dir);
        assert_eq!(tree.quick_open_matches("", 50).len(), 3);

        let pre = tree.quick_open_matches("PRE", 50);
        assert!(pre.iter().any(|e| e.relative.ends_with("preface.typ")));
        assert!(pre.iter().all(|e| e.relative.contains("pre")));

        // Limit applies even without a filter.
        assert_eq!(tree.quick_open_matches("", 2).len(), 2);
        assert!(tree.quick_open_matches("nothing matches", 50).is_empty());

        // Entries keep valid paths.
        let first = tree.quick_open_matches("intro", 50)[0].clone();
        assert_eq!(first.kind, FileKind::Latex);
        assert!(first.path.is_file());
    }

    #[test]
    fn scan_skips_gitignore_style_noise() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join(".git/objects")).unwrap();
        fs::create_dir_all(dir.join("target/debug")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join(".git/objects/abc"), "x").unwrap();
        fs::write(dir.join("target/debug/out"), "x").unwrap();
        fs::write(dir.join("main.tex"), "x").unwrap();

        let tree = ProjectTree::scan(dir);
        let paths: Vec<String> = tree
            .quick_open_matches("", 10_000)
            .iter()
            .map(|e| e.relative.to_string())
            .collect();

        assert_eq!(paths, vec!["main.tex".to_string()]);
    }

    #[test]
    fn scan_caps_depth_and_avoids_symlinked_directories() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join("a/b/c/d")).unwrap();
        fs::write(dir.join("a/b/c/d/extreme.tex"), "x").unwrap();
        // A cycle through a symlink must not hang or recurse forever.
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir, dir.join("loop")).unwrap();

        let tree = ProjectTree::scan(dir);
        assert!(matches!(tree.root_node(), FileNode::Directory { .. }));
        // No crash/hang is the assertion; verify the deep file is reachable
        // within the depth cap.
        assert!(
            tree.quick_open_matches("", 10_000)
                .iter()
                .any(|e| e.relative.ends_with("extreme.tex"))
        );
    }
}
