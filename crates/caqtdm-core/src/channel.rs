/// A process-variable name after the caQtDM `plugin://name` split.
///
/// caQtDM (`caqtdm_lib.cpp`) treats the text before `://` as the control
/// plugin. `pva://` and `ca://` are aliases of the EPICS 4 plugin, and a bare
/// name uses the default plugin (normally `epics3`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelSpec {
    pub plugin: String,
    pub flavor: String,
    pub name: String,
    pub raw: String,
}

impl ChannelSpec {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }
}

pub fn parse_channel(raw: &str, default_plugin: &str) -> ChannelSpec {
    let trimmed = raw.trim();
    if let Some(pos) = trimmed.find("://") {
        let prefix = &trimmed[..pos];
        let name = trimmed[pos + 3..].trim().to_string();
        let (plugin, flavor) = classify_prefix(prefix);
        return ChannelSpec {
            plugin,
            flavor,
            name,
            raw: trimmed.to_string(),
        };
    }
    let plugin = default_plugin.trim();
    let flavor = if plugin.eq_ignore_ascii_case("epics4") || plugin.eq_ignore_ascii_case("pva") {
        "pva".to_string()
    } else {
        String::new()
    };
    ChannelSpec {
        plugin: if plugin.is_empty() {
            "epics3".to_string()
        } else {
            plugin.to_string()
        },
        flavor,
        name: trimmed.to_string(),
        raw: trimmed.to_string(),
    }
}

fn classify_prefix(prefix: &str) -> (String, String) {
    let p = prefix.trim();
    if p.eq_ignore_ascii_case("epics4") || p.eq_ignore_ascii_case("pva") {
        ("epics4".to_string(), "pva".to_string())
    } else if p.eq_ignore_ascii_case("ca") {
        ("epics4".to_string(), "ca".to_string())
    } else if p.eq_ignore_ascii_case("epics3") || p.eq_ignore_ascii_case("ca3") {
        ("epics3".to_string(), String::new())
    } else {
        (p.to_ascii_lowercase(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_match_caqtdm() {
        let c = parse_channel("epics3://SR:VAC:P1", "epics3");
        assert_eq!(c.plugin, "epics3");
        assert_eq!(c.name, "SR:VAC:P1");

        let c = parse_channel("pva://beam:current", "epics3");
        assert_eq!(c.plugin, "epics4");
        assert_eq!(c.flavor, "pva");

        let c = parse_channel("ca://beam:current", "epics3");
        assert_eq!(c.plugin, "epics4");
        assert_eq!(c.flavor, "ca");

        let c = parse_channel("ENV://USER", "epics3");
        assert_eq!(c.plugin, "env");
        assert_eq!(c.name, "USER");

        let c = parse_channel("plain:pv", "epics3");
        assert_eq!(c.plugin, "epics3");
        assert_eq!(c.name, "plain:pv");
    }
}
