//! A small slice of Qt stylesheets: `color`, `background-color`, and `font-size`.

use caqtdm_core::Color;

pub fn css_color(sheet: &str, property: &str) -> Option<Color> {
    let value = css_value(sheet, property)?;
    Color::parse(value)
}

pub fn css_px(sheet: &str, property: &str) -> Option<i32> {
    let value = css_value(sheet, property)?;
    value.trim().trim_end_matches("px").trim().parse().ok()
}

fn css_value<'a>(sheet: &'a str, property: &str) -> Option<&'a str> {
    for decl in sheet.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(property) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}
