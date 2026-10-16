use caqtdm_core::{ChannelData, PropValue, PvValue, Rect, Severity};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_ui::WidgetNode;
use caqtdm_widgets::{Action, Effect, Session, ShapeForm, VisualKind};

fn leaf(class_name: &str, name: &str, channel: &str) -> WidgetNode {
    let mut node = WidgetNode::new(class_name, name);
    node.rect = Rect::new(0, 0, 100, 40);
    if !channel.is_empty() {
        node.set_prop("channel", PropValue::String(channel.into()));
    }
    node
}

#[test]
fn each_widget_honors_the_properties_an_author_sets() {
    let intern = InternSource::new();
    intern.insert("N", ChannelData::scalar(1200.0));
    intern.insert("A", ChannelData::scalar(0.0));
    intern.insert("S", ChannelData::scalar(2.0));
    intern.insert("M", {
        let mut data = ChannelData::scalar(0.0);
        data.enum_strings = vec!["off".into(), "on".into()];
        data
    });
    intern.insert("T", ChannelData::scalar(0.0));
    intern.insert("B", ChannelData::scalar(0.0));
    intern.insert("G", ChannelData::scalar(0.0));
    intern.insert("LED", ChannelData::scalar(2.0));
    intern.insert("GAUGE", ChannelData::scalar(25.0));
    intern.insert("HEX", ChannelData::scalar(255.0));
    intern.insert("CALC", ChannelData::scalar(4.0));
    let mut wave = ChannelData::scalar(0.0);
    wave.value = PvValue::Doubles(vec![1.0, 2.0, 3.0, 4.0]);
    wave.element_count = 4;
    intern.insert("WF", wave);
    let mut alarm = ChannelData::scalar(1.0);
    alarm.severity = Severity::Minor;
    intern.insert("ALM", alarm.clone());
    intern.insert("STRIP", ChannelData::scalar(1.0));
    intern.insert("DRAW", ChannelData::scalar(1.0));
    intern.insert("CAM16", ChannelData::bytes(vec![10, 0, 200, 0]));

    let mut root = WidgetNode::new("QWidget", "root");
    root.rect = Rect::new(0, 0, 400, 400);
    let mut numeric = leaf("caNumeric", "numeric", "intern://N");
    numeric.set_prop("formatType", PropValue::Enum("exponential".into()));
    numeric.set_prop("precisionMode", PropValue::Enum("User".into()));
    numeric.set_prop("precision", PropValue::Double(2.0));
    let apply = leaf("caApplyNumeric", "apply", "intern://A");
    let mut slider = leaf("caSlider", "slider", "intern://S");
    slider.set_prop("limitsMode", PropValue::Enum("User".into()));
    slider.set_prop("minValue", PropValue::Double(0.0));
    slider.set_prop("maxValue", PropValue::Double(10.0));
    slider.set_prop("orientation", PropValue::Enum("Down".into()));
    let menu = leaf("caMenu", "menu", "intern://M");
    let mut choice = leaf("caChoice", "choice", "intern://M");
    choice.set_prop("items", PropValue::String("Off;On".into()));
    let text = leaf("caTextEntry", "text", "intern://T");
    let mut button = leaf("caMessageButton", "button", "intern://B");
    button.set_prop("pressValue", PropValue::Double(4.0));
    let toggle = leaf("caToggleButton", "toggle", "intern://G");
    let mut led = leaf("caLed", "led", "intern://LED");
    led.set_prop("bitNr", PropValue::Double(1.0));
    let mut gauge = leaf("caLinearGauge", "gauge", "intern://GAUGE");
    gauge.set_prop("limitsMode", PropValue::Enum("User".into()));
    gauge.set_prop("minValue", PropValue::Double(0.0));
    gauge.set_prop("maxValue", PropValue::Double(100.0));
    let mut dial = leaf("caCircularGauge", "dial", "intern://GAUGE");
    dial.set_prop("limitsMode", PropValue::Enum("User".into()));
    dial.set_prop("minValue", PropValue::Double(0.0));
    dial.set_prop("maxValue", PropValue::Double(100.0));
    let mut thermo = leaf("caThermo", "thermo", "intern://GAUGE");
    thermo.set_prop("limitsMode", PropValue::Enum("User".into()));
    thermo.set_prop("minValue", PropValue::Double(0.0));
    thermo.set_prop("maxValue", PropValue::Double(100.0));
    let mut meter = leaf("caMeter", "meter", "intern://GAUGE");
    meter.set_prop("limitsMode", PropValue::Enum("User".into()));
    meter.set_prop("minValue", PropValue::Double(0.0));
    meter.set_prop("maxValue", PropValue::Double(100.0));
    let mut hex = leaf("caLineEdit", "hex", "intern://HEX");
    hex.set_prop("formatType", PropValue::Enum("hex".into()));
    let mut lines = leaf("caMultiLineString", "lines", "");
    lines.set_prop("text", PropValue::String("one\ntwo".into()));
    let mut label = leaf("caLabel", "label", "");
    label.set_prop("text", PropValue::String("hello".into()));
    let mut frame = leaf("caFrame", "frame", "");
    let mut child = leaf("caLabel", "inside", "");
    child.rect = Rect::new(4, 6, 10, 10);
    frame.children.push(child);
    let mut shape = leaf("caGraphics", "shape", "");
    shape.set_prop("form", PropValue::Enum("caGraphics::Form::Circle".into()));
    let mut poly = leaf("caPolyLine", "poly", "");
    poly.set_prop("xyPairs", PropValue::String("0,0,10,8".into()));
    poly.set_prop("polystyle", PropValue::Enum("Polygon".into()));
    let mut clock = leaf("caClock", "clock", "");
    clock.set_prop("timeType", PropValue::Enum("Analog".into()));
    let mut bits = leaf("caBitnames", "bits", "intern://LED");
    bits.set_prop("startBit", PropValue::Double(0.0));
    bits.set_prop("endBit", PropValue::Double(1.0));
    bits.set_prop("trueStrings", PropValue::String("lo;hi".into()));
    let byte = leaf("caByte", "byte", "intern://LED");
    let table = leaf("caTable", "table", "intern://WF");
    let wave = leaf("caWaveTable", "wave", "intern://WF");
    let mut calc = leaf("caCalc", "calc", "intern://CALC");
    calc.set_prop("calc", PropValue::String("A+1".into()));
    let fall = leaf("caWaterfallPlot", "fall", "intern://WF");
    let mut scan = leaf("caScan2D", "scan", "intern://WF");
    scan.set_prop("channelWidth", PropValue::Double(2.0));
    scan.set_prop("channelHeight", PropValue::Double(2.0));
    let draw = leaf("caLineDraw", "draw", "intern://DRAW");
    let mut tree = leaf("caAlarmTree", "tree", "");
    let mut row = leaf("caLineEdit", "alm", "intern://ALM");
    row.set_prop("mask", PropValue::String("-".into()));
    tree.children.push(row);
    let log = leaf("caAlarmLog", "log", "intern://ALM");
    let mut plot = leaf("caCartesianPlot", "plot", "intern://N");
    plot.set_prop("Title", PropValue::String("P".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    let mut strip = leaf("caStripPlot", "strip", "intern://STRIP");
    strip.set_prop("timeSpan", PropValue::Double(2.0));
    let mut shell = leaf("caShellCommand", "shell", "");
    shell.set_prop("command", PropValue::String("echo hi".into()));
    let script = leaf("caScriptButton", "script", "");
    let mut mime = leaf("caMimeDisplay", "mime", "");
    mime.set_prop("labels", PropValue::String("open".into()));
    mime.set_prop("files", PropValue::String("other.ui".into()));
    let mut rowcol = leaf("caRowColMenu", "rowcol", "intern://M");
    rowcol.set_prop("labels", PropValue::String("a;b".into()));
    rowcol.set_prop("files", PropValue::String("one.ui;two.ui".into()));
    rowcol.set_prop("args", PropValue::String("A=1;B=2".into()));
    rowcol.set_prop("stacking", PropValue::Enum("Column".into()));
    let mut cam = leaf("caCamera", "cam", "");
    cam.set_prop("channelData", PropValue::String("intern://CAM16".into()));
    cam.set_prop("channelWidth", PropValue::Double(2.0));
    cam.set_prop("channelHeight", PropValue::Double(1.0));
    cam.set_prop("dataType", PropValue::String("mono16".into()));

    root.children.extend([
        numeric, apply, slider, menu, choice, text, button, toggle, led, gauge, dial, thermo,
        meter, hex, lines, label, frame, shape, poly, clock, bits, byte, table, wave, calc, fall,
        scan, draw, tree, log, plot, strip, shell, script, mime, rowcol, cam,
    ]);

    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern.clone()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.allow_shell = true;
    session.tick(1_000);
    intern.insert("STRIP", ChannelData::scalar(2.0));
    session.tick(2_000);
    intern.insert("STRIP", ChannelData::scalar(3.0));
    session.tick(3_000);
    intern.insert("STRIP", ChannelData::scalar(4.0));
    intern.insert("DRAW", ChannelData::scalar(5.0));
    let mut major = ChannelData::scalar(9.0);
    major.severity = Severity::Major;
    intern.insert("ALM", major);
    session.tick(4_000);

    let visuals = session.visuals();
    let by = |name: &str| visuals.iter().find(|v| v.name == name).unwrap().clone();

    assert_eq!(by("numeric").text, "1.20e+03");
    assert_eq!(by("label").text, "hello");
    assert!(by("lines").text.contains('\n'));
    assert_eq!(by("hex").text, "0xff");
    assert!(by("calc").text.starts_with('5'));
    match by("slider").kind {
        VisualKind::Slider { fraction, .. } => assert!((fraction - 0.8).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
    match by("led").kind {
        VisualKind::Led { on } => assert!(on),
        other => panic!("{other:?}"),
    }
    for (name, circular) in [("gauge", false), ("dial", true)] {
        match by(name).kind {
            VisualKind::Gauge {
                fraction,
                circular: flag,
            } => {
                assert!((fraction - 0.25).abs() < 1e-9);
                assert_eq!(flag, circular);
            }
            other => panic!("{name} {other:?}"),
        }
    }
    match by("thermo").kind {
        VisualKind::Thermo { fraction, .. } => assert!((fraction - 0.25).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
    match by("meter").kind {
        VisualKind::Meter { fraction } => assert!((fraction - 0.25).abs() < 1e-9),
        other => panic!("{other:?}"),
    }
    match by("shape").kind {
        VisualKind::Shape { form, filled, .. } => {
            assert_eq!(form, ShapeForm::Circle);
            assert!(filled);
        }
        other => panic!("{other:?}"),
    }
    match by("poly").kind {
        VisualKind::Polyline { points, closed, .. } => {
            assert_eq!(points.len(), 2);
            assert!(closed);
        }
        other => panic!("{other:?}"),
    }
    match by("clock").kind {
        VisualKind::Clock { analog, .. } => assert!(analog),
        other => panic!("{other:?}"),
    }
    match by("bits").kind {
        VisualKind::Bits { count, names, .. } => {
            assert_eq!(count, 2);
            assert_eq!(names, vec!["lo".to_string(), "hi".into()]);
        }
        other => panic!("{other:?}"),
    }
    match by("table").kind {
        VisualKind::Table { cells } => assert_eq!(cells.len(), 4),
        other => panic!("{other:?}"),
    }
    match by("wave").kind {
        VisualKind::Table { cells } => assert_eq!(cells.len(), 4),
        other => panic!("{other:?}"),
    }
    match by("fall").kind {
        VisualKind::Waterfall { rows, .. } => assert!(!rows.is_empty()),
        other => panic!("{other:?}"),
    }
    match by("scan").kind {
        VisualKind::Image {
            width,
            height,
            rgba,
            ..
        } => {
            assert_eq!((width, height), (2, 2));
            assert_eq!(rgba.len(), 16);
        }
        other => panic!("{other:?}"),
    }
    match by("draw").kind {
        VisualKind::Polyline { points, .. } => assert!(points.len() >= 2),
        other => panic!("{other:?}"),
    }
    match by("tree").kind {
        VisualKind::Tree { rows } => assert_eq!(rows[0].cells[3], "MAJOR"),
        other => panic!("{other:?}"),
    }
    match by("log").kind {
        VisualKind::Table { cells } => assert!(cells.iter().any(|row| row[1] == "MAJOR")),
        other => panic!("{other:?}"),
    }
    match by("strip").kind {
        VisualKind::Plot { series, .. } => assert_eq!(series[0], vec![3.0, 4.0]),
        other => panic!("{other:?}"),
    }
    match by("rowcol").kind {
        VisualKind::Grid { columns, cells } => {
            assert_eq!(columns, 2);
            assert_eq!(cells[1].file, "two.ui");
        }
        other => panic!("{other:?}"),
    }
    match by("cam").kind {
        VisualKind::Image { rgba, .. } => assert_eq!(rgba[0], 10),
        other => panic!("{other:?}"),
    }
    let inside = by("inside");
    assert_eq!(inside.rect, Rect::new(4, 6, 10, 10));

    let id = |name: &str| by(name).id;
    session.act(Action::SetNumber {
        id: id("apply"),
        value: 5.0,
    });
    assert!(session.log.iter().any(|line| line.contains("pending")));
    assert!(!session
        .log
        .iter()
        .any(|line| line.contains("put intern://A")));
    session.act(Action::Press { id: id("apply") });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://A = 5")));
    session.act(Action::Choose {
        id: id("menu"),
        index: 1,
    });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://M = 1")));
    session.act(Action::SetText {
        id: id("text"),
        text: "3.5".into(),
    });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://T = 3.5")));
    session.act(Action::Press { id: id("button") });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://B = 4")));
    session.act(Action::Press { id: id("toggle") });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://G = 1")));
    session.act(Action::Zoom {
        id: id("plot"),
        x_min: 0.25,
        x_max: 0.75,
        y_min: 1.0,
        y_max: 2.0,
    });
    session.act(Action::PlaceCursor {
        id: id("plot"),
        at: 0.25,
    });
    let plot = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    match plot.kind {
        VisualKind::Plot {
            x_min,
            x_max,
            y_min,
            y_max,
            cursors,
            ..
        } => {
            assert_eq!((x_min, x_max, y_min, y_max), (0.25, 0.75, 1.0, 2.0));
            assert_eq!(cursors, vec![0.25]);
        }
        other => panic!("{other:?}"),
    }
    let shell = session.act(Action::Press { id: id("shell") });
    assert_eq!(
        shell,
        vec![Effect::Shell {
            command: "echo hi".into(),
            ran: true
        }]
    );
    let script = session.act(Action::Press { id: id("script") });
    assert_eq!(
        script,
        vec![Effect::Shell {
            command: String::new(),
            ran: false
        }]
    );
    let opened = session.act(Action::Press { id: id("mime") });
    assert_eq!(
        opened,
        vec![Effect::Open {
            file: "other.ui".into(),
            macros: String::new()
        }]
    );
    let opened = session.act(Action::Choose {
        id: id("rowcol"),
        index: 1,
    });
    assert_eq!(
        opened,
        vec![Effect::Open {
            file: "two.ui".into(),
            macros: "B=2".into(),
        }]
    );
    session.act(Action::SetNumber {
        id: id("numeric"),
        value: 7.5,
    });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://N = 7.5")));
}
