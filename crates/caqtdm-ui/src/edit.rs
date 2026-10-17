//! In-place editor for Qt `.ui` and MEDM `.adl` text.
//!
//! Saving a file that was not edited returns the bytes that were loaded.
//! An edit replaces one property value and leaves every other byte in place.
//! Upstream's viewer does not write these files: Qt Designer and MEDM do.
//! This is the byte-preserving stand-in, not a second copy of Qt's writer.

use crate::error::UiError;

/// A display file kept as text so an edit can be a local replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayEditor {
    text: String,
}

impl DisplayEditor {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, UiError> {
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| UiError::msg("display file is not utf-8"))?;
        Ok(Self { text })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The file as it should be written. Unedited text is the input bytes.
    pub fn save(&self) -> Vec<u8> {
        self.text.as_bytes().to_vec()
    }

    /// Replace the `<string>` of one widget property. The rest of the file stays.
    pub fn set_ui_string(
        &mut self,
        widget: &str,
        property: &str,
        value: &str,
    ) -> Result<(), UiError> {
        self.text = replace_ui_string(&self.text, widget, property, value)?;
        Ok(())
    }

    /// Replace `field="value"` in an `.adl` file. The rest of the file stays.
    pub fn set_adl_field(&mut self, field: &str, value: &str) -> Result<(), UiError> {
        self.text = replace_adl_field(&self.text, field, value)?;
        Ok(())
    }
}

fn replace_ui_string(
    text: &str,
    widget: &str,
    property: &str,
    value: &str,
) -> Result<String, UiError> {
    let marker = format!("name=\"{widget}\"");
    let name_at = text
        .find(&marker)
        .ok_or_else(|| UiError::msg(format!("widget {widget} is not in the file")))?;
    let widget_start = text[..name_at]
        .rfind("<widget")
        .ok_or_else(|| UiError::msg(format!("widget {widget} has no start tag")))?;
    let widget_end = widget_start
        + end_element(&text[widget_start..], "widget")
            .ok_or_else(|| UiError::msg(format!("widget {widget} is not closed")))?;
    let body = &text[widget_start..widget_end];
    let prop = format!("<property name=\"{property}\">");
    let prop_at = body
        .find(&prop)
        .ok_or_else(|| UiError::msg(format!("{widget}.{property} is not in the file")))?;
    let after_prop = prop_at + prop.len();
    let close = body[after_prop..]
        .find("</property>")
        .ok_or_else(|| UiError::msg(format!("{widget}.{property} is not closed")))?
        + after_prop;
    let slice = &body[after_prop..close];
    let open = slice
        .find("<string>")
        .ok_or_else(|| UiError::msg(format!("{widget}.{property} has no string")))?;
    let inner = after_prop + open + "<string>".len();
    let end = text[widget_start + inner..]
        .find("</string>")
        .ok_or_else(|| UiError::msg(format!("{widget}.{property} string is not closed")))?;
    let mut out = String::with_capacity(text.len() + value.len());
    out.push_str(&text[..widget_start + inner]);
    out.push_str(&escape_xml(value));
    out.push_str(&text[widget_start + inner + end..]);
    Ok(out)
}

fn end_element(text: &str, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut depth = 0i32;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if text[i..].starts_with(&open) {
            depth += 1;
            i += open.len();
            continue;
        }
        if text[i..].starts_with(&close) {
            depth -= 1;
            i += close.len();
            if depth == 0 {
                return Some(i);
            }
            continue;
        }
        i += 1;
    }
    None
}

fn replace_adl_field(text: &str, field: &str, value: &str) -> Result<String, UiError> {
    let key = format!("{field}=\"");
    let start = text
        .find(&key)
        .ok_or_else(|| UiError::msg(format!("adl field {field} is not in the file")))?;
    let inner = start + key.len();
    let end = text[inner..]
        .find('"')
        .ok_or_else(|| UiError::msg(format!("adl field {field} is not quoted")))?;
    let mut out = String::with_capacity(text.len() + value.len());
    out.push_str(&text[..inner]);
    out.push_str(value);
    out.push_str(&text[inner + end..]);
    Ok(out)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_str;
    use std::path::Path;

    #[test]
    fn upstream_ui_round_trips_and_an_edit_keeps_the_other_bytes() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../upstream/caQtDM_Tests/test.ui");
        let original = std::fs::read(&path).expect("upstream test.ui");
        let mut editor = DisplayEditor::from_bytes(&original).unwrap();
        assert_eq!(editor.save(), original);

        editor.set_ui_string("calabel", "text", "kept").unwrap();
        let saved = editor.save();
        assert_ne!(saved, original);
        let old = original
            .windows(b"$(xxx)".len())
            .position(|window| window == b"$(xxx)")
            .unwrap();
        assert_eq!(&saved[..old], &original[..old]);
        assert_eq!(
            &saved[old + "kept".len()..],
            &original[old + "$(xxx)".len()..]
        );

        let display = load_str(editor.text(), Path::new("test.ui")).unwrap();
        let label = display
            .root
            .children
            .iter()
            .flat_map(|child| std::iter::once(child).chain(child.children.iter()))
            .find(|node| node.name == "calabel")
            .unwrap();
        assert_eq!(label.prop_str("text"), Some("kept"));
    }

    #[test]
    fn adl_round_trips_and_an_edit_keeps_the_other_bytes() {
        let original = b"\
file {
  name=\"vac.adl\"
}
text {
  object {
    x=8
    y=8
    width=40
    height=16
  }
  textix=\"Hello\"
}
";
        let mut editor = DisplayEditor::from_bytes(original).unwrap();
        assert_eq!(editor.save(), original);
        editor.set_adl_field("textix", "World").unwrap();
        let saved = editor.save();
        let old = original
            .windows(b"Hello".len())
            .position(|window| window == b"Hello")
            .unwrap();
        assert_eq!(&saved[..old], &original[..old]);
        assert_eq!(
            &saved[old + "World".len()..],
            &original[old + "Hello".len()..]
        );
        let display = load_str(std::str::from_utf8(&saved).unwrap(), Path::new("vac.adl")).unwrap();
        let label = display
            .root
            .children
            .iter()
            .find(|node| node.class_name == "caLabel")
            .unwrap();
        assert_eq!(label.prop_str("text"), Some("World"));
    }
}
