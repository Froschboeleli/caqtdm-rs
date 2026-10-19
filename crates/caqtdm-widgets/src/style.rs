//! A slice of Qt stylesheets: `color`, `background`, `background-color`, and font size.
//!
//! A stylesheet set on a parent applies to descendants when the selector matches,
//! which is how `tests.ui` colors `caLineEdit` from the main window.

use caqtdm_core::Color;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CssStyle {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub font_px: Option<i32>,
}

impl CssStyle {
    fn merge(&mut self, other: &CssStyle) {
        if other.foreground.is_some() {
            self.foreground = other.foreground;
        }
        if other.background.is_some() {
            self.background = other.background;
        }
        if other.font_px.is_some() {
            self.font_px = other.font_px;
        }
    }
}

/// One widget on the path from the display root to the widget being painted.
pub struct WidgetIdent<'a> {
    pub class_name: &'a str,
    pub name: &'a str,
}

/// A stylesheet owned by `owner` (an index into the root-to-widget chain).
pub struct SheetSource<'a> {
    pub css: &'a str,
    pub owner: usize,
}

/// Rules from every ancestor sheet, then the widget's own sheet.
/// Later rules of equal or higher specificity win. A bare declaration list
/// (`color: red` with no selector) applies only to the widget that owns it.
pub fn cascade(chain: &[WidgetIdent<'_>], sheets: &[SheetSource<'_>]) -> CssStyle {
    if chain.is_empty() {
        return CssStyle::default();
    }
    let subject = chain.len() - 1;
    let mut ranked: Vec<(u16, u16, CssStyle)> = Vec::new();
    let mut order = 0u16;
    for sheet in sheets {
        let css = strip_comments(sheet.css);
        if !css.contains('{') {
            if sheet.owner == subject {
                ranked.push((1000, order, parse_decls(&css)));
                order = order.saturating_add(1);
            }
            continue;
        }
        for (selectors, body) in parse_blocks(&css) {
            let decl = parse_decls(body);
            if decl.foreground.is_none() && decl.background.is_none() && decl.font_px.is_none() {
                continue;
            }
            for selector in split_selectors(selectors) {
                let Some(parsed) = parse_selector(selector) else {
                    continue;
                };
                if !selector_matches(&parsed, chain) {
                    continue;
                }
                ranked.push((specificity(&parsed), order, decl.clone()));
                order = order.saturating_add(1);
            }
        }
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut style = CssStyle::default();
    for (_, _, decl) in ranked {
        style.merge(&decl);
    }
    style
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("*/") {
            rest = &rest[end + 2..];
        } else {
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn parse_blocks(css: &str) -> Vec<(&str, &str)> {
    let mut blocks = Vec::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let body = &rest[..close];
        rest = &rest[close + 1..];
        if !selector.is_empty() {
            blocks.push((selector, body));
        }
    }
    blocks
}

fn split_selectors(selectors: &str) -> Vec<&str> {
    selectors
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

struct SimpleSel {
    class_name: Option<String>,
    id: Option<String>,
    /// `:hover`, `:focus`, `::pane`, and other pieces this raster does not draw.
    skip: bool,
}

struct Selector {
    parts: Vec<SimpleSel>,
    /// `child[i]` is true when part i (i > 0) is joined with `>`.
    child: Vec<bool>,
}

fn parse_selector(text: &str) -> Option<Selector> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut child = Vec::new();
    let mut next_child = false;
    while !rest.is_empty() {
        if rest.starts_with('>') {
            next_child = true;
            rest = rest[1..].trim_start();
            continue;
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(rest.len());
        let compound = &rest[..end];
        rest = rest[end..].trim_start();
        if compound.is_empty() {
            continue;
        }
        child.push(next_child);
        next_child = false;
        parts.push(parse_compound(compound));
    }
    if parts.is_empty() {
        None
    } else {
        Some(Selector { parts, child })
    }
}

fn parse_compound(text: &str) -> SimpleSel {
    // `QTabWidget::pane` and `QTabBar::tab` style sub-controls, not the widget.
    let skip_sub = text.contains("::");
    let text = text.split("::").next().unwrap_or(text);
    let (text, skip_pseudo) = if let Some(index) = text.find(':') {
        (&text[..index], true)
    } else {
        (text, false)
    };
    let (class_name, id) = if let Some((ty, id)) = text.split_once('#') {
        (nonempty(ty), nonempty(id))
    } else if let Some(id) = text.strip_prefix('#') {
        (None, nonempty(id))
    } else {
        (nonempty(text), None)
    };
    SimpleSel {
        class_name,
        id,
        skip: skip_sub || skip_pseudo,
    }
}

fn nonempty(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn selector_matches(selector: &Selector, chain: &[WidgetIdent<'_>]) -> bool {
    if selector.parts.is_empty() || selector.parts.iter().any(|part| part.skip) {
        return false;
    }
    let mut index = chain.len() - 1;
    if !part_matches(selector.parts.last().unwrap(), &chain[index]) {
        return false;
    }
    for part_i in (0..selector.parts.len() - 1).rev() {
        let direct = selector.child.get(part_i + 1).copied().unwrap_or(false);
        if direct {
            if index == 0 {
                return false;
            }
            index -= 1;
            if !part_matches(&selector.parts[part_i], &chain[index]) {
                return false;
            }
        } else {
            let mut found = false;
            while index > 0 {
                index -= 1;
                if part_matches(&selector.parts[part_i], &chain[index]) {
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }
    }
    true
}

fn part_matches(part: &SimpleSel, widget: &WidgetIdent<'_>) -> bool {
    if let Some(id) = &part.id {
        if widget.name != id {
            return false;
        }
    }
    if let Some(class_name) = &part.class_name {
        if !type_matches(class_name, widget.class_name) {
            return false;
        }
    }
    part.class_name.is_some() || part.id.is_some()
}

/// Qt type selectors match the class and its subclasses. `QWidget` matches every widget.
fn type_matches(selector: &str, class_name: &str) -> bool {
    if selector == class_name || selector == "QWidget" || selector == "*" {
        return true;
    }
    matches!(
        (selector, class_name),
        ("QLabel", "caLabel" | "caLabelVertical")
            | ("QLineEdit", "caLineEdit" | "caTextEntry")
            | ("caLineEdit", "caTextEntry")
            | ("QFrame", "caFrame" | "QGroupBox")
            | ("QTabWidget", "caDoubleTabWidget")
            | (
                "QAbstractButton",
                "QPushButton" | "QToolButton" | "caMessageButton" | "caToggleButton"
            )
            | ("QPushButton", "caMessageButton")
            | ("QComboBox", "caMenu" | "caChoice")
    )
}

fn specificity(selector: &Selector) -> u16 {
    let mut score = 0u16;
    for part in &selector.parts {
        if part.id.is_some() {
            score = score.saturating_add(100);
        }
        if part.class_name.is_some() {
            score = score.saturating_add(10);
        }
    }
    score
}

fn parse_decls(body: &str) -> CssStyle {
    let mut style = CssStyle::default();
    for decl in body.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim().trim_end_matches("!important").trim();
        if value.is_empty() {
            continue;
        }
        match name.to_ascii_lowercase().as_str() {
            "color" => style.foreground = parse_paint_color(value),
            "background-color" | "background" => {
                if let Some(color) = parse_paint_color(value) {
                    style.background = Some(color);
                }
            }
            "font-size" => style.font_px = parse_css_size(value),
            "font" => {
                if let Some(px) = font_shorthand_size(value) {
                    style.font_px = Some(px);
                }
            }
            _ => {}
        }
    }
    style
}

/// `QTabWidget::tab > QWidget > QWidget` is how tests.ui fills the page.
/// It is not the tab button. `QTabBar::tab` stays a sub-control.
pub fn tab_page_background(css: &str) -> Option<Color> {
    let css = strip_comments(css);
    let mut found = None;
    for (selectors, body) in parse_blocks(&css) {
        let page_rule = split_selectors(selectors).iter().any(|selector| {
            let folded = selector.to_ascii_lowercase();
            folded.contains("::tab") && folded.contains("qwidget") && !folded.contains("qtabbar")
        });
        if !page_rule {
            continue;
        }
        if let Some(color) = parse_decls(body).background {
            found = Some(color);
        }
    }
    found
}

fn parse_paint_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.starts_with("qlineargradient")
        || value.starts_with("qradialgradient")
        || value.starts_with("url(")
    {
        return None;
    }
    Color::parse(value)
}

fn parse_css_size(value: &str) -> Option<i32> {
    let value = value.trim();
    if let Some(number) = value.strip_suffix("px") {
        return number
            .trim()
            .parse::<f64>()
            .ok()
            .map(|px| px.round() as i32);
    }
    if let Some(number) = value.strip_suffix("pt") {
        let pt: f64 = number.trim().parse().ok()?;
        return Some((pt * 96.0 / 72.0).round() as i32);
    }
    value.parse().ok()
}

fn font_shorthand_size(value: &str) -> Option<i32> {
    value.split_whitespace().find_map(parse_css_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_class_rule_reaches_the_child() {
        let chain = [
            WidgetIdent {
                class_name: "QMainWindow",
                name: "MainWindow",
            },
            WidgetIdent {
                class_name: "QWidget",
                name: "centralwidget",
            },
            WidgetIdent {
                class_name: "caLineEdit",
                name: "edit",
            },
        ];
        let sheet = "QWidget#centralwidget {background: rgb(58, 94, 171);}
 caLineEdit { background: lightyellow; color: black; }
 caTextEntry { color: rgb(127, 0, 63); background-color: cornsilk; }";
        let style = cascade(
            &chain,
            &[SheetSource {
                css: sheet,
                owner: 0,
            }],
        );
        assert_eq!(style.background, Some(Color::rgb(255, 255, 224)));
        assert_eq!(style.foreground, Some(Color::rgb(0, 0, 0)));
        let page = cascade(
            &chain[..2],
            &[SheetSource {
                css: sheet,
                owner: 0,
            }],
        );
        assert_eq!(page.background, Some(Color::rgb(58, 94, 171)));
    }

    #[test]
    fn subcontrol_rules_do_not_paint_the_widget() {
        let chain = [WidgetIdent {
            class_name: "caSlider",
            name: "slider",
        }];
        let style = cascade(
            &chain,
            &[SheetSource {
                css: "caSlider::groove:horizontal { background: red; }",
                owner: 0,
            }],
        );
        assert_eq!(style.background, None);
    }

    #[test]
    fn font_shorthand_is_point_size_at_96dpi() {
        let chain = [WidgetIdent {
            class_name: "caTable",
            name: "table",
        }];
        let style = cascade(
            &chain,
            &[SheetSource {
                css: "caTable { font: 10pt; background: cornsilk; }",
                owner: 0,
            }],
        );
        assert_eq!(style.font_px, Some(13));
        assert_eq!(style.background, Some(Color::rgb(255, 248, 220)));
    }

    #[test]
    fn tab_page_rule_is_wheat_and_not_a_widget_fill() {
        let css = "QTabWidget::tab > QWidget > QWidget {background: wheat;}\nQTabBar::tab { background: red; }";
        assert_eq!(tab_page_background(css), Some(Color::rgb(245, 222, 179)));
        let chain = [WidgetIdent {
            class_name: "QTabWidget",
            name: "tabs",
        }];
        let style = cascade(&chain, &[SheetSource { css, owner: 0 }]);
        assert_eq!(style.background, None);
    }
}
