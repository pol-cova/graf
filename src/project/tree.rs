//! Project filesystem tree and directory scanner (spec §49).
//!
//! Scans project directories, filters ignored paths (.git, target),
//! organizes hierarchical tree nodes, and detects LaTeX root documents.

use std::fs;
use std::path::{Path, PathBuf};

/// A node in the project filesystem tree.
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

/// The recognized category of a project file.
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
    /// Determines the file kind from its file extension.
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

    /// Returns the display icon for this file kind.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Latex => "📄",
            Self::Typst => "⚡",
            Self::Bibtex => "📚",
            Self::Style => "⚙️",
            Self::Image => "🖼️",
            Self::Pdf => "📕",
            Self::GrafCanvas => "🎨",
            Self::Other => "📝",
        }
    }
}

/// Project directory tree model.
#[derive(Debug, Clone)]
pub struct ProjectTree {
    root_path: PathBuf,
    root_node: FileNode,
    root_document: Option<PathBuf>,
}

impl ProjectTree {
    /// Scans a project directory and builds the tree hierarchy.
    pub fn scan(root_path: impl Into<PathBuf>) -> Self {
        let root_path = root_path.into();
        let name = root_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let children = scan_directory(&root_path);
        let root_document = detect_root_document(&root_path, &children);

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
        }
    }

    /// Returns the root path of the project.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Returns a reference to the root node of the tree.
    pub fn root_node(&self) -> &FileNode {
        &self.root_node
    }

    /// Returns the detected main root document (e.g. `main.tex`), if any.
    pub fn root_document(&self) -> Option<&Path> {
        self.root_document.as_deref()
    }
}

fn scan_directory(dir: &Path) -> Vec<FileNode> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files and build output directories
        if should_ignore(&file_name) {
            continue;
        }

        if path.is_dir() {
            let children = scan_directory(&path);
            entries.push(FileNode::Directory {
                path,
                name: file_name,
                is_expanded: false,
                children,
            });
        } else if path.is_file() {
            let kind = FileKind::from_path(&path);
            entries.push(FileNode::File {
                path,
                name: file_name,
                kind,
            });
        }
    }

    // Sort: directories first, then alphabetically
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

/// Detects the root LaTeX document in the project.
fn detect_root_document(root_dir: &Path, children: &[FileNode]) -> Option<PathBuf> {
    // 1. Direct match: main.tex or document.tex in root
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

    // 2. Search for file containing \documentclass
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

        let tree = ProjectTree::scan(dir);
        assert_eq!(tree.root_document(), Some(dir.join("main.tex").as_path()));

        if let FileNode::Directory { children, .. } = tree.root_node() {
            assert!(children.iter().any(|c| match c {
                FileNode::Directory { name, .. } => name == "sections",
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
            // Hidden files should be filtered out
            assert!(!children.iter().any(|c| match c {
                FileNode::File { name, .. } => name == ".hidden",
                _ => false,
            }));
        } else {
            panic!("Expected root node to be a Directory");
        }
    }
}
