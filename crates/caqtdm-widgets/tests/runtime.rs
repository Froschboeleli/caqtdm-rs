use caqtdm_core::{parse_macro_list, Severity};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_ui::load_str;
use caqtdm_widgets::{supported_classes, Action, Effect, Session, ShapeForm, VisualKind};

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
    assert!(matches!(shape.kind, VisualKind::Shape { form: ShapeForm::Rectangle, .. }));
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
    let slider = session.visuals().into_iter().find(|v| v.name == "set").unwrap();
    assert!(matches!(slider.kind, VisualKind::Slider { fraction } if (fraction - 0.4).abs() < 1e-6));

    let rel = session.visuals().into_iter().find(|v| v.name == "rel").unwrap();
    let effects = session.act(Action::Press { id: rel.id });
    assert_eq!(
        effects,
        vec![Effect::Open {
            file: "details.ui".into(),
            macros: String::new(),
        }]
    );
    let shell = session.visuals().into_iter().find(|v| v.name == "sh").unwrap();
    let effects = session.act(Action::Press { id: shell.id });
    assert_eq!(
        effects,
        vec![Effect::Shell {
            command: "echo hi".into(),
            ran: false,
        }]
    );

    let writer = session.visuals().into_iter().find(|v| v.name == "writer").unwrap();
    session.act(Action::SetText {
        id: writer.id,
        text: "0".into(),
    });
    session.tick(3_000);
    let shape = session.visuals().into_iter().find(|v| v.name == "box").unwrap();
    assert!(!shape.visible);
    assert!(supported_classes().contains(&"caCartesianPlot"));
}
