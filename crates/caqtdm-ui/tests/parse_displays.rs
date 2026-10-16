use std::path::Path;

use caqtdm_core::{parse_macro_list, PropValue};
use caqtdm_ui::{expand_includes, load_str, SearchPath, SourceFormat};

const LABEL_UI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ui version="4.0">
 <class>MainWindow</class>
 <widget class="QMainWindow" name="MainWindow">
  <property name="geometry">
   <rect><x>0</x><y>0</y><width>400</width><height>200</height></rect>
  </property>
  <property name="windowTitle"><string>Vacuum $(SYS)</string></property>
  <widget class="QWidget" name="centralwidget">
   <widget class="caLabel" name="title">
    <property name="geometry"><rect><x>10</x><y>12</y><width>120</width><height>24</height></rect></property>
    <property name="text"><string>Pressure $(SYS)</string></property>
    <property name="foreground"><color><red>0</red><green>0</green><blue>0</blue></color></property>
   </widget>
   <widget class="caLineEdit" name="value">
    <property name="geometry"><rect><x>140</x><y>12</y><width>80</width><height>24</height></rect></property>
    <property name="channel"><string>$(SYS):PRESSURE</string></property>
    <property name="colorMode"><enum>caLineEdit::colMode::Alarm</enum></property>
    <property name="formatType"><enum>caLineEdit::FormatType::decimal</enum></property>
    <property name="precision"><number>3</number></property>
   </widget>
   <layout class="QHBoxLayout" name="hbox">
    <property name="spacing"><number>4</number></property>
    <item>
     <widget class="QPushButton" name="apply">
      <property name="text"><string>Apply</string></property>
     </widget>
    </item>
    <item>
     <widget class="QPushButton" name="close">
      <property name="text"><string>Close</string></property>
     </widget>
    </item>
   </layout>
  </widget>
 </widget>
 <customwidgets>
  <customwidget>
   <class>caLabel</class>
   <extends>QLabel</extends>
   <header>caLabel</header>
  </customwidget>
 </customwidgets>
</ui>
"#;

#[test]
fn parses_ui_properties_colors_and_layout() {
    let display = load_str(LABEL_UI, Path::new("vacuum.ui")).unwrap();
    assert_eq!(display.source, SourceFormat::Ui);
    assert_eq!(display.class_name, "MainWindow");
    assert_eq!(display.custom_widgets.len(), 1);
    assert_eq!(display.root.count_class("caLabel"), 1);
    assert_eq!(display.root.count_class("caLineEdit"), 1);
    let mut found = false;
    display.root.walk(&mut |w| {
        if w.name == "value" {
            assert_eq!(w.enum_tail("colorMode"), Some("Alarm"));
            assert_eq!(w.prop_f64("precision"), Some(3.0));
            assert_eq!(w.prop_str("channel"), Some("$(SYS):PRESSURE"));
            found = true;
        }
        if w.name == "title" {
            match w.prop("foreground") {
                Some(PropValue::Color(c)) => assert_eq!((c.r, c.g, c.b), (0, 0, 0)),
                other => panic!("color {other:?}"),
            }
        }
    });
    assert!(found);
    let label = display
        .root
        .children
        .iter()
        .flat_map(|c| c.children.iter())
        .find(|w| w.name == "title");
    let label = label.expect("label");
    assert_eq!(label.rect, caqtdm_core::Rect::new(10, 12, 120, 24));
}

#[test]
fn adl_text_and_related_display() {
    let adl = r#"
file { name="vac.adl" version=030109 }
display {
  object { x=0 y=0 width=300 height=180 }
  clr=14
  bclr=0
}
"text" {
  object { x=8 y=8 width=80 height=20 }
  "basic attribute" { clr=14 bclr=2 }
  "dynamic attribute" { vis="if not zero" calc="A" }
  textix="Vacuum"
}
"text update" {
  object { x=100 y=8 width=90 height=20 }
  "dynamic attribute" { chan="SR:VAC:P" clr="alarm" }
}
"related display" {
  object { x=8 y=40 width=120 height=30 }
  label0="Details"
  name0="details.adl"
}
"#;
    let display = load_str(adl, Path::new("vac.adl")).unwrap();
    assert_eq!(display.source, SourceFormat::Adl);
    assert_eq!(display.root.count_class("caLabel"), 1);
    assert_eq!(display.root.count_class("caLineEdit"), 1);
    assert_eq!(display.root.count_class("caRelatedDisplay"), 1);
    display.root.walk(&mut |w| {
        if w.class_name == "caLineEdit" {
            assert_eq!(w.prop_str("channel"), Some("SR:VAC:P"));
            assert_eq!(w.enum_tail("colorMode"), Some("Alarm"));
        }
        if w.class_name == "caLabel" {
            assert_eq!(w.enum_tail("visibility"), Some("IfNotZero"));
            assert_eq!(w.prop_str("visibilityCalc"), Some("A"));
        }
    });
}

#[test]
fn edl_alh_and_prc() {
    let edl = r#"
4 0 1
beginScreenProperties
major 4
minor 0
release 1
w 320
h 160
title "RF"
endScreenProperties
object activeXTextClass
beginObjectProperties
x 4
y 6
w 100
h 20
controlPv "RF:AMPL"
fgAlarm 1
endObjectProperties
"#;
    let display = load_str(edl, Path::new("rf.edl")).unwrap();
    assert_eq!(display.root.rect.width, 320);
    assert_eq!(display.root.count_class("caLineEdit"), 1);

    let alh = r#"
GROUP NULL Vacuum
$GUIDANCE
watch the ion pump
$END
    CHANNEL P1 SR:VAC:P1 -----
GROUP NULL RF
    CHANNEL AMP RF:AMPL -----
"#;
    let alh_display = load_str(alh, Path::new("demo.alh")).unwrap();
    assert_eq!(alh_display.root.count_class("caFrame"), 2);
    assert_eq!(alh_display.root.count_class("caLineEdit"), 2);

    let prc = r##"
#!grid 2
#channel name type
comment comment -span 2 -fg "#ff00ff" magnets
$(A):SOL:1 formRead
$(A):SOL:1 formWrite
"##;
    let prc_display = load_str(prc, Path::new("mag.prc")).unwrap();
    assert_eq!(prc_display.root.count_class("caLabel"), 1);
    assert_eq!(prc_display.root.count_class("caLineEdit"), 1);
    assert_eq!(prc_display.root.count_class("caTextEntry"), 1);
}

#[test]
fn include_expands_with_macros() {
    let dir = std::env::temp_dir().join(format!("caqtdm-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let child = dir.join("child.ui");
    std::fs::write(
        &child,
        r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>80</width><height>24</height></rect></property>
 <widget class="caLabel" name="lab">
  <property name="geometry"><rect><x>0</x><y>0</y><width>80</width><height>24</height></rect></property>
  <property name="text"><string>$(DEV)</string></property>
 </widget>
</widget></ui>"#,
    )
    .unwrap();
    let parent = r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>200</width><height>40</height></rect></property>
 <widget class="caInclude" name="inc">
  <property name="geometry"><rect><x>0</x><y>0</y><width>80</width><height>24</height></rect></property>
  <property name="filename"><string>child.ui</string></property>
  <property name="numberOfItems"><number>2</number></property>
  <property name="stacking"><enum>caInclude::Stacking::Row</enum></property>
  <property name="macro"><string>DEV=SOL1;DEV=SOL2</string></property>
 </widget>
</widget></ui>"#;
    let mut display = load_str(parent, &dir.join("parent.ui")).unwrap();
    let mut search = SearchPath::new();
    search.push(&dir);
    expand_includes(&mut display.root, &search, &parse_macro_list("")).unwrap();
    let mut texts = Vec::new();
    display.root.walk(&mut |w| {
        if w.class_name == "caLabel" {
            texts.push(w.prop_str("text").unwrap_or("").to_string());
        }
    });
    assert_eq!(texts, vec!["SOL1".to_string(), "SOL2".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upstream_numeric_ui_parses_when_present() {
    let path = Path::new("upstream/caQtDM_Tests/numeric.ui");
    if !path.is_file() {
        return;
    }
    let text = std::fs::read_to_string(path).unwrap();
    let display = load_str(&text, path).unwrap();
    assert!(
        display.root.count_class("caNumeric") > 0,
        "expected caNumeric widgets"
    );
    assert!(display.root.count_class("caLineDraw") > 0);
    assert!(display.widget_count() > 20);
}
