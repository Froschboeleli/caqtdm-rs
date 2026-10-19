//! Desktop viewer. It paints the same visuals as the PPM rasterizer and sends
//! clicks back through [`Session::act`](caqtdm_widgets::Session::act).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use caqtdm_core::{parse_macro_list, MacroMap};
use caqtdm_datasource::Hub;
use caqtdm_ui::{expand_includes, load_path, SearchPath};
use caqtdm_widgets::{
    cell_at, load_image_notes, tab_at, tab_rects, toggled_bit_value, Action, Effect, Session,
    Visual, VisualKind,
};
use eframe::egui;

pub fn launch(
    session: Session,
    path: PathBuf,
    search: SearchPath,
    macros: MacroMap,
    hub_factory: Arc<dyn Fn() -> Hub + Send + Sync>,
) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "caQtDM",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(Viewer {
                stack: vec![Page { session, path }],
                search,
                macros,
                hub_factory,
            }))
        }),
    )
}

struct Page {
    session: Session,
    path: PathBuf,
}

struct Viewer {
    stack: Vec<Page>,
    search: SearchPath,
    macros: MacroMap,
    hub_factory: Arc<dyn Fn() -> Hub + Send + Sync>,
}

impl eframe::App for Viewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(80));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Some(page) = self.stack.last_mut() {
            page.session.tick(now);
        }
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Back").clicked() && self.stack.len() > 1 {
                    self.stack.pop();
                }
                if let Some(page) = self.stack.last() {
                    ui.label(format!("{} widgets", page.session.visuals().len()));
                    if let Some(last) = page.session.log.last() {
                        ui.label(last);
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut visuals = self
                .stack
                .last()
                .map(|page| page.session.visuals())
                .unwrap_or_default();
            let search = self.search.clone();
            load_image_notes(&mut visuals, |note| {
                search.find(note).and_then(|path| std::fs::read(path).ok())
            });
            let width = visuals
                .iter()
                .map(|v| v.rect.x + v.rect.width)
                .max()
                .unwrap_or(400) as f32;
            let height = visuals
                .iter()
                .map(|v| v.rect.y + v.rect.height)
                .max()
                .unwrap_or(300) as f32;
            let (response, painter) = ui.allocate_painter(
                egui::vec2(width.max(1.0), height.max(1.0)),
                egui::Sense::click_and_drag(),
            );
            let mut textures = Vec::new();
            for visual in &visuals {
                draw(ctx, &painter, visual, &mut textures);
            }
            let _hold_textures = textures.len();
            if response.clicked() || response.drag_stopped() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(hit) = hit_test(&visuals, pos) {
                        self.handle(hit, pos, response.drag_stopped());
                    }
                }
            }
        });
    }
}

impl Viewer {
    fn handle(&mut self, visual: Visual, pos: egui::Pos2, dragged: bool) {
        let action = action_for(&visual, pos.x, pos.y, dragged);
        let effects = {
            let Some(page) = self.stack.last_mut() else {
                return;
            };
            page.session.act(action)
        };
        let mut opened = Vec::new();
        let mut reload = false;
        for effect in effects {
            match effect {
                Effect::Open { file, macros } => opened.push((file, macros)),
                Effect::SetMacro { key, value } => {
                    self.macros.insert(key, value);
                    reload = true;
                }
                Effect::Message(_) | Effect::Shell { .. } => {}
            }
        }
        if reload {
            self.reload_top();
        }
        for (file, extra) in opened {
            self.open_related(&file, &extra);
        }
    }

    fn reload_top(&mut self) {
        let Some(path) = self.stack.last().map(|page| page.path.clone()) else {
            return;
        };
        let Ok(mut display) = load_path(&path) else {
            return;
        };
        match expand_includes(&mut display.root, &self.search, &self.macros) {
            Ok(warnings) => {
                if let Some(page) = self.stack.last_mut() {
                    page.session.log.extend(warnings);
                }
            }
            Err(err) => {
                if let Some(page) = self.stack.last_mut() {
                    page.session.log.push(err.to_string());
                }
                return;
            }
        }
        let session = Session::open(display.root, (self.hub_factory)(), &self.macros);
        if let Some(page) = self.stack.last_mut() {
            page.session = session;
        }
    }

    fn open_related(&mut self, file: &str, extra: &str) {
        let Some(path) = self.search.resolve_include(file) else {
            if let Some(page) = self.stack.last_mut() {
                page.session.log.push(format!("missing display {file}"));
            }
            return;
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                self.search.origin = Some(parent.to_path_buf());
            }
        }
        let Ok(mut display) = load_path(&path) else {
            return;
        };
        let mut macros = self.macros.clone();
        macros.extend_from(&parse_macro_list(extra));
        match expand_includes(&mut display.root, &self.search, &macros) {
            Ok(warnings) => {
                if let Some(page) = self.stack.last_mut() {
                    page.session.log.extend(warnings);
                }
            }
            Err(err) => {
                if let Some(page) = self.stack.last_mut() {
                    page.session.log.push(err.to_string());
                }
                return;
            }
        }
        let session = Session::open(display.root, (self.hub_factory)(), &macros);
        self.stack.push(Page { session, path });
    }
}

fn draw(
    ctx: &egui::Context,
    painter: &egui::Painter,
    visual: &Visual,
    textures: &mut Vec<egui::TextureHandle>,
) {
    if !visual.visible {
        return;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(visual.rect.x as f32, visual.rect.y as f32),
        egui::vec2(
            visual.rect.width.max(1) as f32,
            visual.rect.height.max(1) as f32,
        ),
    );
    let fg = color(visual.foreground);
    let bg = color(visual.background);
    if visual.background.a > 0 {
        painter.rect_filled(rect, 2.0, bg);
    }
    if visual.border_px > 0 {
        let edge = visual.border.map(color).unwrap_or(fg);
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(visual.border_px as f32, edge),
            egui::StrokeKind::Inside,
        );
    }
    if let VisualKind::Slider { fraction, .. } | VisualKind::Thermo { fraction, .. } = &visual.kind
    {
        let mut bar = rect;
        bar.set_width(rect.width() * *fraction as f32);
        painter.rect_filled(bar, 0.0, fg);
    }
    if let VisualKind::Image {
        rgba,
        width,
        height,
        ..
    } = &visual.kind
    {
        if *width > 0 && *height > 0 && rgba.len() >= (*width as usize * *height as usize * 4) {
            let image =
                egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], rgba);
            let texture = ctx.load_texture(
                format!("ca-image-{}", visual.id),
                image,
                egui::TextureOptions::NEAREST,
            );
            painter.image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            textures.push(texture);
        }
    }
    if let VisualKind::Tabs { labels, current } = &visual.kind {
        let bar_h = 28.0_f32.min(rect.height());
        let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), bar_h));
        painter.rect_filled(bar, 0.0, egui::Color32::from_rgb(225, 225, 225));
        let ink = egui::Color32::from_rgb(20, 20, 20);
        for (index, (label, tab)) in labels
            .iter()
            .zip(tab_rects(visual.rect, labels, *current, visual.font_px))
            .enumerate()
        {
            let chip = egui::Rect::from_min_size(
                egui::pos2(tab.x as f32, tab.y as f32),
                egui::vec2(tab.width as f32, tab.height as f32),
            );
            let fill = if index == *current {
                egui::Color32::from_rgb(250, 250, 250)
            } else {
                egui::Color32::from_rgb(216, 216, 216)
            };
            painter.rect_filled(chip, 0.0, fill);
            painter.rect_stroke(
                chip,
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(196, 196, 195)),
                egui::StrokeKind::Inside,
            );
            painter.text(
                chip.left_top() + egui::vec2(6.0, 4.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::monospace(visual.font_px.clamp(8, 14) as f32),
                ink,
            );
        }
    }
    if let VisualKind::Led { on } = &visual.kind {
        let fill = if *on || visual.class_name == "caLed" {
            fg
        } else {
            egui::Color32::DARK_GRAY
        };
        painter.circle_filled(rect.center(), rect.width().min(rect.height()) * 0.4, fill);
    }
    if !visual.text.is_empty() {
        let (pos, align) = match visual.align {
            1 => (
                rect.center_top() + egui::vec2(0.0, 2.0),
                egui::Align2::CENTER_TOP,
            ),
            2 => (
                rect.right_top() + egui::vec2(-3.0, 2.0),
                egui::Align2::RIGHT_TOP,
            ),
            _ => (
                rect.left_top() + egui::vec2(3.0, 2.0),
                egui::Align2::LEFT_TOP,
            ),
        };
        painter.text(pos, align, &visual.text, egui::FontId::monospace(13.0), fg);
    }
}

fn color(c: caqtdm_core::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn hit_test(visuals: &[Visual], pos: egui::Pos2) -> Option<Visual> {
    hit_visual(visuals, pos.x, pos.y)
}

/// The pointer handler the viewer runs after a click or a drag.
///
/// `x` and `y` are display pixels. `dragged` is true when the pointer was
/// released at the end of a drag, which is how a plot zoom is distinguished
/// from a cursor click.
pub fn action_for(visual: &Visual, x: f32, y: f32, dragged: bool) -> Action {
    match &visual.kind {
        VisualKind::Bits { value, count, .. } if visual.class_name == "caByteController" => {
            Action::SetNumber {
                id: visual.id,
                value: toggled_bit_value(*value, *count, x, visual.rect.x, visual.rect.width),
            }
        }
        VisualKind::Plot { y_min, y_max, .. } => {
            if dragged {
                let (lo, hi) = if y_max > y_min {
                    (*y_min, *y_max)
                } else {
                    (0.0, 100.0)
                };
                let y_span = visual.rect.height.max(1) as f32;
                let y_fraction = ((y - visual.rect.y as f32) / y_span).clamp(0.0, 1.0) as f64;
                let mid = lo + (hi - lo) * (1.0 - y_fraction);
                let half = (hi - lo) / 4.0;
                let x_span = visual.rect.width.max(1) as f32;
                let x_fraction = ((x - visual.rect.x as f32) / x_span).clamp(0.0, 1.0) as f64;
                Action::Zoom {
                    id: visual.id,
                    x_min: (x_fraction - 0.25).clamp(0.0, 1.0),
                    x_max: (x_fraction + 0.25).clamp(0.0, 1.0),
                    y_min: mid - half,
                    y_max: mid + half,
                }
            } else {
                let span = visual.rect.width.max(1) as f32;
                let at = ((x - visual.rect.x as f32) / span).clamp(0.0, 1.0) as f64;
                Action::PlaceCursor { id: visual.id, at }
            }
        }
        VisualKind::Slider { min, max, .. } => {
            let span = visual.rect.width.max(1) as f32;
            let fraction = ((x - visual.rect.x as f32) / span).clamp(0.0, 1.0);
            Action::SetNumber {
                id: visual.id,
                value: min + (max - min) * f64::from(fraction),
            }
        }
        VisualKind::Tabs { labels, current } if !labels.is_empty() => Action::Choose {
            id: visual.id,
            index: tab_at(visual.rect, labels, *current, visual.font_px, x, y),
        },
        VisualKind::Menu { items, .. } if !items.is_empty() => Action::Choose {
            id: visual.id,
            index: row_index(y, visual.rect.y, 16.0, items.len()),
        },
        VisualKind::Choice {
            items,
            columns,
            inverse,
            ..
        } if !items.is_empty() => Action::Choose {
            id: visual.id,
            index: cell_at(visual.rect, x, y, items.len(), *columns, *inverse),
        },
        VisualKind::Grid { columns, cells } if !cells.is_empty() => Action::Choose {
            id: visual.id,
            index: cell_at(visual.rect, x, y, cells.len(), *columns, false),
        },
        VisualKind::Related { entries } if entries.len() > 1 => Action::Choose {
            id: visual.id,
            index: row_index(y, visual.rect.y, 18.0, entries.len()),
        },
        VisualKind::Tree { rows } if rows.iter().any(|row| row.group) => Action::ToggleGroup {
            id: visual.id,
            index: row_index(y, visual.rect.y, 16.0, rows.len()),
        },
        _ => {
            if dragged && (visual.class_name == "caCamera" || visual.class_name == "caScan2D") {
                Action::Roi {
                    id: visual.id,
                    x0: f64::from(visual.rect.x),
                    y0: f64::from(visual.rect.y),
                    x1: f64::from(x as i32),
                    y1: f64::from(y as i32),
                }
            } else {
                Action::Press { id: visual.id }
            }
        }
    }
}

/// Topmost visible widget whose rectangle contains the point.
pub fn hit_visual(visuals: &[Visual], x: f32, y: f32) -> Option<Visual> {
    visuals
        .iter()
        .rev()
        .find(|v| v.visible && v.rect.contains(x, y))
        .cloned()
}

fn row_index(y: f32, top: i32, row: f32, len: usize) -> usize {
    let index = ((y - top as f32) / row).floor() as isize;
    if index < 0 {
        0
    } else {
        (index as usize).min(len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::{action_for, hit_visual};
    use caqtdm_core::{Color, Rect, Severity};
    use caqtdm_widgets::{Action, Visual, VisualKind};

    fn visual(kind: VisualKind) -> Visual {
        Visual {
            id: 7,
            class_name: "widget".into(),
            name: "w".into(),
            rect: Rect::new(0, 0, 100, 40),
            foreground: Color::rgb(0, 0, 0),
            background: Color::rgb(255, 255, 255),
            text: String::new(),
            kind,
            visible: true,
            severity: Severity::NoAlarm,
            font_px: 10,
            rotation: 0,
            align: 0,
            border: None,
            border_px: 1,
        }
    }

    #[test]
    fn pointer_events_build_the_viewer_actions() {
        let slider = visual(VisualKind::Slider {
            fraction: 0.0,
            min: 0.0,
            max: 10.0,
        });
        assert_eq!(
            action_for(&slider, 50.0, 10.0, true),
            Action::SetNumber { id: 7, value: 5.0 }
        );

        let plot = visual(VisualKind::Plot {
            series: vec![vec![0.0, 10.0]],
            title: String::new(),
            title_x: String::new(),
            title_y: String::new(),
            x_min: 0.0,
            x_max: 1.0,
            y_min: 0.0,
            y_max: 100.0,
            y2_min: f64::NAN,
            y2_max: f64::NAN,
            log_y: false,
            x_ticks: 2,
            cursors: Vec::new(),
            legend: Vec::new(),
            colors: Vec::new(),
            axes: Vec::new(),
            styles: Vec::new(),
            grid: true,
            show_x: true,
            show_y: true,
            symbols: Vec::new(),
            symbol_px: 2,
            x_limits: None,
            log_x: false,
            time_x: false,
            grid_color: Color::rgb(160, 160, 160),
            scale_color: None,
            xs: Vec::new(),
            align_px: 0,
            time_at: 0.0,
        });
        assert_eq!(
            action_for(&plot, 50.0, 20.0, false),
            Action::PlaceCursor { id: 7, at: 0.5 }
        );
        match action_for(&plot, 50.0, 20.0, true) {
            Action::Zoom {
                x_min,
                x_max,
                y_min,
                y_max,
                ..
            } => {
                assert!((x_min - 0.25).abs() < 1e-9);
                assert!((x_max - 0.75).abs() < 1e-9);
                assert!(y_max > y_min);
            }
            other => panic!("{other:?}"),
        }

        let mut tabs = visual(VisualKind::Tabs {
            labels: vec!["One".into(), "Two".into()],
            current: 0,
        });
        tabs.rect = Rect::new(0, 0, 200, 30);
        assert_eq!(
            action_for(&tabs, 60.0, 4.0, false),
            Action::Choose { id: 7, index: 1 }
        );

        let menu = visual(VisualKind::Menu {
            items: vec!["off".into(), "on".into()],
            selected: 0,
        });
        assert_eq!(
            action_for(&menu, 4.0, 20.0, false),
            Action::Choose { id: 7, index: 1 }
        );

        let mut back = visual(VisualKind::Label);
        back.rect = Rect::new(0, 0, 80, 40);
        back.id = 1;
        let mut front = visual(VisualKind::Button);
        front.rect = Rect::new(10, 10, 20, 20);
        front.id = 2;
        let mut camera = visual(VisualKind::Image {
            note: String::new(),
            width: 4,
            height: 4,
            rgba: vec![0; 64],
            roi: None,
        });
        camera.class_name = "caCamera".into();
        camera.rect = Rect::new(2, 3, 20, 20);
        assert_eq!(
            action_for(&camera, 12.0, 14.0, true),
            Action::Roi {
                id: 7,
                x0: 2.0,
                y0: 3.0,
                x1: 12.0,
                y1: 14.0,
            }
        );

        let hit = hit_visual(&[back, front], 15.0, 15.0).unwrap();
        assert_eq!(hit.id, 2);
        assert!(hit_visual(&[hit], 0.0, 0.0).is_none());
    }
}
