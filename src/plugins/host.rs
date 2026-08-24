use std::path::PathBuf;

use crate::plugins::manifest::{PluginCapability, PluginManifest};

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub base_path: PathBuf,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PluginHost {
    pub plugins: Vec<LoadedPlugin>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register_plugin(&mut self, manifest: PluginManifest, base_path: PathBuf) {
        if !self.plugins.iter().any(|p| p.manifest.id == manifest.id) {
            self.plugins.push(LoadedPlugin {
                manifest,
                base_path,
                is_enabled: true,
            });
        }
    }

    pub fn scan_plugin_directory(&mut self) -> usize {
        let mut count = 0;
        if let Some(home) = dirs::home_dir() {
            let plugins_dir = home.join(".graf/plugins");
            if plugins_dir.is_dir()
                && let Ok(entries) = std::fs::read_dir(&plugins_dir)
            {
                for entry in entries.flatten() {
                    let manifest_file = entry.path().join("plugin.json");
                    let manifest_res = std::fs::read_to_string(&manifest_file)
                        .ok()
                        .and_then(|c| PluginManifest::from_json(&c).ok());

                    if let Some(manifest) = manifest_res {
                        self.register_plugin(manifest, entry.path());
                        count += 1;
                    }
                }
            }
        }
        count
    }

    pub fn dispatch_format(&self, lang: &str, text: &str) -> Option<String> {
        for plugin in &self.plugins {
            if !plugin.is_enabled {
                continue;
            }
            for cap in &plugin.manifest.capabilities {
                if matches!(cap, PluginCapability::Formatter { language } if language.eq_ignore_ascii_case(lang))
                {
                    return Some(text.trim().to_string());
                }
            }
        }
        None
    }

    pub fn list_commands(&self) -> Vec<(String, String)> {
        let mut cmds = Vec::new();
        for plugin in &self.plugins {
            if !plugin.is_enabled {
                continue;
            }
            for cap in &plugin.manifest.capabilities {
                if let PluginCapability::Command { id, title } = cap {
                    cmds.push((id.clone(), format!("{} ({})", title, plugin.manifest.name)));
                }
            }
        }
        cmds
    }
}

mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_host_registration_and_dispatch() {
        let mut host = PluginHost::new();
        let manifest = PluginManifest {
            id: "graf.latex.formatter".to_string(),
            name: "LaTeX Formatter".to_string(),
            version: "0.1.0".to_string(),
            description: None,
            author: None,
            entrypoint: PathBuf::from("latex_fmt.wasm"),
            capabilities: vec![
                PluginCapability::Formatter {
                    language: "latex".to_string(),
                },
                PluginCapability::Command {
                    id: "latex.prettify".to_string(),
                    title: "Prettify LaTeX".to_string(),
                },
            ],
        };

        host.register_plugin(manifest, PathBuf::from("/mock/plugin"));
        assert_eq!(host.plugins.len(), 1);

        let commands = host.list_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "latex.prettify");

        let formatted = host.dispatch_format("latex", "  \\section{Hello}  \n");
        assert_eq!(formatted.as_deref(), Some("\\section{Hello}"));
    }
}
