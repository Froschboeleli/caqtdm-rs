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

    /// Read a display from a local file or from `http://`.
    pub fn read_text(&self, name: &str) -> Option<String> {
        if let Some(path) = self.find(name) {
            return std::fs::read_to_string(path).ok();
        }
        if name.starts_with("http://") {
            return fetch_http(name).ok();
        }
        None
    }
}

/// GET a text body from an `http://host[:port]/path` URL. HTTPS is not spoken.
pub fn fetch_http(url: &str) -> Result<String, crate::error::UiError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| crate::error::UiError::msg("only http:// display URLs are fetched"))?;
    let (hostport, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match hostport.split_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| crate::error::UiError::msg("bad http port"))?,
        ),
        None => (hostport, 80),
    };
    let mut stream = std::net::TcpStream::connect((host, port))
        .map_err(|err| crate::error::UiError::msg(format!("http connect: {err}")))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(800)))
        .ok();
    let request = format!("GET /{path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    std::io::Write::write_all(&mut stream, request.as_bytes())
        .map_err(|err| crate::error::UiError::msg(err.to_string()))?;
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(&mut stream, &mut buf);
    let body = buf
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    if !buf.starts_with("HTTP/1.") || !buf.contains(" 200 ") {
        return Err(crate::error::UiError::msg("http display fetch failed"));
    }
    Ok(body.to_string())
}
