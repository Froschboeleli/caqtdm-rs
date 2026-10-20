use caqtdm_core::{alarm_color, ChannelData, Limits, PropValue, PvValue, Severity};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_ui::WidgetNode;
use caqtdm_widgets::{Action, Session, VisualKind};

fn leaf(class_name: &str, name: &str, channel: &str) -> WidgetNode {
    let mut node = WidgetNode::new(class_name, name);
    node.rect = caqtdm_core::Rect::new(0, 0, 80, 40);
    if !channel.is_empty() {
        node.set_prop("channel", PropValue::String(channel.into()));
    }
    node
}

fn session(root: WidgetNode, intern: InternSource) -> Session {
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(1_000);
    session
}

#[test]
fn numeric_write_is_clamped_to_user_limits() {
    let intern = InternSource::new();
    intern.insert("N", ChannelData::scalar(4.0));
    let mut numeric = leaf("caNumeric", "numeric", "intern://N");
    numeric.set_prop("limitsMode", PropValue::Enum("User".into()));
    numeric.set_prop("minValue", PropValue::Double(0.0));
    numeric.set_prop("maxValue", PropValue::Double(10.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(numeric);
    let mut session = session(root, intern);
    let id = session.visuals()[1].id;
    session.act(Action::SetNumber { id, value: 50.0 });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://N = 10")));
}

#[test]
fn choice_buttons_stack_and_write_the_bit_range() {
    let intern = InternSource::new();
    intern.insert("M", ChannelData::scalar(0.0));
    let mut choice = leaf("caChoice", "choice", "intern://M");
    choice.set_prop("items", PropValue::String("Off;On;Tri".into()));
    choice.set_prop("stackingMode", PropValue::Enum("Column".into()));
    choice.set_prop("startBit", PropValue::Double(1.0));
    choice.set_prop("endBit", PropValue::Double(2.0));
    let mut inverse = leaf("caChoice", "inverse", "intern://M");
    inverse.set_prop("items", PropValue::String("Off;On".into()));
    inverse.set_prop("stackingMode", PropValue::Enum("RowInverse".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.extend([choice, inverse]);
    let mut session = session(root, intern);
    let visuals = session.visuals();
    let stacked = visuals.iter().find(|v| v.name == "choice").unwrap();
    match &stacked.kind {
        VisualKind::Choice {
            items,
            columns,
            inverse,
            ..
        } => {
            assert_eq!(items, &["On".to_string(), "Tri".into()]);
            assert_eq!(*columns, 2);
            assert!(!inverse);
        }
        other => panic!("{other:?}"),
    }
    let flipped = visuals.iter().find(|v| v.name == "inverse").unwrap();
    match &flipped.kind {
        VisualKind::Choice {
            columns, inverse, ..
        } => {
            assert_eq!(*columns, 1);
            assert!(inverse);
        }
        other => panic!("{other:?}"),
    }
    session.act(Action::Choose {
        id: stacked.id,
        index: 0,
    });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://M = 1")));
}

#[test]
fn tables_use_columns_and_a_wave_grid() {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(1.0));
    intern.insert("B", ChannelData::scalar(2.0));
    let mut wave = ChannelData::scalar(0.0);
    wave.value = PvValue::Doubles(vec![1.0, 2.0, 3.0, 4.0]);
    intern.insert("W", wave);
    let mut table = leaf("caTable", "table", "intern://A");
    table.set_prop("channels", PropValue::String("intern://B".into()));
    table.set_prop("precisionMode", PropValue::Enum("User".into()));
    table.set_prop("precision", PropValue::Double(0.0));
    let mut grid = leaf("caWaveTable", "wave", "intern://W");
    grid.set_prop("numberOfRows", PropValue::Double(2.0));
    grid.set_prop("numberOfColumns", PropValue::Double(2.0));
    grid.set_prop("precisionMode", PropValue::Enum("User".into()));
    grid.set_prop("precision", PropValue::Double(0.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.extend([table, grid]);
    let session = session(root, intern);
    let visuals = session.visuals();
    match &visuals.iter().find(|v| v.name == "table").unwrap().kind {
        VisualKind::Table { cells } => assert_eq!(cells, &vec![vec!["1".to_string(), "2".into()]]),
        other => panic!("{other:?}"),
    }
    match &visuals.iter().find(|v| v.name == "wave").unwrap().kind {
        VisualKind::Table { cells } => {
            assert_eq!(
                cells,
                &vec![
                    vec!["1".to_string(), "2".into()],
                    vec!["3".into(), "4".into()]
                ]
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn alarm_group_collapses() {
    let intern = InternSource::new();
    let mut alarm = ChannelData::scalar(1.0);
    alarm.severity = Severity::Major;
    intern.insert("ALM", alarm);
    let mut tree = leaf("caAlarmTree", "tree", "");
    let mut group = leaf("caFrame", "grp", "");
    let row = leaf("caLineEdit", "alm", "intern://ALM");
    group.children.push(row);
    tree.children.push(group);
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(tree);
    let mut session = session(root, intern);
    let before = session.visuals();
    let tree_visual = before.iter().find(|v| v.name == "tree").unwrap();
    match &tree_visual.kind {
        VisualKind::Tree { rows } => {
            assert_eq!(rows.len(), 2);
            assert!(rows[0].group && rows[0].open);
            assert_eq!(rows[1].cells[3], "MAJOR");
        }
        other => panic!("{other:?}"),
    }
    session.act(Action::ToggleGroup {
        id: tree_visual.id,
        index: 0,
    });
    let after = session.visuals();
    match &after.iter().find(|v| v.name == "tree").unwrap().kind {
        VisualKind::Tree { rows } => {
            assert_eq!(rows.len(), 1);
            assert!(!rows[0].open);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn line_draw_uses_pen_width_and_alarm_color() {
    let intern = InternSource::new();
    let mut sample = ChannelData::scalar(5.0);
    sample.severity = Severity::Major;
    sample.limits = Limits {
        lower_display: 0.0,
        upper_display: 10.0,
        ..Limits::default()
    };
    intern.insert("DRAW", sample);
    let mut draw = leaf("caLineDraw", "draw", "intern://DRAW");
    draw.set_prop("colorMode", PropValue::Enum("Alarm".into()));
    draw.set_prop("frameLineWidth", PropValue::Double(3.0));
    draw.set_prop("limitsMode", PropValue::Enum("Channel".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(draw);
    let mut session = session(root, intern);
    intern_tick(&mut session);
    let visual = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "draw")
        .unwrap();
    assert_eq!(visual.foreground, alarm_color(Severity::Major));
    match visual.kind {
        VisualKind::Polyline { points, width, .. } => {
            assert_eq!(width, 3);
            assert!(points.len() >= 2);
            assert_eq!(points[0].y, points[1].y);
        }
        other => panic!("{other:?}"),
    }
}

fn intern_tick(session: &mut Session) {
    session.tick(2_000);
}

#[test]
fn plots_carry_log_scale_and_a_second_axis() {
    let intern = InternSource::new();
    intern.insert("Y1", ChannelData::scalar(2.0));
    intern.insert("Y2", ChannelData::scalar(500.0));
    let mut plot = leaf("caCartesianPlot", "plot", "");
    plot.set_prop("channels_1", PropValue::String("intern://Y1".into()));
    plot.set_prop("channels_2", PropValue::String("intern://Y2".into()));
    plot.set_prop("YAxisType", PropValue::Enum("log10".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(1.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(100.0));
    plot.set_prop("YaxisLimitsMin_2", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_2", PropValue::Double(1000.0));
    let mut strip = leaf("caStripPlot", "strip", "intern://Y1");
    strip.set_prop("timeSpan", PropValue::Double(1.0));
    strip.set_prop("YAxisType", PropValue::Enum("log10".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.extend([plot, strip]);
    let mut session = session(root, intern);
    session.tick(2_000);
    let visuals = session.visuals();
    match &visuals.iter().find(|v| v.name == "plot").unwrap().kind {
        VisualKind::Plot {
            series,
            log_y,
            y_min,
            y_max,
            y2_min,
            y2_max,
            ..
        } => {
            assert!(log_y);
            assert_eq!((*y_min, *y_max), (1.0, 100.0));
            assert_eq!((*y2_min, *y2_max), (0.0, 1000.0));
            assert_eq!(series.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    match &visuals.iter().find(|v| v.name == "strip").unwrap().kind {
        VisualKind::Plot { series, log_y, .. } => {
            assert!(log_y);
            assert_eq!(series[0].len(), 1);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn analog_clock_reads_the_tick() {
    let mut clock = leaf("caClock", "clock", "");
    clock.set_prop("timeType", PropValue::Enum("Analog".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(clock);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(InternSource::new()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(3 * 3_600_000);
    match &session.visuals()[1].kind {
        VisualKind::Clock {
            analog,
            hour,
            minute,
            second,
        } => {
            assert!(analog);
            assert_eq!((*hour, *minute, *second), (3, 0, 0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn designer_plugin_paints_as_the_registered_kind() {
    use caqtdm_widgets::{install_designer_plugin, DesignerKind, DesignerPlugin};

    install_designer_plugin(DesignerPlugin {
        class_name: "psiDemoLamp".into(),
        group: "PSI".into(),
        properties: vec!["channel".into(), "foreground".into()],
        kind: DesignerKind::Led,
    });
    let mut lamp = leaf("psiDemoLamp", "lamp", "");
    lamp.set_prop(
        "foreground",
        PropValue::Color(caqtdm_core::Color::rgb(0, 180, 0)),
    );
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(lamp);
    let session = session(root, InternSource::new());
    let visuals = session.visuals();
    let visual = visuals.iter().find(|v| v.name == "lamp").unwrap();
    assert!(matches!(visual.kind, VisualKind::Led { on: true, .. }));
    let image = caqtdm_paint::paint(std::slice::from_ref(visual));
    let pixel = image
        .pixel(
            visual.rect.x + visual.rect.width / 2,
            visual.rect.y + visual.rect.height / 2,
        )
        .unwrap();
    assert!(pixel.g > 100, "registered lamp paints a lit pixel");
}

#[test]
fn led_units_alignment_and_log_scale_follow_the_properties() {
    use caqtdm_core::ChannelData;

    let mut led = leaf("caLed", "lamp", "intern://LAMP");
    led.set_prop(
        "trueColor",
        PropValue::Color(caqtdm_core::Color::rgb(0, 200, 0)),
    );
    led.set_prop(
        "falseColor",
        PropValue::Color(caqtdm_core::Color::rgb(180, 0, 0)),
    );
    led.set_prop("trueValue", PropValue::String("2".into()));
    let mut entry = leaf("caLineEdit", "readout", "intern://EGU");
    entry.set_prop("unitsEnabled", PropValue::Bool(true));
    let mut label = leaf("caLabel", "title", "");
    label.set_prop("text", PropValue::String("Hi".into()));
    label.set_prop("alignment", PropValue::Set("Qt::AlignRight".into()));
    label.rect.width = 80;
    let mut gauge = leaf("caLinearGauge", "bar", "intern://LOG");
    gauge.set_prop("minValue", PropValue::Double(1.0));
    gauge.set_prop("maxValue", PropValue::Double(100.0));
    gauge.set_prop("limitsMode", PropValue::Enum("User".into()));
    gauge.set_prop("logarithmicScale", PropValue::Bool(true));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.extend([led, entry, label, gauge]);
    let mut hub = Hub::new("epics3");
    let intern = InternSource::new();
    intern.insert("LAMP", ChannelData::scalar(2.0));
    let mut egu = ChannelData::scalar(3.5);
    egu.units = "mA".into();
    intern.insert("EGU", egu);
    intern.insert("LOG", ChannelData::scalar(10.0));
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(0);
    let visuals = session.visuals();
    let lamp = visuals.iter().find(|v| v.name == "lamp").unwrap();
    assert!(matches!(lamp.kind, VisualKind::Led { on: true, .. }));
    assert_eq!(lamp.foreground, caqtdm_core::Color::rgb(0, 200, 0));
    let readout = visuals.iter().find(|v| v.name == "readout").unwrap();
    assert!(readout.text.ends_with(" mA"), "{}", readout.text);
    let title = visuals.iter().find(|v| v.name == "title").unwrap();
    assert_eq!(title.align, 2);
    let bar = visuals.iter().find(|v| v.name == "bar").unwrap();
    match &bar.kind {
        VisualKind::Gauge {
            fraction,
            circular: false,
        } => {
            let expected = (10.0f64.log10() - 1.0f64.log10()) / (100.0f64.log10() - 1.0f64.log10());
            assert!((fraction - expected).abs() < 1e-9, "{fraction}");
        }
        other => panic!("{other:?}"),
    }
}
