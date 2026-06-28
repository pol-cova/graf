//! Third-party open-source license audit and manifest (spec §6, §7.6, M7).

use serde::{Deserialize, Serialize};

/// Metadata describing an open-source dependency license.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLicense {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub description: &'static str,
    pub repository: &'static str,
}

/// Returns the complete list of audited third-party dependencies used in Graf.
pub fn audited_licenses() -> Vec<DependencyLicense> {
    vec![
        DependencyLicense {
            name: "GPUI",
            version: "0.1.0",
            license: "Apache-2.0 / MIT",
            description: "High-performance GPU-accelerated immediate-mode UI framework",
            repository: "https://github.com/zed-industries/zed",
        },
        DependencyLicense {
            name: "Tectonic",
            version: "0.15.0",
            license: "MIT",
            description: "Complete, modernized, self-contained TeX/LaTeX typesetting engine",
            repository: "https://github.com/tectonic-typesetting/tectonic",
        },
        DependencyLicense {
            name: "Typst",
            version: "0.12.0",
            license: "Apache-2.0",
            description: "Markup-based typesetting system that is powerful and easy to learn",
            repository: "https://github.com/typst/typst",
        },
        DependencyLicense {
            name: "Serde & Serde JSON",
            version: "1.0.217",
            license: "MIT / Apache-2.0",
            description: "Efficient zero-copy JSON and binary serialization framework for Rust",
            repository: "https://github.com/serde-rs/serde",
        },
        DependencyLicense {
            name: "Unicode Segmentation",
            version: "1.12.0",
            license: "MIT / Apache-2.0",
            description: "Grapheme cluster, word, and sentence boundary iteration for Unicode text",
            repository: "https://github.com/unicode-rs/unicode-segmentation",
        },
        DependencyLicense {
            name: "Log & Env Logger",
            version: "0.4 / 0.11",
            license: "MIT / Apache-2.0",
            description: "Lightweight structured logging facade and terminal formatting",
            repository: "https://github.com/rust-lang/log",
        },
        DependencyLicense {
            name: "Tempfile",
            version: "3.14.0",
            license: "MIT / Apache-2.0",
            description: "Secure cross-platform temporary file and directory management",
            repository: "https://github.com/Stebalien/tempfile",
        },
        DependencyLicense {
            name: "Agent Client Protocol",
            version: "1.0",
            license: "Apache-2.0",
            description: "Standard JSON-RPC 2.0 protocol for code editors and AI agents",
            repository: "https://github.com/agentclientprotocol/agent-client-protocol",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audited_licenses() {
        let licenses = audited_licenses();
        assert!(licenses.len() >= 8);
        assert!(licenses.iter().any(|l| l.name == "GPUI"));
        assert!(licenses.iter().any(|l| l.name == "Tectonic"));
        assert!(licenses.iter().any(|l| l.name == "Typst"));
        assert!(licenses.iter().any(|l| l.name == "Agent Client Protocol"));
    }
}
