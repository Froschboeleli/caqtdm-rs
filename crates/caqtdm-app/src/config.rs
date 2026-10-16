//! Operator settings file.
//!
//! caQtDM stores these choices in a Qt dialog. This file is the same set of
//! fields, one `key=value` per line, so a headless run can load and save them.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub control_system: String,
    pub ca: String,
    pub pva: String,
    pub macros: String,
    pub execute_shell: bool,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            control_system: "epics3".into(),
            ca: String::new(),
            pva: String::new(),
            macros: String::new(),
            execute_shell: false,
        }
    }
}

impl ConfigFile {
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "control_system" => config.control_system = value.trim().to_string(),
                "ca" => config.ca = value.trim().to_string(),
                "pva" => config.pva = value.trim().to_string(),
                "macros" => config.macros = value.trim().to_string(),
                "execute_shell" => {
                    config.execute_shell = matches!(value.trim(), "1" | "true" | "yes");
                }
                _ => {}
            }
        }
        config
    }

    pub fn to_text(&self) -> String {
        format!(
            "control_system={}\nca={}\npva={}\nmacros={}\nexecute_shell={}\n",
            self.control_system,
            self.ca,
            self.pva,
            self.macros,
            if self.execute_shell { "true" } else { "false" }
        )
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("config {}: {err}", path.display()))?;
        Ok(Self::parse(&text))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_text()).map_err(|err| err.to_string())
    }
}
