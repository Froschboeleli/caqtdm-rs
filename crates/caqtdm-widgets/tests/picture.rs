use caqtdm_core::{ChannelData, Color, PropValue, PvValue, Rect};
use caqtdm_datasource::{Hub, InternSource};
use caqtdm_paint::{paint, plot_canvas, plot_interior, plot_sample_x, plot_value_y};
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
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
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
    let inner = plot_interior(&drawn).unwrap();
    let image = paint(&[drawn]);
    let mid = inner.x + inner.width / 2;
    assert!(
        image
            .pixel(mid, plot_value_y(inner, 500.0, 0.0, 1000.0))
            .unwrap()
            .r
            > 180,
        "second axis places 500 mid-scale"
    );
    assert!(
        image.pixel(inner.x + 2, inner.y + 1).unwrap().r < 80,
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
    // The text is left-aligned, so the interior right of it keeps the fill.
    assert!(
        image.pixel(19, 8).unwrap().r > 200,
        "interior stays the background"
    );
}

/// A painted plot and the canvas interior its curves are drawn in.
type Plotted = (caqtdm_paint::Image, Rect);

/// The row a horizontal grid line of value `value` lands on (Qwt rounds).
fn grid_row(inner: Rect, value: f64, lo: f64, hi: f64) -> i32 {
    let frac = (value - lo) / (hi - lo);
    inner.y + inner.height - 1 - (frac * f64::from(inner.height - 1)).round() as i32
}

fn plot_with(
    style: &str,
    grid: bool,
    show_x: bool,
) -> (caqtdm_paint::Image, Rect, caqtdm_widgets::Visual) {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
    plot.set_prop("scaleColor", PropValue::Color(Color::rgb(200, 200, 200)));
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
    let inner = plot_interior(&drawn).unwrap();
    (paint(std::slice::from_ref(&drawn)), inner, drawn)
}

#[test]
fn curve_style_grid_and_x_axis_change_the_plot() {
    let (lines, inner, _) = plot_with("Lines", true, true);
    let (dots, _, _) = plot_with("Dots", true, true);
    let (none, bare, _) = plot_with("NoCurve", false, false);
    let mid = inner.x + inner.width / 2;
    let y5 = plot_value_y(inner, 5.0, 0.0, 10.0);
    assert!(
        lines.pixel(mid, y5).unwrap().r > 250,
        "a line crosses mid-scale"
    );
    assert!(
        dots.pixel(mid, y5).unwrap().r < 40,
        "dots leave the middle empty"
    );
    let row = grid_row(inner, 6.0, 0.0, 10.0);
    let on = lines.pixel(inner.x + 1, row).unwrap();
    let off = none
        .pixel(bare.x + 1, grid_row(bare, 6.0, 0.0, 10.0))
        .unwrap();
    assert!(
        on.r > 100 && on.r < 180 && on.g > 100,
        "grid is Qt gray, got {on:?}"
    );
    assert!(
        off.r < 20 && off.g < 20,
        "grid off leaves the widget fill, got {off:?}"
    );
    // Qt DashLine at width 1 is 4 on, 2 off. The line starts at the canvas edge.
    let fourth = lines.pixel(inner.x + 3, row).unwrap();
    let gap = lines.pixel(inner.x + 4, row).unwrap();
    assert!(
        fourth.r > 100 && fourth.r < 180 && fourth.g > 100,
        "DashLine keeps the fourth pixel on, got {fourth:?}"
    );
    assert!(
        gap.r < 20 && gap.g < 20,
        "DashLine turns the fifth pixel off, got {gap:?}"
    );
    // The first major tick sits under the data rectangle's left edge, just
    // below the canvas frame.
    let canvas = plot_canvas(&plot_with("Lines", true, true).2).unwrap();
    let tick = (inner.x, canvas.y + canvas.height + 3);
    assert_ne!(
        lines.pixel(tick.0, tick.1).unwrap(),
        none.pixel(tick.0, tick.1).unwrap(),
        "hiding the x axis removes the tick"
    );
}

#[test]
fn symbols_x_limits_time_and_grid_color_change_pixels() {
    let (plain, inner) = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
    });
    let (cross, _) = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
        plot.set_prop("symbol_1", PropValue::Enum("Cross".into()));
        plot.set_prop("symbolResizeFactor", PropValue::Double(10.0));
    });
    let (ellipse, _) = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
        plot.set_prop("symbol_1", PropValue::Enum("Rect".into()));
        plot.set_prop("symbolResizeFactor", PropValue::Double(10.0));
    });
    // Three ticks put the middle sample of value 5 at the canvas center.
    let cx = plot_sample_x(inner, 1, 3);
    let cy = plot_value_y(inner, 5.0, 0.0, 10.0);
    assert!(
        plain.pixel(cx, cy - 5).unwrap().r < 40,
        "no symbol leaves the arm empty"
    );
    assert!(
        cross.pixel(cx, cy - 5).unwrap().r > 250,
        "a cross paints the vertical arm"
    );
    assert!(
        ellipse.pixel(cx + 3, cy + 3).unwrap().r > 250,
        "Rect is the ellipse myMarker draws"
    );
    assert!(
        cross.pixel(cx + 3, cy + 3).unwrap().r < 40,
        "the cross does not fill the diagonal"
    );

    let (wide, _) = styled_plot(|_| {});
    let (limited, _) = styled_plot(|plot| {
        plot.set_prop("XaxisScaling", PropValue::Enum("User".into()));
        plot.set_prop("XaxisLimits", PropValue::String("0;6".into()));
    });
    let mid = inner.x + inner.width / 2;
    assert!(
        wide.pixel(mid, cy).unwrap().r > 250,
        "index scale spans the plot"
    );
    assert!(
        limited.pixel(mid, cy).unwrap().r < 40,
        "user limits 0;6 stop index 2 before mid-plot"
    );
    let (linear, peak) = peaked(false);
    let (logged, _) = peaked(true);
    let peak_at = (
        plot_sample_x(peak, 1, 3),
        plot_value_y(peak, 8.0, 0.0, 10.0),
    );
    assert!(
        linear.pixel(peak_at.0, peak_at.1).unwrap().r > 250,
        "the middle sample is the linear peak"
    );
    assert!(
        logged.pixel(peak_at.0, peak_at.1).unwrap().r < 40,
        "log10 moves that peak toward the right"
    );

    let (timed, _) = styled_plot(|plot| {
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

    let (red, _) = styled_plot(|plot| {
        plot.set_prop("gridColor", PropValue::Color(Color::rgb(200, 0, 0)));
    });
    let grid = red
        .pixel(inner.x + 1, grid_row(inner, 6.0, 0.0, 10.0))
        .unwrap();
    assert!(grid.r > 180 && grid.g < 40, "gridColor replaces Qt gray");
}

fn peaked(log_x: bool) -> Plotted {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(0.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
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
    let inner = plot_interior(&drawn).unwrap();
    (paint(&[drawn]), inner)
}

fn styled_plot(setup: impl Fn(&mut WidgetNode)) -> Plotted {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
    plot.set_prop("scaleColor", PropValue::Color(Color::rgb(200, 200, 200)));
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
    let inner = plot_interior(&drawn).unwrap();
    (paint(&[drawn]), inner)
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

#[test]
fn cartesian_vectors_count_trigger_and_erase_change_the_trace() {
    let intern = InternSource::new();
    let mut xs = ChannelData::scalar(0.0);
    xs.value = PvValue::Doubles(vec![0.0, 9.0, 10.0]);
    let mut ys = ChannelData::scalar(0.0);
    ys.value = PvValue::Doubles(vec![0.0, 8.0, 0.0]);
    intern.insert("X", xs);
    intern.insert("Y", ys);
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop(
        "channels_1",
        PropValue::String("intern://X;intern://Y".into()),
    );
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    plot.set_prop("grid", PropValue::Bool(false));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(1_000);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    match &drawn.kind {
        VisualKind::Plot { series, xs, .. } => {
            assert_eq!(series[0], vec![0.0, 8.0, 0.0]);
            assert_eq!(xs[0], vec![0.0, 9.0, 10.0]);
        }
        other => panic!("{other:?}"),
    }
    let inner = plot_interior(&drawn).unwrap();
    let image = paint(&[drawn]);
    let y8 = plot_value_y(inner, 8.0, 0.0, 10.0);
    let x9 = inner.x + (0.9 * f64::from(inner.width - 1)) as i32;
    assert!(image.pixel(x9, y8).unwrap().r > 250, "the peak sits at X=9");
    assert!(
        image.pixel(plot_sample_x(inner, 1, 3), y8).unwrap().r < 250,
        "index 1 is not the X position"
    );

    assert_eq!(
        scalar_trace("PlotNPointsAndStop", "2", &[1.0, 2.0, 9.0], None),
        vec![1.0, 2.0]
    );
    assert_eq!(
        scalar_trace("PlotLastNPoints", "2", &[1.0, 2.0, 9.0], None),
        vec![2.0, 9.0]
    );
    assert_eq!(
        scalar_trace(
            "PlotNPointsAndStop",
            "intern://C",
            &[1.0, 2.0, 9.0],
            Some(2.0)
        ),
        vec![1.0, 2.0]
    );
    assert_eq!(
        trigger_trace(4.0, 0.0),
        vec![1.0],
        "a steady trigger keeps the previous point"
    );
    assert_eq!(trigger_trace(4.0, 1.0), vec![1.0, 4.0]);

    let (before, after) = erase_trace();
    assert_eq!(before, vec![8.0, 8.0]);
    assert!(after.is_empty(), "ifnotzero clears when erase becomes 1");
}

fn scalar_trace(mode: &str, count: &str, values: &[f64], count_channel: Option<f64>) -> Vec<f64> {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(values[0]));
    if let Some(count) = count_channel {
        intern.insert("C", ChannelData::scalar(count));
    }
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 40, 30);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("plotMode", PropValue::Enum(mode.into()));
    plot.set_prop("countNumOrChannel", PropValue::String(count.into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern.clone()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    for (step, value) in values.iter().enumerate() {
        intern.insert("A", ChannelData::scalar(*value));
        session.tick((step as u64 + 1) * 1_000);
    }
    series_of(&session)
}

fn trigger_trace(second: f64, next_trigger: f64) -> Vec<f64> {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(1.0));
    intern.insert("T", ChannelData::scalar(0.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 40, 30);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("countNumOrChannel", PropValue::String("3".into()));
    plot.set_prop("triggerChannel", PropValue::String("intern://T".into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern.clone()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(1_000);
    intern.insert("A", ChannelData::scalar(second));
    session.tick(2_000);
    intern.insert("T", ChannelData::scalar(next_trigger));
    session.tick(3_000);
    series_of(&session)
}

fn erase_trace() -> (Vec<f64>, Vec<f64>) {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(8.0));
    intern.insert("E", ChannelData::scalar(0.0));
    let mut plot = WidgetNode::new("caCartesianPlot", "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    plot.set_prop("channel", PropValue::String("intern://A".into()));
    plot.set_prop("countNumOrChannel", PropValue::String("2".into()));
    plot.set_prop("eraseChannel", PropValue::String("intern://E".into()));
    plot.set_prop("eraseMode", PropValue::Enum("ifnotzero".into()));
    plot.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    plot.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    plot.set_prop("grid", PropValue::Bool(false));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(0, 0, 0)));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern.clone()));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session.tick(1_000);
    session.tick(2_000);
    let before = series_of(&session);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    let inner = plot_interior(&drawn).unwrap();
    let probe = (
        inner.x + inner.width / 2,
        plot_value_y(inner, 8.0, 0.0, 10.0),
    );
    let ink = paint(&[drawn]);
    assert!(
        ink.pixel(probe.0, probe.1).unwrap().r > 250,
        "the trace is painted"
    );
    intern.insert("E", ChannelData::scalar(1.0));
    session.tick(3_000);
    let after = series_of(&session);
    let drawn = session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap();
    let cleared = paint(&[drawn]);
    assert!(
        cleared.pixel(probe.0, probe.1).unwrap().r < 40,
        "erase removes the painted trace"
    );
    (before, after)
}

fn series_of(session: &Session) -> Vec<f64> {
    match &session
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap()
        .kind
    {
        VisualKind::Plot { series, .. } => series[0].clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn strip_refresh_period_sync_and_archive_backfill() {
    let low = strip_samples("Low", &[0, 100, 200, 300, 400]);
    let high = strip_samples("High", &[0, 100, 200, 300, 400]);
    assert_eq!(low.len(), 2, "Low refresh waits 400 ms");
    assert_eq!(high.len(), 5, "High refresh waits 100 ms");

    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut narrow = WidgetNode::new("caStripPlot", "narrow");
    narrow.rect = Rect::new(0, 0, 80, 60);
    narrow.set_prop("channel", PropValue::String("intern://A".into()));
    narrow.set_prop("XaxisSyncGroup", PropValue::Double(3.0));
    narrow.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    narrow.set_prop("YaxisLimitsMax_1", PropValue::Double(10.0));
    let mut wide = WidgetNode::new("caStripPlot", "wide");
    wide.rect = Rect::new(0, 70, 80, 60);
    wide.set_prop("channel", PropValue::String("intern://A".into()));
    wide.set_prop("XaxisSyncGroup", PropValue::Double(3.0));
    wide.set_prop("YaxisLimitsMin_1", PropValue::Double(0.0));
    wide.set_prop("YaxisLimitsMax_1", PropValue::Double(100_000.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.extend([narrow, wide]);
    let drawn = open(root, intern);
    let narrow = drawn
        .visuals()
        .into_iter()
        .find(|v| v.name == "narrow")
        .unwrap();
    match &narrow.kind {
        VisualKind::Plot { align_px, .. } => assert_eq!(*align_px, 20),
        other => panic!("{other:?}"),
    }

    let intern = InternSource::new();
    intern.insert("VAC", ChannelData::scalar(4.0));
    let mut strip = WidgetNode::new("caStripPlot", "strip");
    strip.rect = Rect::new(0, 0, 80, 40);
    strip.set_prop("channel", PropValue::String("intern://VAC".into()));
    strip.set_prop("secondsPast", PropValue::Double(10.0));
    strip.set_prop("nrOfBins", PropValue::Double(2.0));
    strip.set_prop("secondsUpdate", PropValue::Double(1_000.0));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(strip);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    session
        .set_archive_fixture("channel VAC\n0,1,0\n5000,3,0\n9000,9,0\n")
        .unwrap();
    session.tick(10_000);
    match &session.visuals()[1].kind {
        VisualKind::Plot { series, .. } => assert_eq!(series[0], vec![1.0, 6.0]),
        other => panic!("{other:?}"),
    }
    session.tick(10_500);
    let urls = session.archive_requests();
    assert_eq!(urls.len(), 1, "secondsUpdate holds the next request");
    assert!(urls[0].contains("/api/4/binned?"), "{}", urls[0]);
    assert!(urls[0].contains("channelName=VAC"));
    assert!(urls[0].contains("binCount=2"));
    assert!(urls[0].contains("backend=sf-archiver"));
    let (hms, date) = caqtdm_paint::axis_time_lines(0, 0);
    assert_eq!((hms.as_str(), date.as_str()), ("00:00:00", "01-01-70"));
    let (hms, date) = caqtdm_paint::axis_time_lines(23 * 3_600, 2 * 3_600);
    assert_eq!((hms.as_str(), date.as_str()), ("01:00:00", "02-01-70"));
    let (ellipse, inner) = styled_plot(|plot| {
        plot.set_prop("Style_1", PropValue::Enum("NoCurve".into()));
        plot.set_prop("symbol_1", PropValue::Enum("Ellipse".into()));
        plot.set_prop("symbolResizeFactor", PropValue::Double(10.0));
        plot.set_prop("grid", PropValue::Bool(false));
    });
    let edge = ellipse
        .pixel(
            plot_sample_x(inner, 1, 3) + 6,
            plot_value_y(inner, 5.0, 0.0, 10.0) + 3,
        )
        .unwrap();
    assert!(
        edge.r > 20 && edge.r < 250,
        "the ellipse edge is antialiased, got {edge:?}"
    );
}

#[test]
fn omitted_plot_colors_follow_the_constructor() {
    // The constructor foreground (white) fills the widget; the cartesian
    // canvas is transparent, so the black constructor background never shows.
    let cartesian = bare_plot("caCartesianPlot");
    assert_eq!(cartesian.foreground, Color::rgb(255, 255, 255));
    assert_eq!(cartesian.background, Color::rgb(255, 255, 255));
    match &cartesian.kind {
        VisualKind::Plot {
            scale_color,
            colors,
            canvas,
            ..
        } => {
            assert_eq!(*scale_color, Some(Color::rgb(0, 0, 0)));
            assert_eq!(*canvas, None);
            assert_eq!(colors[3], Some(Color::rgb(0, 0, 255)));
            assert_eq!(colors[5], Some(Color::rgb(255, 0, 255)));
        }
        other => panic!("{other:?}"),
    }
    let inner = plot_interior(&cartesian).unwrap();
    let cartesian_again = cartesian.clone();
    let image = paint(&[cartesian]);
    // Sampled off the mid-scale grid line.
    assert_eq!(
        image
            .pixel(inner.x + inner.width / 2, inner.y + inner.height / 2 + 3)
            .unwrap(),
        Color::rgb(255, 255, 255),
        "canvas shows the widget fill"
    );
    // Left of the canvas frame, on a row no major tick reaches.
    let canvas = plot_canvas(&cartesian_again).unwrap();
    assert_eq!(
        image
            .pixel(canvas.x - 4, inner.y + inner.height / 2 + 3)
            .unwrap(),
        Color::rgb(255, 255, 255),
        "scale well between two ticks"
    );
    assert_eq!(
        image
            .pixel(inner.x - 5, inner.y + inner.height - 1)
            .unwrap(),
        Color::rgb(0, 0, 0),
        "black tick on the well"
    );

    // A strip plot paints its canvas with the (black) background.
    let strip = bare_plot("caStripPlot");
    assert_eq!(strip.foreground, Color::rgb(133, 190, 232));
    assert_eq!(strip.background, Color::rgb(133, 190, 232));
    match &strip.kind {
        VisualKind::Plot { colors, canvas, .. } => {
            assert_eq!(*canvas, Some(Color::rgb(0, 0, 0)));
            assert_eq!(colors[3], Some(Color::rgb(0, 255, 255)));
            assert_eq!(colors[4], Some(Color::rgb(255, 0, 255)));
            assert_eq!(colors[5], Some(Color::rgb(0, 255, 0)));
        }
        other => panic!("{other:?}"),
    }
    let mut plot = WidgetNode::new("caCartesianPlot", "chosen");
    plot.rect = Rect::new(0, 0, 40, 30);
    plot.set_prop("background", PropValue::Color(Color::rgb(10, 20, 30)));
    plot.set_prop("foreground", PropValue::Color(Color::rgb(1, 2, 3)));
    plot.set_prop("scaleColor", PropValue::Color(Color::rgb(4, 5, 6)));
    plot.set_prop("color_4", PropValue::Color(Color::rgb(7, 8, 9)));
    for (index, name) in ["A", "B", "C", "D"].into_iter().enumerate() {
        plot.set_prop(
            format!("channels_{}", index + 1),
            PropValue::String(format!("intern://{name}")),
        );
    }
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    let chosen = open(root, InternSource::new())
        .visuals()
        .into_iter()
        .find(|v| v.name == "chosen")
        .unwrap();
    assert_eq!(chosen.background, Color::rgb(1, 2, 3), "widget fill");
    assert_eq!(chosen.foreground, Color::rgb(1, 2, 3));
    match chosen.kind {
        VisualKind::Plot {
            scale_color,
            colors,
            ..
        } => {
            assert_eq!(scale_color, Some(Color::rgb(4, 5, 6)));
            assert_eq!(colors[3], Some(Color::rgb(7, 8, 9)));
        }
        other => panic!("{other:?}"),
    }
}

fn bare_plot(class_name: &str) -> caqtdm_widgets::Visual {
    let mut plot = WidgetNode::new(class_name, "plot");
    plot.rect = Rect::new(0, 0, 80, 60);
    for (index, name) in ["A", "B", "C", "D", "E", "F"].into_iter().enumerate() {
        plot.set_prop(
            format!("channels_{}", index + 1),
            PropValue::String(format!("intern://{name}")),
        );
    }
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(plot);
    open(root, InternSource::new())
        .visuals()
        .into_iter()
        .find(|v| v.name == "plot")
        .unwrap()
}

fn strip_samples(refresh: &str, ticks: &[u64]) -> Vec<f64> {
    let intern = InternSource::new();
    intern.insert("A", ChannelData::scalar(5.0));
    let mut strip = WidgetNode::new("caStripPlot", "strip");
    strip.rect = Rect::new(0, 0, 80, 40);
    strip.set_prop("channel", PropValue::String("intern://A".into()));
    strip.set_prop("period", PropValue::Double(1.0));
    strip.set_prop("units", PropValue::Enum("Second".into()));
    strip.set_prop("refreshRate", PropValue::Enum(refresh.into()));
    let mut root = WidgetNode::new("QWidget", "root");
    root.children.push(strip);
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(intern));
    let mut session = Session::open(root, hub, &caqtdm_core::MacroMap::new());
    for tick in ticks {
        session.tick(*tick);
    }
    match &session.visuals()[1].kind {
        VisualKind::Plot { series, .. } => series[0].clone(),
        other => panic!("{other:?}"),
    }
}
