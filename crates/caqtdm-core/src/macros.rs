use std::collections::BTreeMap;

/// Ordered macro table. Later definitions override earlier ones with the same key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MacroMap {
    pairs: BTreeMap<String, String>,
}

impl MacroMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.pairs.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs.get(key).map(String::as_str)
    }

    pub fn extend_from(&mut self, other: &MacroMap) {
        for (k, v) in &other.pairs {
            self.pairs.insert(k.clone(), v.clone());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Parse `A=1,B=two,C="has spaces"` and also semicolon-separated lists.
/// A bare token without `=` is ignored.
pub fn parse_macro_list(text: &str) -> MacroMap {
    let mut map = MacroMap::new();
    for piece in split_macro_pieces(text) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some((k, v)) = piece.split_once('=') {
            let key = k.trim();
            if key.is_empty() {
                continue;
            }
            map.insert(key, unquote(v.trim()));
        }
    }
    map
}

fn split_macro_pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in text.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                cur.push(ch);
            }
            ',' | ';' if !in_quote => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Replace `$(name)`, `$(name=default)`, and `${name}` in `text`.
/// An unknown macro without a default is left unchanged, which is what
/// caQtDM does so a missing substitution stays visible on the display.
pub fn apply_macros(text: &str, macros: &MacroMap) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let open = after.as_bytes().get(1).copied();
        if open == Some(b'(') || open == Some(b'{') {
            let close = if open == Some(b'(') { ')' } else { '}' };
            if let Some(end) = after[2..].find(close) {
                let inner = &after[2..2 + end];
                let (name, default) = split_default(inner);
                if let Some(value) = macros.get(name.trim()) {
                    out.push_str(value);
                } else if let Some(def) = default {
                    out.push_str(def.trim());
                } else {
                    out.push_str(&after[..2 + end + 1]);
                }
                rest = &after[2 + end + 1..];
                continue;
            }
        }
        out.push('$');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

fn split_default(inner: &str) -> (&str, Option<&str>) {
    if let Some((name, def)) = inner.split_once('=') {
        (name, Some(def))
    } else {
        (inner, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_and_keeps_unknown() {
        let macros = parse_macro_list(r#"A=QUAD, B="linac 1", C=3"#);
        assert_eq!(apply_macros("$(A):CURR", &macros), "QUAD:CURR");
        assert_eq!(apply_macros("$(missing)", &macros), "$(missing)");
        assert_eq!(apply_macros("$(missing=fallback)", &macros), "fallback");
        assert_eq!(apply_macros("${B}", &macros), "linac 1");
    }
}
