use std::path::{Path, PathBuf};

/// Directories searched for included displays, images, and stylesheets.
/// `CAQTDM_DISPLAY_PATH` is a colon-separated list, the same variable caQtDM uses.
#[derive(Debug, Clone, Default)]
pub struct SearchPath {
    pub dirs: Vec<PathBuf>,
}

impl SearchPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(file: &Path) -> Self {
        Self::from_env_and_file(Some(file))
    }

    pub fn from_env_and_file(file: Option<&Path>) -> Self {
        let mut dirs = Vec::new();
        if let Some(file) = file {
            if let Some(parent) = file.parent() {
                if !parent.as_os_str().is_empty() {
                    dirs.push(parent.to_path_buf());
                }
            }
        }
        if let Ok(env) = std::env::var("CAQTDM_DISPLAY_PATH") {
            for part in env.split([':', ';']) {
                let part = part.trim();
                if !part.is_empty() {
                    dirs.push(PathBuf::from(part));
                }
            }
        }
        Self { dirs }
    }

    pub fn push(&mut self, dir: impl Into<PathBuf>) {
        self.dirs.push(dir.into());
    }

    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let raw = PathBuf::from(name);
        if raw.is_file() {
            return Some(raw);
        }
        for dir in &self.dirs {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}
