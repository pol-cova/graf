//! Plugin manifest metadata and capability definitions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Capabilities declared and exported by a Graf plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginCapability {
    Formatter { language: String },
    Linter,
    Command { id: String, title: String },
    Exporter { target_format: String },
}

/// Metadata and manifest definition for a Graf extension / plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub entrypoint: PathBuf,
    pub capabilities: Vec<PluginCapability>,
}

impl PluginManifest {
    /// Deserializes a manifest from JSON text.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serializes the manifest to formatted JSON text.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_roundtrip() {
        let manifest = PluginManifest {
            id: "graf.typst.formatter".to_string(),
            name: "Typst Auto-Formatter".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Formats Typst markup using typstyle".to_string()),
            author: Some("Graf Community".to_string()),
            entrypoint: PathBuf::from("plugin.wasm"),
            capabilities: vec![
                PluginCapability::Formatter {
                    language: "typst".to_string(),
                },
                PluginCapability::Command {
                    id: "typst.format".to_string(),
                    title: "Format Typst Document".to_string(),
                },
            ],
        };

        let json = manifest.to_json().expect("Serialization should succeed");
        let parsed = PluginManifest::from_json(&json).expect("Deserialization should succeed");
        assert_eq!(manifest, parsed);
    }
}
