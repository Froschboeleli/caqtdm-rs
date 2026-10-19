use std::path::{Path, PathBuf};

/// Directories searched for displays, images, and stylesheets.
///
/// This follows `searchFile::findFile`: the name as given (cwd or absolute),
/// then each entry of `CAQTDM_DISPLAY_PATH`. The separator is `:` on Unix and
/// `;` on Windows, matching `pathdefinitions.h`. The directory of the display
/// that is open is tried last, which is the include fallback after the path.
#[derive(Debug, Clone, Default)]
pub struct SearchPath {
    /// `CAQTDM_DISPLAY_PATH`, then extra directories such as `-url`.
    pub dirs: Vec<PathBuf>,
    /// Directory of the display currently being expanded.
    pub origin: Option<PathBuf>,
}

impl SearchPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_env() -> Self {
        let dirs = std::env::var("CAQTDM_DISPLAY_PATH")
            .map(|value| split_display_path(&value))
            .unwrap_or_default();
        Self { dirs, origin: None }
    }

    pub fn from_file(file: &Path) -> Self {
        Self::from_env_and_file(Some(file))
    }

    pub fn from_env_and_file(file: Option<&Path>) -> Self {
        let mut path = Self::from_env();
        if let Some(file) = file {
            path.origin = directory_of(file);
        }
        path
    }

    pub fn push(&mut self, dir: impl Into<PathBuf>) {
        self.dirs.push(dir.into());
    }

    /// `searchFile::findFile`, then the open display's directory.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
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
        if let Some(origin) = &self.origin {
            let candidate = origin.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Include and related-display lookup. The original name is searched first.
    /// A missing `.adl` or `.edl` is tried again as `.ui`, and any other
    /// suffix is rewritten to `.ui`, matching `caInclude` in `caqtdm_lib.cpp`.
    pub fn resolve_include(&self, provided: &str) -> Option<PathBuf> {
        let provided = provided.trim();
        if provided.is_empty() {
            return None;
        }
        if let Some(found) = self.find(provided) {
            return Some(found);
        }
        let rewritten = rewrite_missing_include(provided);
        if rewritten != provided {
            return self.find(&rewritten);
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
        if name.starts_with("https://") {
            let anchors = trust_anchors_from_env();
            let borrowed: Vec<&[u8]> = anchors.iter().map(Vec::as_slice).collect();
            return fetch_https(name, &borrowed).ok();
        }
        None
    }
}

fn directory_of(file: &Path) -> Option<PathBuf> {
    let parent = file.parent()?;
    if parent.as_os_str().is_empty() {
        None
    } else {
        Some(parent.to_path_buf())
    }
}

/// `pathSeparator` in `pathdefinitions.h`. Windows entries may be quoted.
pub fn split_display_path(value: &str) -> Vec<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    value
        .split(separator)
        .filter_map(|part| {
            let part = if cfg!(windows) {
                part.replace('"', "")
            } else {
                part.to_string()
            };
            let part = part.trim();
            if part.is_empty() {
                None
            } else {
                Some(PathBuf::from(part))
            }
        })
        .collect()
}

fn rewrite_missing_include(name: &str) -> String {
    let path = Path::new(name);
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "prc" || ext == "ui" {
        return name.to_string();
    }
    let mut rewritten = PathBuf::from(name);
    rewritten.set_extension("ui");
    rewritten.to_string_lossy().into_owned()
}

fn trust_anchors_from_env() -> Vec<Vec<u8>> {
    let Ok(path) = std::env::var("CAQTDM_CA_FILE") else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    decode_certs(&bytes)
}

/// GET a text body from an `http://host[:port]/path` URL.
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

/// GET a text body from `https://host[:port]/path` using rustls.
/// `trust_der` is one or more DER trust anchors. Display search also reads
/// `CAQTDM_CA_FILE` (PEM or DER).
pub fn fetch_https(url: &str, trust_der: &[&[u8]]) -> Result<String, crate::error::UiError> {
    use std::io::{Read, Write};
    use std::sync::Arc;

    use rustls::pki_types::{CertificateDer, ServerName};
    use rustls::{ClientConfig, ClientConnection, RootCertStore};

    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| crate::error::UiError::msg("only https:// display URLs are fetched here"))?;
    let (hostport, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match hostport.split_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| crate::error::UiError::msg("bad https port"))?,
        ),
        None => (hostport, 443),
    };
    if trust_der.is_empty() {
        return Err(crate::error::UiError::msg(
            "https display fetch needs a trust anchor",
        ));
    }
    let mut roots = RootCertStore::empty();
    for der in trust_der {
        roots
            .add(CertificateDer::from(der.to_vec()))
            .map_err(|err| crate::error::UiError::msg(format!("trust anchor: {err}")))?;
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|_| crate::error::UiError::msg("bad https host"))?;
    let mut conn = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|err| crate::error::UiError::msg(format!("rustls: {err}")))?;
    let mut tcp = std::net::TcpStream::connect((host, port))
        .map_err(|err| crate::error::UiError::msg(format!("https connect: {err}")))?;
    tcp.set_read_timeout(Some(std::time::Duration::from_millis(1500)))
        .ok();
    let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
    let request = format!("GET /{path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    tls.write_all(request.as_bytes())
        .map_err(|err| crate::error::UiError::msg(format!("https write: {err}")))?;
    let mut buf = String::new();
    let _ = tls.read_to_string(&mut buf);
    let body = buf
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    if !buf.starts_with("HTTP/1.") || !buf.contains(" 200 ") {
        return Err(crate::error::UiError::msg("https display fetch failed"));
    }
    Ok(body.to_string())
}

fn decode_certs(bytes: &[u8]) -> Vec<Vec<u8>> {
    if !bytes.starts_with(b"-----") {
        return vec![bytes.to_vec()];
    }
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut b64 = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.contains("BEGIN CERTIFICATE") {
            inside = true;
            b64.clear();
            continue;
        }
        if line.contains("END CERTIFICATE") {
            if let Some(der) = base64_decode(&b64) {
                out.push(der);
            }
            inside = false;
            continue;
        }
        if inside {
            b64.push_str(line.trim());
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    fn val(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = text.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= bytes.len() + (4 - bytes.len() % 4) % 4 {
        if i >= bytes.len() {
            break;
        }
        let a = val(*bytes.get(i)?)?;
        let b = val(*bytes.get(i + 1)?)?;
        let c = bytes.get(i + 2).copied().and_then(val);
        let d = bytes.get(i + 3).copied().and_then(val);
        out.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            out.push((b << 4) | (c >> 2));
            if let Some(d) = d {
                out.push((c << 6) | d);
            }
        }
        i += 4;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_separator_matches_search_file() {
        let parts = split_display_path(if cfg!(windows) {
            r#""C:\one";"D:\two";"#
        } else {
            "/one:/two: :/three"
        });
        if cfg!(windows) {
            assert_eq!(
                parts,
                vec![PathBuf::from(r"C:\one"), PathBuf::from(r"D:\two")]
            );
        } else {
            assert_eq!(
                parts,
                vec![
                    PathBuf::from("/one"),
                    PathBuf::from("/two"),
                    PathBuf::from("/three")
                ]
            );
            assert_eq!(split_display_path("/a;/b").len(), 1);
        }
    }

    #[test]
    fn display_path_beats_the_including_directory() {
        let root = std::env::temp_dir().join(format!("caqtdm-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path_dir = root.join("path");
        let origin = root.join("origin");
        std::fs::create_dir_all(&path_dir).unwrap();
        std::fs::create_dir_all(&origin).unwrap();
        std::fs::write(path_dir.join("panel.ui"), "from-path").unwrap();
        std::fs::write(origin.join("panel.ui"), "from-origin").unwrap();
        std::fs::write(origin.join("sibling.ui"), "sib").unwrap();
        std::fs::write(origin.join("object_quad.ui"), "converted").unwrap();
        let search = SearchPath {
            dirs: vec![path_dir],
            origin: Some(origin),
        };
        let found = search.find("panel.ui").unwrap();
        assert_eq!(std::fs::read_to_string(&found).unwrap(), "from-path");
        assert_eq!(
            std::fs::read_to_string(search.find("sibling.ui").unwrap()).unwrap(),
            "sib"
        );
        let rewritten = search.resolve_include("object_quad.adl").unwrap();
        assert_eq!(std::fs::read_to_string(&rewritten).unwrap(), "converted");
        assert!(search.resolve_include("missing.adl").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
