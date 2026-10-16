//! Desktop viewer. It paints the same visuals as the PPM rasterizer and sends
//! clicks back through [`Session::act`](caqtdm_widgets::Session::act).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use caqtdm_core::{parse_macro_list, MacroMap};
use caqtdm_datasource::Hub;
use caqtdm_ui::{expand_includes, load_path, SearchPath};
use caqtdm_widgets::{Action, Effect, Session, Visual, VisualKind};
use eframe::egui;

pub fn launch(
    session: Session,
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
                stack: vec![session],
                search,
                macros,
                hub_factory,
            }))
        }),
    )
}

struct Viewer {
    stack: Vec<Session>,
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
        if let Some(session) = self.stack.last_mut() {
            session.tick(now);
        }
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Back").clicked() && self.stack.len() > 1 {
                    self.stack.pop();
                }
                if let Some(session) = self.stack.last() {
                    ui.label(format!("{} widgets", session.visuals().len()));
                    if let Some(last) = session.log.last() {
                        ui.label(last);
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let visuals = self.stack.last().map(Session::visuals).unwrap_or_default();
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
            for visual in &visuals {
                draw(&painter, visual);
            }
            if response.clicked() || response.drag_stopped() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(hit) = hit_test(&visuals, pos) {
                        self.handle(hit, pos);
                    }
                }
            }
        });
    }
}

impl Viewer {
    fn handle(&mut self, visual: Visual, pos: egui::Pos2) {
        let action = match &visual.kind {
            VisualKind::Slider { min, max, .. } => {
                let span = visual.rect.width.max(1) as f32;
                let fraction = ((pos.x - visual.rect.x as f32) / span).clamp(0.0, 1.0);
                Action::SetNumber {
                    id: visual.id,
                    value: min + (max - min) * f64::from(fraction),
                }
            }
            VisualKind::Menu { items, .. } if !items.is_empty() => Action::Choose {
                id: visual.id,
                index: row_index(pos.y, visual.rect.y, 16.0, items.len()),
            },
            VisualKind::Related { entries } if entries.len() > 1 => Action::Choose {
                id: visual.id,
                index: row_index(pos.y, visual.rect.y, 18.0, entries.len()),
            },
            _ => Action::Press { id: visual.id },
        };
        let Some(session) = self.stack.last_mut() else {
            return;
        };
        let effects = session.act(action);
        let mut opened = Vec::new();
        for effect in effects {
            if let Effect::Open { file, macros } = effect {
                opened.push((file, macros));
            }
        }
        for (file, extra) in opened {
            self.open_related(&file, &extra);
        }
    }

    fn open_related(&mut self, file: &str, extra: &str) {
        let Some(path) = self.search.find(file) else {
            if let Some(session) = self.stack.last_mut() {
                session.log.push(format!("missing display {file}"));
            }
            return;
        };
        let Ok(mut display) = load_path(&path) else {
            return;
        };
        let mut macros = self.macros.clone();
        macros.extend_from(&parse_macro_list(extra));
        if let Err(err) = expand_includes(&mut display.root, &self.search, &macros) {
            if let Some(session) = self.stack.last_mut() {
                session.log.push(err.to_string());
            }
            return;
        }
        let session = Session::open(display.root, (self.hub_factory)(), &macros);
        self.stack.push(session);
    }
}

fn draw(painter: &egui::Painter, visual: &Visual) {
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
    painter.rect_filled(rect, 2.0, bg);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0_f32, fg),
        egui::StrokeKind::Inside,
    );
    if let VisualKind::Slider { fraction, .. } | VisualKind::Thermo { fraction } = &visual.kind {
        let mut bar = rect;
        bar.set_width(rect.width() * *fraction as f32);
        painter.rect_filled(bar, 0.0, fg);
    }
    if let VisualKind::Led { on } = &visual.kind {
        let fill = if *on { fg } else { egui::Color32::DARK_GRAY };
        painter.circle_filled(rect.center(), rect.width().min(rect.height()) * 0.4, fill);
    }
    if !visual.text.is_empty() {
        painter.text(
            rect.left_top() + egui::vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            &visual.text,
            egui::FontId::monospace(13.0),
            fg,
        );
    }
}

fn color(c: caqtdm_core::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn hit_test(visuals: &[Visual], pos: egui::Pos2) -> Option<Visual> {
    visuals
        .iter()
        .rev()
        .find(|v| v.visible && v.rect.contains(pos.x, pos.y))
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
