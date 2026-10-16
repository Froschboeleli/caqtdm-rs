//! Load upstream sample displays and compare the painted result with the
//! values those files actually contain.

use std::path::Path;

use caqtdm_core::{Color, MacroMap};
use caqtdm_datasource::Hub;
use caqtdm_paint::paint;
use caqtdm_ui::load_path;
use caqtdm_widgets::Session;

fn upstream(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream")
        .join(rel)
}

#[test]
fn upstream_edm_graphics_paint_matches_the_file() {
    let path = upstream("caQtDM_Viewer/parserEDM/graphics.edl");
    let display = load_path(&path).expect("graphics.edl");
    assert!(display.root.rect.width >= 500);
    assert!(display.root.rect.height >= 600);
    let labels: Vec<_> = display
        .root
        .children
        .iter()
        .filter(|node| node.class_name == "caLabel")
        .collect();
    assert!(
        labels
            .iter()
            .any(|node| node.prop_str("text") == Some("Hello")),
        "static text value from graphics.edl"
    );
    assert!(display
        .root
        .children
        .iter()
        .any(|node| node.class_name == "caPolyLine"));
    assert!(display
        .root
        .children
        .iter()
        .any(|node| node.class_name == "caGraphics"));
    assert!(display
        .root
        .children
        .iter()
        .all(|node| { node.class_name != "caLabel" || node.prop_str("text") != Some("nope") }));

    let mut session = Session::open(display.root, Hub::new("epics3"), &MacroMap::new());
    session.tick(0);
    let image = paint(&session.visuals());
    // Rectangle at (85, 358), outline, lineColor index 14 (black).
    let border = image.pixel(85, 358).unwrap();
    assert_eq!((border.r, border.g, border.b), (0, 0, 0));
    // The widget interior is the display background, not a filled rectangle.
    let inside = image.pixel(120, 400).unwrap();
    assert_ne!((inside.r, inside.g, inside.b), (0, 0, 0));
    // First polyline vertex is absolute (186, 100).
    let vertex = image.pixel(186, 100).unwrap();
    assert_eq!((vertex.r, vertex.g, vertex.b), (0, 0, 0));
    // "Hello" is drawn inside the label at (325, 247).
    let mut lit = false;
    for y in 247..260 {
        for x in 325..370 {
            if let Some(pixel) = image.pixel(x, y) {
                if pixel.r < 20 && pixel.g < 20 && pixel.b < 20 {
                    lit = true;
                }
            }
        }
    }
    assert!(lit, "Hello glyph was not painted");
}

#[test]
fn upstream_ui_and_prc_samples_render() {
    let ui = load_path(&upstream("caQtDM_Tests/test.ui")).unwrap();
    let mut macros = MacroMap::new();
    macros.insert("macro1", "SECTOR");
    macros.insert("macro2", "RF");
    macros.insert("xxx", "ok");
    let mut session = Session::open(ui.root, Hub::new("epics3"), &macros);
    session.tick(0);
    let visuals = session.visuals();
    let texts: Vec<_> = visuals.iter().map(|v| v.text.as_str()).collect();
    assert!(texts.contains(&"SECTOR"));
    assert!(texts.contains(&"RF"));
    assert!(texts.contains(&"ok"));
    let image = paint(&visuals);
    let label = visuals.iter().find(|v| v.text == "SECTOR").unwrap();
    let mut lit = false;
    for y in label.rect.y..label.rect.y + label.rect.height {
        for x in label.rect.x..label.rect.x + label.rect.width {
            if image.pixel(x, y) == Some(Color::rgb(20, 20, 20)) {
                lit = true;
            }
        }
    }
    assert!(lit, "substituted macro text was not painted");

    let prc = load_path(&upstream("caQtDM_Tests/prcshowcase.prc")).unwrap();
    assert!(prc.root.count_class("caLed") >= 1);
    assert!(prc.root.count_class("caSlider") >= 1);
    assert!(prc.root.count_class("caThermo") >= 1);
    assert!(prc.root.count_class("caTextEntry") >= 1);
    assert!(prc.root.count_class("caMenu") >= 1);
    assert!(prc.root.count_class("caChoice") >= 1);
    assert!(prc.root.count_class("caToggleButton") >= 1);
    let mut session = Session::open(prc.root, Hub::new("epics3"), &MacroMap::new());
    session.tick(0);
    let image = paint(&session.visuals());
    assert!(image.width > 100);
    assert!(image.height > 100);
    let comment = session
        .visuals()
        .into_iter()
        .find(|v| v.text.contains("Formatted readbacks"))
        .expect("comment text");
    assert_eq!((comment.foreground.r, comment.foreground.b), (0x00, 0x80));
}
