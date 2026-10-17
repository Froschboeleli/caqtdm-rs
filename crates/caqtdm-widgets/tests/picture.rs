use caqtdm_core::{ChannelData, Color, PropValue, PvValue, Rect};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_paint::paint;
use caqtdm_ui::WidgetNode;
use caqtdm_widgets::{Action, Session, VisualKind};

fn text_channel(text: &str) -> ChannelData {
    let mut data = ChannelData::scalar(0.0);
    data.value = PvValue::String(text.into());
    data
}

fn open(root: WidgetNode, intern: InternSource) -> Session {
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(1_000);
    session.tick(2_000);
    session
}

#[test]
fn legend_colors_and_extra_axes_change_the_plot() {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(0.0));
    intern.insert("B", ChannelData::scalar(500.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop(
        "channels",
        PropValue::StringList(vec!["intern://A".into(), "intern://B".into()]),
    );
    plot.set_prop("LegendEnabled", PropValue::Bool(true));
    plot.set_prop("YAxisScaling", PropValue::Enum("fixedScale".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    plot.set_prop("YaxisLimitsMin_2", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_2", PropValue::Double(1000.0));
    plot.set_prop("color_2", PropValue::Color(Color::rgb(220, 0, 0)));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let session = open(root, intern);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    match &drawn.kind {
        VisualKind::Plot {
            legend,
            colors,
            axes,
            ..
        } => {
            assert_eq!(legend[0], "intern://A [0.,10.]");
            assert_eq!(legend[1], "intern://B [0.,1000.]");
            assert_eq!(colors[1], Some(Color::rgb(220, 0, 0)));
            assert_eq!(axes[1], (0.0, 1000.0));
        }
        other => panic!("{other:?}"),
    }
    let image = paint(&[drawn]);
    assert!(
        image.pixel(20, 34).unwrap().r > 180,
        "second axis places 500 mid-scale"
    );
    assert!(
        image.pixel(20, 18).unwrap().r < 80,
        "500 is not clipped to the top"
    );
    let mut legend_ink = false;
    for y in 8..16 {
        for x in 50..78 {
            if image.pixel(x, y).unwrap().r > 180 {
                legend_ink = true;
            }
        }
    }
    assert!(legend_ink, "legend text is painted in the series color");

    let mut hidden = WidgetNode::new("caStripPlot", "quiet");
    hidden.rect = Rect::new(0, 0, 80, 60);
    hidden.set_prop("channel", PropValue::String("intern://A".into()));
    hidden.set_prop("LegendEnabled", PropValue::Bool(false));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(hidden);
    let session = open(root, InternSource::new());
    match session
        .visuals()
        .into_iter()
        .find(|v| v.name == "quiet")
        .unwrap()
        .kind
    {
        VisualKind::Plot { legend, .. } => assert!(legend.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn roi_marker_and_write_follow_upstream() {
    let intern = InternSource::new();
    intern.insert("X0", ChannelData::scalar(0.0));
    intern.insert("Y0", ChannelData::scalar(0.0));
    intern.insert("X1", ChannelData::scalar(7.0));
    intern.insert("Y1", ChannelData::scalar(7.0));
    intern.insert("WX", ChannelData::scalar(0.0));
    intern.insert("WY", ChannelData::scalar(0.0));
    let mut camera = WidgetNode::new("caCamera", "cam");
    camera.rect = Rect::new(0, 0, 8, 8);
    camera.set_prop("channelData", PropValue::String("intern://PIX".into()));
    camera.set_prop("dataType", PropValue::String("mono8".into()));
    camera.set_prop("channelWidth", PropValue::Double(8.0));
    camera.set_prop("channelHeight", PropValue::Double(8.0));
    camera.set_prop(
        "ROI_readChannels",
        PropValue::String("intern://X0;intern://Y0;intern://X1;intern://Y1".into()),
    );
    camera.set_prop("ROI_readType", PropValue::Enum("xy1_xy2".into()));
    camera.set_prop("ROI_readmarkerType", PropValue::Enum("box".into()));
    camera.set_prop(
        "ROI_writeType",
        PropValue::Enum("xycenter_width_height".into()),
    );
    camera.set_prop(
        "ROI_writeChannels",
        PropValue::String("intern://WX;intern://WY;intern://X1;intern://Y1".into()),
    );
    let mut bytes = ChannelData::scalar(0.0);
    bytes.value = PvValue::Bytes(vec![0; 64]);
    intern.insert("PIX", bytes);
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(camera);
    let mut session = open(root, intern);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "cam")
        .unwrap();
    let image = paint(&[drawn]);
    assert_eq!(image.pixel(0, 0).unwrap().r, 220, "roi box");
    assert!(
        image.pixel(3, 3).unwrap().r < 40,
        "inside the box stays dark"
    );
    let id = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "cam")
        .unwrap()
        .id;
    session.act(Action::Roi {
        id,
        x0: 1.0,
        y0: 1.0,
        x1: 5.0,
        y1: 3.0,
    });
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://WX = 3")));
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://WY = 2")));
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://X1 = 4")));
    assert!(session
        .log
        .iter()
        .any(|line| line.contains("put intern://Y1 = 2")));
}

#[test]
fn scan_save_path_loads_the_mda_image() {
    let dir = std::env::temp_dir().join(format!("caqtdm-mda-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let file = sub.join("scan.mda");
    std::fs::write(
        &file,
        caqtdm_widgets::write_rank2(&[vec![1.0, 0.0], vec![0.0, 1.0]], 0, "outer"),
    )
    .unwrap();
    let intern = InternSource::new();
    intern.insert("PATH", text_channel(&dir.display().to_string()));
    intern.insert("SUB", text_channel("sub"));
    intern.insert("FILE", text_channel("scan.mda"));
    let mut scan = WidgetNode::new("caScan2D", "scan");
    scan.rect = Rect::new(0, 0, 2, 2);
    scan.set_prop("channelData", PropValue::String("det0001".into()));
    scan.set_prop("channelWidth", PropValue::Double(2.0));
    scan.set_prop("channelHeight", PropValue::Double(2.0));
    scan.set_prop(
        "channelSAVEDATA_PATH",
        PropValue::String("intern://PATH".into()),
    );
    scan.set_prop(
        "channelSAVEDATA_SUBDIR",
        PropValue::String("intern://SUB".into()),
    );
    scan.set_prop(
        "channelSAVEDATA_FILENAME",
        PropValue::String("intern://FILE".into()),
    );
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(scan);
    let session = open(root, intern);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "scan")
        .unwrap();
    match drawn.kind {
        VisualKind::Image {
            note,
            width,
            height,
            rgba,
            ..
        } => {
            assert_eq!(note, "mda");
            assert_eq!((width, height), (2, 2));
            assert_eq!(rgba[0], 255);
            assert_eq!(rgba[4], 0);
            assert_eq!(rgba[8], 0);
            assert_eq!(rgba[12], 255);
        }
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn polyline_line_color_and_width_are_painted() {
    let mut line = WidgetNode::new("caPolyLine", "pen");
    line.rect = Rect::new(0, 0, 20, 8);
    line.set_prop("xyPairs", PropValue::String("0,4;19,4".into()));
    line.set_prop("lineSize", PropValue::Double(3.0));
    line.set_prop("lineColor", PropValue::Color(Color::rgb(0, 180, 0)));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(line);
    let session = open(root, InternSource::new());
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "pen")
        .unwrap();
    assert_eq!(drawn.foreground, Color::rgb(0, 180, 0));
    match &drawn.kind {
        VisualKind::Polyline { width, .. } => assert_eq!(*width, 3),
        other => panic!("{other:?}"),
    }
    let image = paint(&[drawn]);
    assert!(image.pixel(4, 4).unwrap().g > 150);
    assert!(
        image.pixel(4, 5).unwrap().g > 150,
        "lineSize spreads the stroke"
    );
}

#[test]
fn border_color_and_line_width_change_the_outline() {
    let mut label = WidgetNode::new("caLabel", "box");
    label.rect = Rect::new(0, 0, 24, 16);
    label.set_prop("text", PropValue::String("A".into()));
    label.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
    label.set_prop("background", PropValue::Color(Color::rgb(240, 240, 240)));
    label.set_prop("borderColor", PropValue::Color(Color::rgb(220, 0, 0)));
    label.set_prop("lineWidth", PropValue::Double(3.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(label);
    let drawn = open(root, InternSource::new())
        .visuals()
        .into_iter()
        .find(|v| v.name == "box")
        .unwrap();
    assert_eq!(drawn.border, Some(Color::rgb(220, 0, 0)));
    assert_eq!(drawn.border_px, 3);
    let image = paint(&[drawn]);
    assert_eq!(image.pixel(0, 0).unwrap().r, 220);
    assert_eq!(
        image.pixel(1, 1).unwrap().r,
        220,
        "lineWidth is more than one pixel"
    );
    assert!(
        image.pixel(8, 8).unwrap().r > 200,
        "interior stays the background"
    );
}

fn plot_with(style: &str, grid: bool, show_x: bool) -> caqtdm_paint::Image {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("Style_1", PropValue::Enum(style.into()));
    plot.set_prop("grid", PropValue::Bool(grid));
    plot.set_prop("XaxisEnabled", PropValue::Bool(show_x));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let drawn = open(root, intern)
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    paint(&[drawn])
}

#[test]
fn curve_style_grid_and_x_axis_change_the_plot() {
    let lines = plot_with("Lines", true, true);
    let dots = plot_with("Dots", true, true);
    let none = plot_with("NoCurve", false, false);
    assert!(
        lines.pixel(40, 34).unwrap().r > 250,
        "a line crosses mid-scale"
    );
    assert!(
        dots.pixel(40, 34).unwrap().r > 35,
        "dots leave the middle empty"
    );
    let on = lines.pixel(20, 22).unwrap();
    let off = none.pixel(20, 22).unwrap();
    assert!(
        on.r > 100 && on.r < 180 && on.g > 100,
        "grid is Qt gray, got {on:?}"
    );
    assert!(
        off.r > 220 && off.g > 220,
        "grid off leaves the widget background, got {off:?}"
    );
    assert_ne!(
        lines.pixel(18, 47).unwrap(),
        none.pixel(18, 47).unwrap(),
        "hiding the x axis removes the tick"
    );
}

#[test]
fn symbols_x_limits_time_and_grid_color_change_pixels() {
    let plain = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
    });
    let cross = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
        plot.set_prop("symbol_1", PropValue::Enum("Cross".into()));
        plot.set_prop("symbolResizeFactor", PropValue::Double(10.0));
    });
    let ellipse = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
        plot.set_prop("symbol_1", PropValue::Enum("Rect".into()));
        plot.set_prop("symbolResizeFactor", PropValue::Double(10.0));
    });
    assert!(
        plain.pixel(39, 28).unwrap().r > 35,
        "no symbol leaves the arm empty"
    );
    assert!(
        cross.pixel(39, 28).unwrap().r > 250,
        "a cross paints the vertical arm"
    );
    assert!(
        ellipse.pixel(42, 37).unwrap().r > 250,
        "Rect is the ellipse myMarker draws"
    );
    assert!(
        cross.pixel(42, 37).unwrap().r > 35,
        "the cross does not fill the diagonal"
    );

    let wide = styled_plot(|_| {});
    let limited = styled_plot(|plot| {
        plot.set_prop("XaxisScaling", PropValue::Enum("User".into()));
        plot.set_prop("XaxisLimits", PropValue::String("0;6".into()));
    });
    assert!(
        wide.pixel(40, 34).unwrap().r > 250,
        "index scale spans the plot"
    );
    assert!(
        limited.pixel(40, 34).unwrap().r > 35,
        "user limits 0;6 stop index 2 before mid-plot"
    );
    let linear = peaked(false);
    let logged = peaked(true);
    assert!(
        linear.pixel(39, 25).unwrap().r > 250,
        "the middle sample is the linear peak"
    );
    assert!(
        logged.pixel(39, 25).unwrap().r > 35,
        "log10 moves that peak toward the right"
    );

    let timed = styled_plot(|plot| {
        plot.set_prop("XaxisType", PropValue::Enum("time".into()));
    });
    let mut clock = false;
    let mut diffs = 0;
    for y in 0..60 {
        for x in 0..80 {
            if timed.pixel(x, y).unwrap() != wide.pixel(x, y).unwrap() {
                diffs += 1;
                clock = true;
            }
        }
    }
    assert!(clock, "a time axis stamps hh:mm:ss ({diffs} pixels)");

    let red = styled_plot(|plot| {
        plot.set_prop("gridColor", PropValue::Color(Color::rgb(200, 0, 0)));
    });
    let grid = red.pixel(20, 22).unwrap();
    assert!(grid.r > 180 && grid.g < 40, "gridColor replaces Qt gray");
}

fn peaked(log_x: bool) -> caqtdm_paint::Image {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(0.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    plot.set_prop("grid", PropValue::Bool(false));
    if log_x {
        plot.set_prop("XaxisType", PropValue::Enum("log10".into()));
    }
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern.clone()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    for (step, value) in [0.0, 8.0, 0.0].into_iter().enumerate() {
        intern.insert("A", ChannelData::scalar(value));
        session.tick((step as u64 + 1) * 1_000);
    }
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    paint(&[drawn])
}

fn styled_plot(setup: impl Fn(&mut WidgetNode)) -> caqtdm_paint::Image {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    setup(&mut plot);
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut session = Session::open(
        root,
        {
            let mut hub = Hub::new("epics3");
            hub.register(Box::new(intern));
            hub
        },
        &caqtdm_core::MacroMap::new(),
    );
    session.tick(1_000);
    session.tick(2_000);
    session.tick(3_000);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    paint(&[drawn])
}

#[test]
fn waterfall_colormap_and_camera_colormap_change_pixels() {
    let heat = waterfall("spectrum_heat");
    let grey = waterfall("grey");
    let hot = heat.pixel(30, 2).unwrap();
    assert!(hot.r > 200 && hot.b < 30, "heat high is red");
    let gray = grey.pixel(30, 2).unwrap();
    assert!(
        gray.r > 240 && gray.g > 240 && gray.b > 240,
        "grey high is white"
    );

    let intern = InternSource::new();
    let mut bytes = ChannelData::scalar(0.0);
    bytes.value = PvValue::Bytes(vec![0, 255]);
    intern.insert("PIX", bytes);
    let mut camera = WidgetNode::new("caCamera", "cam");
    camera.rect = Rect::new(0, 0, 8, 8);
    camera.set_prop("channelData", PropValue::String("intern://PIX".into()));
    camera.set_prop("dataType", PropValue::String("mono8".into()));
    camera.set_prop("channelWidth", PropValue::Double(2.0));
    camera.set_prop("channelHeight", PropValue::Double(1.0));
    camera.set_prop("ColorMap", PropValue::Enum("spectrum_heat".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(camera);
    let drawn = open(root, intern)
        .visuals()
        .into_iter()
        .find(|v| v.name == "cam")
        .unwrap();
    let image = paint(&[drawn]);
    assert!(image.pixel(2, 4).unwrap().b > 200, "heat low is blue");
    assert!(image.pixel(6, 4).unwrap().r > 200, "heat high is red");
}

fn waterfall(map: &str) -> caqtdm_paint::Image {
    let intern = InternSource::new();
    let mut wave = ChannelData::scalar(0.0);
    wave.value = PvValue::Doubles(vec![0.0, 100.0]);
    intern.insert("WF", wave);
    let mut plot = WidgetNode::new("caWaterfallPlot", "fall");
    plot.rect = Rect::new(0, 0, 40, 20);
    plot.set_prop("channel", PropValue::String("intern://WF".into()));
    plot.set_prop("ColorMap", PropValue::Enum(map.into()));
    plot.set_prop("IntensityMin", PropValue::Double(0.0));
    plot.set_prop("IntensityMax", PropValue::Double(100.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let drawn = open(root, intern)
        .visuals()
        .into_iter()
        .find(|v| v.name == "fall")
        .unwrap();
    paint(&[drawn])
}
