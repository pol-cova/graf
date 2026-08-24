use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLicense {
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub description: &'static str,
    pub repository: &'static str,
}

/// Returns the direct Rust dependencies reviewed for the current lockfile.
pub fn direct_dependency_licenses() -> Vec<DependencyLicense> {
    vec![
        DependencyLicense {
            name: "GPUI",
            version: "0.2.2",
            license: "Apache-2.0",
            description: "Native GPU application framework",
            repository: "https://github.com/zed-industries/zed",
        },
        DependencyLicense {
            name: "GPUI Platform",
            version: "0.1.0",
            license: "Apache-2.0",
            description: "GPUI platform integration",
            repository: "https://github.com/zed-industries/zed",
        },
        DependencyLicense {
            name: "Serde and Serde JSON",
            version: "1.0.229 / 1.0.151",
            license: "MIT OR Apache-2.0",
            description: "Settings and document serialization",
            repository: "https://github.com/serde-rs/serde",
        },
        DependencyLicense {
            name: "Unicode Segmentation",
            version: "1.13.3",
            license: "MIT OR Apache-2.0",
            description: "Unicode text boundary handling",
            repository: "https://github.com/unicode-rs/unicode-segmentation",
        },
        DependencyLicense {
            name: "Log and Env Logger",
            version: "0.4.34 / 0.11.11",
            license: "MIT OR Apache-2.0",
            description: "Application logging",
            repository: "https://github.com/rust-cli/env_logger",
        },
        DependencyLicense {
            name: "Tempfile",
            version: "3.27.0",
            license: "MIT OR Apache-2.0",
            description: "Atomic persistence and temporary build files",
            repository: "https://github.com/Stebalien/tempfile",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_current_direct_dependencies() {
        let licenses = direct_dependency_licenses();
        assert_eq!(licenses.len(), 6);
        assert!(licenses.iter().any(|license| license.name == "GPUI"));
        assert!(
            licenses
                .iter()
                .any(|license| license.name == "Unicode Segmentation")
        );
        assert!(!licenses.iter().any(|license| license.name == "Tectonic"));
    }
}
