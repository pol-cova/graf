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

#[derive(Debug, Clone)]
pub struct ProjectTree {
    root_path: PathBuf,
    root_node: FileNode,
    root_document: Option<PathBuf>,
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

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn root_node(&self) -> &FileNode {
        &self.root_node
    }

    pub fn file_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        collect_file_paths(&self.root_node, &mut paths);
        paths
    }

    pub fn root_document(&self) -> Option<&Path> {
        self.root_document.as_deref()
    }

    pub fn toggle_directory(&mut self, path: &Path) -> bool {
        toggle_directory_node(&mut self.root_node, path)
    }
}

fn collect_file_paths(node: &FileNode, paths: &mut Vec<PathBuf>) {
    match node {
        FileNode::Directory { children, .. } => {
            for child in children {
                collect_file_paths(child, paths);
            }
        }
        FileNode::File { path, .. } => paths.push(path.clone()),
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

fn scan_directory(dir: &Path) -> Vec<FileNode> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

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
}
