use crate::plugins::manifest::{PluginCapability, PluginManifest};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
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

    pub fn register_plugin(&mut self, manifest: PluginManifest) {
        if !self.plugins.iter().any(|p| p.manifest.id == manifest.id) {
            self.plugins.push(LoadedPlugin {
                manifest,
                is_enabled: true,
            });
        }
    }

    pub fn scan_plugin_directory(&mut self) -> usize {
        let mut count = 0;
        if let Some(home) = crate::util::home_dir() {
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
                        self.register_plugin(manifest);
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
                if let PluginCapability::Formatter {
                    language,
                    command,
                    args,
                } = cap
                    && language.eq_ignore_ascii_case(lang)
                    && !command.is_empty()
                {
                    return run_formatter(command, args, text);
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

fn run_formatter(command: &str, args: &[String], text: &str) -> Option<String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let input = text.to_string();
    let mut stdin = child.stdin.take()?;
    std::thread::spawn(move || {
        stdin.write_all(input.as_bytes()).ok();
    });

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
                    command: String::new(),
                    args: vec![],
                },
                PluginCapability::Command {
                    id: "latex.prettify".to_string(),
                    title: "Prettify LaTeX".to_string(),
                },
            ],
        };

        host.register_plugin(manifest);
        assert_eq!(host.plugins.len(), 1);

        let commands = host.list_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "latex.prettify");
    }

    #[cfg(unix)]
    #[test]
    fn dispatch_format_runs_plugin_command() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = temp.path().join("upper.sh");
        std::fs::write(&script, "#!/bin/sh\ntr '[:lower:]' '[:upper:]'\n").expect("write script");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let mut host = PluginHost::new();
        host.register_plugin(PluginManifest {
            id: "graf.test.formatter".to_string(),
            name: "Upper Formatter".to_string(),
            version: "0.1.0".to_string(),
            description: None,
            author: None,
            entrypoint: PathBuf::from("plugin.wasm"),
            capabilities: vec![PluginCapability::Formatter {
                language: "latex".to_string(),
                command: script.display().to_string(),
                args: vec![],
            }],
        });

        let formatted = host.dispatch_format("latex", "hello formatter");
        assert_eq!(formatted.as_deref(), Some("HELLO FORMATTER"));
    }

    #[test]
    fn dispatch_format_ignores_plugins_without_commands() {
        let mut host = PluginHost::new();
        host.register_plugin(PluginManifest {
            id: "graf.latex.formatter".to_string(),
            name: "LaTeX Formatter".to_string(),
            version: "0.1.0".to_string(),
            description: None,
            author: None,
            entrypoint: PathBuf::from("latex_fmt.wasm"),
            capabilities: vec![PluginCapability::Formatter {
                language: "latex".to_string(),
                command: String::new(),
                args: vec![],
            }],
        });

        assert_eq!(host.dispatch_format("latex", "\\section{Hello}"), None);
    }
}
