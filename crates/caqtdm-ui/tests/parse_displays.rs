use std::path::{Path, PathBuf};

use caqtdm_core::{parse_macro_list, PropValue};
use caqtdm_ui::{expand_includes, load_path, load_str, SearchPath, SourceFormat};

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
valuator {
  object { x=8 y=80 width=160 height=20 }
  control { chan="SR:VAC:SET" lopr="0" hopr="10" }
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
        if w.class_name == "caSlider" {
            assert_eq!(w.prop_str("channel"), Some("SR:VAC:SET"));
            assert_eq!(w.prop_f64("minValue"), Some(0.0));
            assert_eq!(w.prop_f64("maxValue"), Some(10.0));
            assert_eq!(w.enum_tail("limitsMode"), Some("User"));
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
object activeRelatedDisplayClass
beginObjectProperties
x 4
y 40
w 80
h 24
displayFileName "gauges.ui"
buttonLabel "Gauges"
endObjectProperties
"#;
    let display = load_str(edl, Path::new("rf.edl")).unwrap();
    assert_eq!(display.root.rect.width, 320);
    assert_eq!(display.root.count_class("caLabel"), 1);
    assert_eq!(display.root.count_class("caRelatedDisplay"), 1);
    display.root.walk(&mut |w| {
        if w.class_name == "caRelatedDisplay" {
            assert_eq!(w.prop_str("files"), Some("gauges.ui"));
        }
    });

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
    let warnings = expand_includes(&mut display.root, &search, &parse_macro_list("")).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
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

#[test]
fn less_common_adl_edl_alh_and_prc_objects() {
    let adl = r#"
composite {
  object { x=1 y=2 width=40 height=30 }
  "image name"="mark.ppm"
  text { textix="inside" object { x=3 y=4 width=10 height=8 } }
  scale { object { x=0 y=12 width=20 height=8 } }
}
"#;
    let display = load_str(adl, Path::new("extra.adl")).unwrap();
    assert_eq!(display.root.children[0].class_name, "caFrame");
    assert!(display.root.children[0]
        .children
        .iter()
        .any(|child| child.class_name == "caLabel"));
    assert!(display.root.children[0]
        .children
        .iter()
        .any(|child| child.class_name == "caLinearGauge"));

    let edl = r#"
beginScreenProperties
major 4
minor 0
endScreenProperties
object activeByteClass
beginObjectProperties
x 1
y 2
w 40
h 16
controlPv "RF:BITS"
endObjectProperties
object activeImageClass
beginObjectProperties
x 0
y 0
w 8
h 8
file "cam.gif"
endObjectProperties
object shellCmdClass
beginObjectProperties
x 0
y 20
w 40
h 16
command "echo hi"
endObjectProperties
"#;
    let display = load_str(edl, Path::new("extra.edl")).unwrap();
    assert_eq!(display.root.count_class("caByte"), 1);
    assert_eq!(display.root.count_class("caImage"), 1);
    assert_eq!(display.root.count_class("caShellCommand"), 1);

    let alh = "GROUP NULL vac\nCHANNEL p intern://P -\nALIAS vac:p\nFORCEPV intern://FORCE\n";
    let display = load_str(alh, Path::new("extra.alh")).unwrap();
    let mut alias = None;
    display.root.walk(&mut |node| {
        if node.class_name == "caLineEdit" {
            alias = node.prop_str("alias").map(str::to_string);
            assert_eq!(node.prop_str("forcePv"), Some("intern://FORCE"));
        }
    });
    assert_eq!(alias.as_deref(), Some("vac:p"));

    let prc = "#!grid 2\nVAC:LED formLed\nVAC:SET formMenu off on\n";
    let display = load_str(prc, Path::new("extra.prc")).unwrap();
    assert_eq!(display.root.count_class("caLed"), 1);
    let menu = display
        .root
        .children
        .iter()
        .find(|node| node.class_name == "caMenu")
        .unwrap();
    assert_eq!(menu.prop_str("items"), Some("off,on"));
}

#[test]
fn http_display_fetch() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = std::io::Read::read(&mut stream, &mut [0u8; 512]);
            let body = b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nhello";
            let _ = std::io::Write::write_all(&mut stream, body);
        }
    });
    let text = caqtdm_ui::fetch_http(&format!("http://{addr}/panel.ui")).unwrap();
    assert_eq!(text, "hello");
}

#[test]
fn unrecognized_medm_and_edm_objects_follow_upstream() {
    let adl = r#"
mystery {
  object { x=1 y=1 width=8 height=8 }
  textix="should not appear"
}
button {
  object { x=0 y=0 width=40 height=16 }
}
falling line {
  object { x=0 y=20 width=10 height=10 }
}
"#;
    let display = load_str(adl, Path::new("odd.adl")).unwrap();
    assert_eq!(display.root.children.len(), 2);
    assert_eq!(display.root.count_class("caChoice"), 1);
    assert_eq!(display.root.count_class("caPolyLine"), 1);
    assert!(!display
        .root
        .children
        .iter()
        .any(|child| child.class_name == "caGraphics"));

    let edl = r#"
4 0 1
beginScreenProperties
w 200
h 200
endScreenProperties
object activeXTextClass
beginObjectProperties
x 10
y 12
w 40
h 16
value {
  "Hello"
}
endObjectProperties
object notARealClass
beginObjectProperties
x 0
y 0
w 10
h 10
value "nope"
endObjectProperties
object activeMessageBoxClass
beginObjectProperties
x 0
y 0
w 10
h 10
endObjectProperties
object activeExitButtonClass
beginObjectProperties
x 0
y 30
w 10
h 10
endObjectProperties
object activeRadioButtonClass
beginObjectProperties
x 0
y 40
w 80
h 40
controlPv "RF:MODE"
orientation 0
endObjectProperties
object ByteClass
beginObjectProperties
x 0
y 80
w 64
h 16
controlPv "RF:BITS"
numBits 8
endObjectProperties
object activeLineClass
beginObjectProperties
x 4
y 8
w 20
h 20
lineColor index 14
numPoints 2
xPoints {
0 4
1 24
}
yPoints {
0 8
1 18
}
endObjectProperties
"#;
    let display = load_str(edl, Path::new("odd.edl")).unwrap();
    assert_eq!(display.root.count_class("caLabel"), 1);
    assert_eq!(display.root.children.len(), 4);
    display.root.walk(&mut |node| {
        if node.class_name == "caLabel" {
            assert_eq!(node.prop_str("text"), Some("Hello"));
        }
        if node.class_name == "caByte" {
            assert_eq!(node.prop_str("channel"), Some("RF:BITS"));
        }
        if node.class_name == "caPolyLine" {
            assert_eq!(node.prop_str("xyPairs"), Some("0,0;20,10"));
        }
        if node.class_name == "caChoice" {
            assert_eq!(node.prop_str("channel"), Some("RF:MODE"));
        }
    });
}

#[test]
fn https_display_fetch_uses_rustls() {
    use std::sync::Arc;

    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use rustls::ServerConfig;

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let key_der = cert.serialize_private_key_der();
    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert_der.clone())],
            PrivateKeyDer::Pkcs8(key_der.into()),
        )
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Arc::new(server_config);
    std::thread::spawn(move || {
        let (mut tcp, _) = listener.accept().unwrap();
        let mut conn = rustls::ServerConnection::new(config).unwrap();
        let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
        let _ = std::io::Read::read(&mut tls, &mut [0u8; 1024]);
        let body = b"HTTP/1.0 200 OK\r\nContent-Length: 7\r\n\r\npanel-1";
        let _ = std::io::Write::write_all(&mut tls, body);
        let _ = std::io::Write::flush(&mut tls);
    });
    let text = caqtdm_ui::fetch_https(
        &format!("https://localhost:{}/panel.ui", addr.port()),
        &[cert_der.as_slice()],
    )
    .unwrap();
    assert_eq!(text, "panel-1");
}

#[test]
fn missing_include_warns_and_keeps_the_parent() {
    let parent = r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>40</width><height>20</height></rect></property>
 <widget class="caInclude" name="hole">
  <property name="geometry"><rect><x>0</x><y>0</y><width>10</width><height>10</height></rect></property>
  <property name="filename"><string>object_quad.adl</string></property>
 </widget>
 <widget class="caLabel" name="stays">
  <property name="geometry"><rect><x>0</x><y>0</y><width>10</width><height>10</height></rect></property>
  <property name="text"><string>here</string></property>
 </widget>
</widget></ui>"#;
    let mut display = load_str(parent, Path::new("parent.ui")).unwrap();
    let warnings =
        expand_includes(&mut display.root, &SearchPath::new(), &parse_macro_list("")).unwrap();
    assert!(warnings.iter().any(|line| line.contains("object_quad.adl")));
    assert_eq!(display.root.count_class("caLabel"), 1);
}

#[test]
fn upstream_tests_ui_opens_with_a_missing_include() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../upstream/caQtDM_Tests/tests.ui");
    if !path.is_file() {
        return;
    }
    let mut display = load_path(&path).unwrap();
    let search = SearchPath::from_file(&path);
    let warnings = expand_includes(&mut display.root, &search, &parse_macro_list("")).unwrap();
    assert!(
        warnings
            .iter()
            .any(|line| line.contains("recursive_bad.ui")),
        "{warnings:?}"
    );
    assert!(display.widget_count() > 50);
}
