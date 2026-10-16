//! Desktop viewer. It paints the same visuals as the PPM rasterizer and sends
//! clicks back through [`Session::act`](caqtdm_widgets::Session::act).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use caqtdm_core::{parse_macro_list, MacroMap};
use caqtdm_datasource::Hub;
use caqtdm_ui::{expand_includes, load_path, SearchPath};
use caqtdm_widgets::{
    load_image_notes, toggled_bit_value, Action, Effect, Session, Visual, VisualKind,
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
            VisualKind::Bits { value, count, .. } if visual.class_name == "caByteController" => {
                Action::SetNumber {
                    id: visual.id,
                    value: toggled_bit_value(
                        *value,
                        *count,
                        pos.x,
                        visual.rect.x,
                        visual.rect.width,
                    ),
                }
            }
            VisualKind::Slider { min, max, .. } => {
                let span = visual.rect.width.max(1) as f32;
                let fraction = ((pos.x - visual.rect.x as f32) / span).clamp(0.0, 1.0);
                Action::SetNumber {
                    id: visual.id,
                    value: min + (max - min) * f64::from(fraction),
                }
            }
            VisualKind::Tabs { labels, .. } if !labels.is_empty() => Action::Choose {
                id: visual.id,
                index: row_index(pos.x, visual.rect.x, 48.0, labels.len()),
            },
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
        if let Err(err) = expand_includes(&mut display.root, &self.search, &self.macros) {
            if let Some(page) = self.stack.last_mut() {
                page.session.log.push(err.to_string());
            }
            return;
        }
        let session = Session::open(display.root, (self.hub_factory)(), &self.macros);
        if let Some(page) = self.stack.last_mut() {
            page.session = session;
        }
    }

    fn open_related(&mut self, file: &str, extra: &str) {
        let Some(path) = self.search.find(file) else {
            if let Some(page) = self.stack.last_mut() {
                page.session.log.push(format!("missing display {file}"));
            }
            return;
        };
        let Ok(mut display) = load_path(&path) else {
            return;
        };
        let mut macros = self.macros.clone();
        macros.extend_from(&parse_macro_list(extra));
        if let Err(err) = expand_includes(&mut display.root, &self.search, &macros) {
            if let Some(page) = self.stack.last_mut() {
                page.session.log.push(err.to_string());
            }
            return;
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
