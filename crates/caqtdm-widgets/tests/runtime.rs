use caqtdm_core::{parse_macro_list, ChannelData, Color, MacroMap, Severity};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_ui::load_str;
use caqtdm_widgets::{
    decode_ppm, load_image_notes, supported_classes, toggled_bit_value, Action, Effect, Session,
    ShapeForm, VisualKind,
};

fn ui() -> &'static str {
    r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>420</width><height>280</height></rect></property>
 <widget class="caLabel" name="title">
  <property name="geometry"><rect><x>8</x><y>8</y><width>160</width><height>22</height></rect></property>
  <property name="text"><string>Vacuum $(SYS)</string></property>
 </widget>
 <widget class="caLineEdit" name="pv">
  <property name="geometry"><rect><x>180</x><y>8</y><width>90</width><height>22</height></rect></property>
  <property name="channel"><string>intern://$(SYS):P</string></property>
  <property name="colorMode"><enum>caLineEdit::colMode::Alarm</enum></property>
  <property name="formatType"><enum>caLineEdit::FormatType::decimal</enum></property>
  <property name="precisionMode"><enum>User</enum></property>
  <property name="precision"><number>2</number></property>
 </widget>
 <widget class="caLed" name="led">
  <property name="geometry"><rect><x>280</x><y>8</y><width>22</width><height>22</height></rect></property>
  <property name="channel"><string>intern://$(SYS):P</string></property>
 </widget>
 <widget class="caGraphics" name="box">
  <property name="geometry"><rect><x>8</x><y>40</y><width>40</width><height>30</height></rect></property>
  <property name="form"><enum>caGraphics::Form::Rectangle</enum></property>
  <property name="visibility"><enum>IfNotZero</enum></property>
  <property name="channel"><string>intern://$(SYS):P</string></property>
 </widget>
 <widget class="caCalc" name="calc">
  <property name="geometry"><rect><x>60</x><y>40</y><width>80</width><height>22</height></rect></property>
  <property name="channel"><string>intern://$(SYS):P</string></property>
  <property name="calc"><string>A*2</string></property>
 </widget>
 <widget class="caSlider" name="set">
  <property name="geometry"><rect><x>8</x><y>80</y><width>160</width><height>24</height></rect></property>
  <property name="channel"><string>intern://SET</string></property>
  <property name="limitsMode"><enum>User</enum></property>
  <property name="minValue"><double>0</double></property>
  <property name="maxValue"><double>10</double></property>
 </widget>
 <widget class="caRelatedDisplay" name="rel">
  <property name="geometry"><rect><x>8</x><y>120</y><width>120</width><height>28</height></rect></property>
  <property name="labels"><string>Details</string></property>
  <property name="files"><string>details.ui</string></property>
 </widget>
 <widget class="caStripPlot" name="plot">
  <property name="geometry"><rect><x>8</x><y>160</y><width>200</width><height>80</height></rect></property>
  <property name="channels"><string>intern://$(SYS):P</string></property>
 </widget>
 <widget class="caTextEntry" name="writer">
  <property name="geometry"><rect><x>180</x><y>80</y><width>80</width><height>22</height></rect></property>
  <property name="channel"><string>intern://$(SYS):P</string></property>
 </widget>
 <widget class="caShellCommand" name="sh">
  <property name="geometry"><rect><x>220</x><y>120</y><width>80</width><height>28</height></rect></property>
  <property name="command"><string>echo hi</string></property>
 </widget>
 <widget class="caClock" name="clk">
  <property name="geometry"><rect><x>300</x><y>40</y><width>80</width><height>22</height></rect></property>
 </widget>
</widget></ui>"#
}

#[test]
fn alarm_calc_visibility_and_writes() {
    let display = load_str(ui(), std::path::Path::new("vac.ui")).unwrap();
    let mut hub = Hub::new("epics3");
    let intern = InternSource::new();
    intern.insert("VAC:P", {
        let mut data = caqtdm_core::ChannelData::scalar(2.5);
        data.severity = Severity::Minor;
        data
    });
    hub.register(Box::new(intern));
    let mut session = Session::open(display.root, hub, &parse_macro_list("SYS=VAC"));
    session.tick(1_000);
    let visuals = session.visuals();
    let title = visuals.iter().find(|v| v.name == "title").unwrap();
    assert_eq!(title.text, "Vacuum VAC");
    let pv = visuals.iter().find(|v| v.name == "pv").unwrap();
    assert_eq!(pv.text, "2.50");
    assert_eq!(pv.severity, Severity::Minor);
    assert_eq!(pv.foreground, caqtdm_core::alarm_color(Severity::Minor));
    let led = visuals.iter().find(|v| v.name == "led").unwrap();
    assert!(matches!(led.kind, VisualKind::Led { on: true }));
    let shape = visuals.iter().find(|v| v.name == "box").unwrap();
    assert!(shape.visible);
    assert!(matches!(
        shape.kind,
        VisualKind::Shape {
            form: ShapeForm::Rectangle,
            ..
        }
    ));
    let calc = visuals.iter().find(|v| v.name == "calc").unwrap();
    assert_eq!(calc.text, "5.000");
    let clock = visuals.iter().find(|v| v.name == "clk").unwrap();
    assert_eq!(clock.text, "00:00:01");

    let slider = visuals.iter().find(|v| v.name == "set").unwrap();
    session.act(Action::SetNumber {
        id: slider.id,
        value: 4.0,
    });
    session.tick(2_000);
    let slider = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "set")
        .unwrap();
    assert!(
        matches!(slider.kind, VisualKind::Slider { fraction, .. } if (fraction - 0.4).abs() < 1e-6)
    );

    let rel = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "rel")
        .unwrap();
    let effects = session.act(Action::Press { id: rel.id });
    assert_eq!(
        effects,
        vec![Effect::Open {
            file: "details.ui".into(),
            macros: String::new(),
        }]
    );
    let shell = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "sh")
        .unwrap();
    let effects = session.act(Action::Press { id: shell.id });
    assert_eq!(
        effects,
        vec![Effect::Shell {
            command: "echo hi".into(),
            ran: false,
        }]
    );

    let writer = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "writer")
        .unwrap();
    session.act(Action::SetText {
        id: writer.id,
        text: "0".into(),
    });
    session.tick(3_000);
    let shape = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "box")
        .unwrap();
    assert!(!shape.visible);
    assert!(supported_classes().contains(&"caCartesianPlot"));
    assert!(supported_classes().contains(&"caReplaceMacro"));
}

#[test]
fn byte_toggle_macro_menu_and_ppm() {
    assert_eq!(toggled_bit_value(0, 8, 10.0, 0, 80), 2.0);
    assert_eq!(toggled_bit_value(1, 8, 0.0, 0, 80), 0.0);

    let ui = r#"<?xml version="1.0"?>
<ui version="4.0"><class>Form</class>
<widget class="QWidget" name="Form">
 <property name="geometry"><rect><x>0</x><y>0</y><width>200</width><height>80</height></rect></property>
 <widget class="caByteController" name="bits">
  <property name="geometry"><rect><x>0</x><y>0</y><width>80</width><height>20</height></rect></property>
  <property name="channel"><string>intern://BITS</string></property>
  <property name="endBit"><number>7</number></property>
 </widget>
 <widget class="caReplaceMacro" name="sector">
  <property name="geometry"><rect><x>0</x><y>28</y><width>80</width><height>40</height></rect></property>
  <property name="macroKey"><string>SYS</string></property>
  <property name="macroValues"><string>VAC,RF</string></property>
 </widget>
 <widget class="caImage" name="pic">
  <property name="geometry"><rect><x>100</x><y>0</y><width>8</width><height>8</height></rect></property>
  <property name="filename"><string>mark.ppm</string></property>
 </widget>
</widget></ui>"#;
    let display = load_str(ui, std::path::Path::new("bits.ui")).unwrap();
    let soft = InternSource::new();
    soft.insert("BITS", caqtdm_core::ChannelData::scalar(1.0));
    let mut hub = Hub::new("intern");
    hub.register(Box::new(soft));
    let mut session = Session::open(display.root, hub, &parse_macro_list(""));
    session.tick(0);
    let bits = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "bits")
        .unwrap();
    assert!(matches!(
        bits.kind,
        VisualKind::Bits {
            value: 1,
            count: 8,
            ..
        }
    ));
    let value = toggled_bit_value(1, 8, 0.0, bits.rect.x, bits.rect.width);
    session.act(Action::SetNumber { id: bits.id, value });
    session.tick(10);
    let bits = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "bits")
        .unwrap();
    assert!(matches!(bits.kind, VisualKind::Bits { value: 0, .. }));

    let menu = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "sector")
        .unwrap();
    let effects = session.act(Action::Choose {
        id: menu.id,
        index: 1,
    });
    assert_eq!(
        effects,
        vec![Effect::SetMacro {
            key: "SYS".into(),
            value: "RF".into(),
        }]
    );

    let ppm = b"P6\n2 1\n255\n\xff\x00\x00\x00\x00\xff";
    let (w, h, rgba) = decode_ppm(ppm).unwrap();
    assert_eq!((w, h), (2, 1));
    assert_eq!(rgba[0], 255);
    assert_eq!(rgba[7], 255);
    let mut visuals = session.visuals();
    load_image_notes(&mut visuals, |note| {
        assert_eq!(note, "mark.ppm");
        Some(ppm.to_vec())
    });
    let pic = visuals.into_iter().find(|v| v.name == "pic").unwrap();
    assert!(matches!(
        pic.kind,
        VisualKind::Image {
            width: 2,
            height: 1,
            ..
        }
    ));
}

#[test]
fn properties_axes_tabs_rotation_and_images() {
    let ui = r#"<?xml version="1.0"?>
<ui version="4.0"><widget class="QMainWindow" name="Main"><widget class="QWidget" name="central">
 <widget class="caLabelVertical" name="vert"><property name="text"><string>UP</string></property>
  <property name="direction"><enum>caLabelVertical::Up</enum></property>
  <property name="fontScaleMode"><enum>EAbstract::Height</enum></property>
  <property name="geometry"><rect><x>0</x><y>0</y><width>20</width><height>80</height></rect></property>
 </widget>
 <widget class="caCartesianPlot" name="plot">
  <property name="Title"><string>Vacuum</string></property>
  <property name="TitleX"><string>s</string></property>
  <property name="TitleY"><string>mbar</string></property>
  <property name="YaxisLimitsMin_1"><double>0</double></property>
  <property name="YaxisLimitsMax_1"><double>10</double></property>
  <property name="numberOfXticks"><number>5</number></property>
  <property name="channel"><string>sim://sine:1:1000:1</string></property>
  <property name="geometry"><rect><x>0</x><y>90</y><width>120</width><height>80</height></rect></property>
 </widget>
 <widget class="caDoubleTabWidget" name="pages">
  <property name="items"><string>Overview;Details</string></property>
  <property name="geometry"><rect><x>0</x><y>180</y><width>120</width><height>40</height></rect></property>
 </widget>
 <widget class="caHMIConfig" name="cfg">
  <property name="outputA"><string>intern://OUT</string></property>
  <property name="valueOrCalc"><string>3</string></property>
  <property name="geometry"><rect><x>0</x><y>230</y><width>40</width><height>20</height></rect></property>
 </widget>
 <widget class="wmSignalRescale" name="scale">
  <property name="softChannelA"><string>intern://W</string></property>
  <property name="softChannelB"><string>intern://H</string></property>
  <property name="geometry"><rect><x>0</x><y>260</y><width>33</width><height>17</height></rect></property>
 </widget>
 <widget class="caSpinbox" name="spin">
  <property name="channel"><string>intern://N</string></property>
  <property name="incrementValue"><double>2</double></property>
  <property name="geometry"><rect><x>50</x><y>230</y><width>40</width><height>20</height></rect></property>
 </widget>
 <widget class="caLabel" name="styled">
  <property name="text"><string>ok</string></property>
  <property name="styleSheet"><string>color: #112233; background-color: #abcdef; font-size: 18px;</string></property>
  <property name="geometry"><rect><x>0</x><y>290</y><width>40</width><height>16</height></rect></property>
 </widget>
 <widget class="caImage" name="svg">
  <property name="filename"><string>mark.svg</string></property>
  <property name="geometry"><rect><x>0</x><y>310</y><width>16</width><height>16</height></rect></property>
 </widget>
 <widget class="caCamera" name="cam">
  <property name="channelData"><string>intern://CAM</string></property>
  <property name="channelWidth"><string>intern://CW</string></property>
  <property name="channelHeight"><string>intern://CH</string></property>
  <property name="geometry"><rect><x>20</x><y>310</y><width>8</width><height>8</height></rect></property>
 </widget>
</widget></widget></ui>"#;
    let display = load_str(ui, std::path::Path::new("props.ui")).unwrap();
    let intern = InternSource::new();
    intern.insert("OUT", ChannelData::scalar(0.0));
    intern.insert("W", ChannelData::scalar(0.0));
    intern.insert("H", ChannelData::scalar(0.0));
    intern.insert("N", ChannelData::scalar(4.0));
    intern.insert("CAM", ChannelData::bytes(vec![255, 0, 0, 0, 255, 0]));
    intern.insert("CW", ChannelData::scalar(2.0));
    intern.insert("CH", ChannelData::scalar(1.0));
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(display.root, hub, &MacroMap::new());
    session.tick(0);
    let visuals = session.visuals();
    let vert = visuals.iter().find(|v| v.name == "vert").unwrap();
    assert_eq!(vert.rotation, 90);
    assert!(vert.font_px > 10);
    let plot = visuals.iter().find(|v| v.name == "plot").unwrap();
    match &plot.kind {
        VisualKind::Plot {
            title,
            title_x,
            title_y,
            y_min,
            y_max,
            x_ticks,
            ..
        } => {
            assert_eq!(title, "Vacuum");
            assert_eq!(title_x, "s");
            assert_eq!(title_y, "mbar");
            assert_eq!(*y_min, 0.0);
            assert_eq!(*y_max, 10.0);
            assert_eq!(*x_ticks, 5);
        }
        other => panic!("{other:?}"),
    }
    let pages = visuals.iter().find(|v| v.name == "pages").unwrap();
    session.act(Action::Choose {
        id: pages.id,
        index: 1,
    });
    let pages = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "pages")
        .unwrap();
    assert!(matches!(pages.kind, VisualKind::Tabs { current: 1, .. }));
    let cfg = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "cfg")
        .unwrap();
    session.act(Action::Press { id: cfg.id });
    let spin = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "spin")
        .unwrap();
    session.act(Action::Press { id: spin.id });
    session.tick(1);
    let styled = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "styled")
        .unwrap();
    assert_eq!(styled.font_px, 18);
    assert_eq!(styled.foreground, Color::rgb(0x11, 0x22, 0x33));
    let svg = "<svg width=\"4\" height=\"2\"><rect x=\"0\" y=\"0\" width=\"2\" height=\"2\" fill=\"#ff0000\"/></svg>";
    let mut visuals = session.visuals();
    load_image_notes(&mut visuals, |name| {
        if name == "mark.svg" {
            Some(svg.as_bytes().to_vec())
        } else {
            None
        }
    });
    let pic = visuals.into_iter().find(|v| v.name == "svg").unwrap();
    match pic.kind {
        VisualKind::Image {
            width,
            height,
            rgba,
            ..
        } => {
            assert_eq!((width, height), (4, 2));
            assert_eq!(&rgba[0..3], &[255, 0, 0]);
        }
        other => panic!("{other:?}"),
    }
    let cam = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "cam")
        .unwrap();
    match cam.kind {
        VisualKind::Image {
            width: 2,
            height: 1,
            rgba,
            ..
        } => {
            assert_eq!(&rgba[0..3], &[255, 0, 0]);
            assert_eq!(&rgba[4..7], &[0, 255, 0]);
        }
        other => panic!("{other:?}"),
    }
    assert!(session
        .log
        .iter()
        .any(|line| line == "put intern://OUT = 3"));
    assert!(session.log.iter().any(|line| line == "put intern://N = 6"));
    assert!(session
        .log
        .iter()
        .any(|line| line == "rescale intern://W = 33"));
    assert!(session
        .log
        .iter()
        .any(|line| line == "rescale intern://H = 17"));
    session.tick(2);
    let spin = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "spin")
        .unwrap();
    assert!(spin.text.starts_with('6'), "{}", spin.text);

    let png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0,
        3, 1, 1, 0, 201, 254, 146, 239, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let (w, h, rgba) = caqtdm_widgets::decode_image(png).expect("png");
    assert_eq!((w, h), (1, 1));
    assert_eq!(&rgba[0..3], &[255, 0, 0]);
    let gif: &[u8] = &[
        71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 255, 0, 0, 0, 0, 0, 33, 249, 4, 1, 0, 0, 0,
        0, 44, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
    ];
    let (w, h, rgba) = caqtdm_widgets::decode_image(gif).expect("gif");
    assert_eq!((w, h), (1, 1));
    assert_eq!(&rgba[0..3], &[255, 0, 0]);
}

#[test]
fn stylesheet_and_the_current_tab_decide_what_is_painted() {
    let ui = r#"<?xml version="1.0"?>
<ui version="4.0"><class>MainWindow</class>
<widget class="QMainWindow" name="MainWindow">
 <property name="geometry"><rect><x>0</x><y>0</y><width>200</width><height>120</height></rect></property>
 <property name="styleSheet"><string>QWidget#centralwidget {background: rgb(58, 94, 171);}
caLineEdit { background: lightyellow; color: black; }</string></property>
 <widget class="QWidget" name="centralwidget">
  <widget class="caLineEdit" name="edit">
   <property name="geometry"><rect><x>4</x><y>4</y><width>40</width><height>16</height></rect></property>
   <property name="channel"><string>intern://E</string></property>
  </widget>
  <widget class="QTabWidget" name="pages">
   <property name="geometry"><rect><x>0</x><y>30</y><width>180</width><height>80</height></rect></property>
   <property name="currentIndex"><number>1</number></property>
   <property name="styleSheet"><string>QTabWidget::tab &gt; QWidget &gt; QWidget {background: wheat;}</string></property>
   <widget class="QWidget" name="first">
    <attribute name="title"><string>One</string></attribute>
    <widget class="caLabel" name="hiddenlabel">
     <property name="geometry"><rect><x>2</x><y>2</y><width>20</width><height>12</height></rect></property>
     <property name="text"><string>HIDE</string></property>
    </widget>
   </widget>
   <widget class="QWidget" name="second">
    <attribute name="title"><string>Two</string></attribute>
    <widget class="caLabel" name="shownlabel">
     <property name="geometry"><rect><x>2</x><y>2</y><width>30</width><height>12</height></rect></property>
     <property name="text"><string>SHOW</string></property>
     <property name="fontScaleMode"><enum>ESimpleLabel::ScaleMode::None</enum></property>
    </widget>
   </widget>
  </widget>
 </widget>
</widget></ui>"#;
    let display = load_str(ui, std::path::Path::new("styled.ui")).unwrap();
    let session = Session::open(display.root, Hub::new("epics3"), &MacroMap::new());
    let visuals = session.visuals();
    let edit = visuals.iter().find(|v| v.name == "edit").unwrap();
    assert_eq!(edit.background, Color::rgb(255, 255, 224));
    assert_eq!(edit.foreground, Color::rgb(0, 0, 0));
    let page = visuals.iter().find(|v| v.name == "centralwidget").unwrap();
    assert_eq!(page.background, Color::rgb(58, 94, 171));
    let hidden = visuals.iter().find(|v| v.name == "hiddenlabel").unwrap();
    let shown = visuals.iter().find(|v| v.name == "shownlabel").unwrap();
    assert!(!hidden.visible);
    assert!(shown.visible);
    let page = visuals.iter().find(|v| v.name == "second").unwrap();
    assert_eq!(page.background, Color::rgb(245, 222, 179));
    assert!(shown.rect.y >= 30 + 28);
    let tabs = visuals.iter().find(|v| v.name == "pages").unwrap();
    match &tabs.kind {
        VisualKind::Tabs { labels, current } => {
            assert_eq!(labels, &vec!["One".to_string(), "Two".to_string()]);
            assert_eq!(*current, 1);
        }
        other => panic!("expected tabs, got {other:?}"),
    }
}
