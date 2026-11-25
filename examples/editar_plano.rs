//! Windows: cargo run --release --example editar_plano -- samples/plano_demo_chile.pdf
mod support;

use eframe::egui;
use redbim::app::{LaunchConfig, RedBimApp};
use redbim::geometry::math::Rect2;
use redbim::model::document::DocumentTab;
use redbim::model::markup::{Markup, MarkupAttachment, MarkupKind, MarkupStyle};
use redbim::pdf::{base64_encode, io::open_pdf_file};
use serde_json::json;
use std::hash::{DefaultHasher, Hasher};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const IMAGES: usize = 4;
const MEDIA: usize = IMAGES + 1;
const CHANGES: usize = MEDIA + 80;

fn png(seed: usize) -> Vec<u8> {
    let mut state = seed as u32 + 1;
    let pixels = image::RgbImage::from_fn(512, 512, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let [r, g, b, _] = state.to_le_bytes();
        image::Rgb([r, g, b])
    });
    let mut output = Cursor::new(Vec::new());
    pixels
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

fn add_change(tab: &mut DocumentTab, index: usize) {
    tab.push_undo();
    if index == IMAGES {
        let attachment = MarkupAttachment::from_image_bytes("Terreno", &png(index)).unwrap();
        tab.markups.last_mut().unwrap().attachments.push(attachment);
        return;
    }
    let rect = Rect2::from_xywh(
        35. + (index % 10) as f64 * 90.,
        50. + (index / 10) as f64 * 65.,
        75.,
        50.,
    );
    let kind = if index < IMAGES {
        MarkupKind::Image {
            rect,
            png_b64: base64_encode(&png(index)).into(),
            caption: format!("Foto {index}"),
        }
    } else {
        MarkupKind::Rectangle { rect }
    };
    let mark = Markup::new(
        0,
        format!("Prueba {index}"),
        "RedBeam",
        MarkupStyle::default(),
        kind,
    );
    let id = mark.id;
    tab.markups.push(mark);
    tab.select_only(id);
}

// Hash serialized markup contents (including IDs, attachments and order) without copying image data.
#[derive(Default)]
struct Digest(DefaultHasher);
impl Write for Digest {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn digest(tab: &DocumentTab) -> u64 {
    let mut digest = Digest::default();
    serde_json::to_writer(&mut digest, &tab.markups).unwrap();
    digest.0.finish()
}

struct Edit {
    app: RedBimApp,
    step: usize,
    initial: u64,
    edited: u64,
    settle_until: Instant,
    started: Instant,
}

impl Edit {
    fn sample(&self, stage: &str) {
        let tab = &self.app.tabs[0];
        let textures: usize = self
            .app
            .image_textures
            .values()
            .map(|t| t.byte_size())
            .sum();
        println!(
            "{}",
            json!({"stage":stage,"step":self.step,"markups":tab.markups.len(),
            "undo":tab.undo_stack.len(),"redo":tab.redo_stack.len(),"image_textures":self.app.image_textures.len(),
            "texture_bytes":textures, "memory":support::memory()})
        );
    }
}

impl eframe::App for Edit {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        if self.app.force_close_allowed {
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        assert!(
            self.started.elapsed() < Duration::from_secs(120),
            "Recorrido incompleto"
        );
        if Instant::now() < self.settle_until {
            return;
        }
        let tab = &self.app.tabs[0];
        if self.step == 0 {
            if !self.app.base_textures.contains_key(&(tab.id, 0))
                || self.app.thumb_textures.len() != self.app.requested_thumbs.len()
                || self.app.snap_indices.len() != self.app.requested_snaps.len()
                || tab.search_text_by_page.iter().all(String::is_empty)
            {
                return;
            }
            self.initial = digest(tab);
            self.sample("inicial");
        } else if self.step == MEDIA {
            self.sample("imagenes");
        } else if self.step == CHANGES {
            self.edited = digest(tab);
            self.sample("editado");
        } else if self.step == 2 * CHANGES {
            assert_eq!(digest(tab), self.initial);
            assert_eq!(tab.redo_stack.len(), CHANGES);
            self.sample("deshecho");
        } else if self.step == 3 * CHANGES {
            assert_eq!(digest(tab), self.edited);
            assert_eq!(tab.undo_stack.len(), CHANGES);
            self.sample("rehecho");
        } else if self.step == 4 * CHANGES + 1 {
            assert_eq!(tab.undo_stack.len(), 1);
            assert!(tab.redo_stack.is_empty());
            self.sample("nueva_rama");
            self.app.force_close_allowed = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let tab = &mut self.app.tabs[0];
        match self.step {
            n if n < CHANGES => add_change(tab, n),
            n if n < 2 * CHANGES => assert!(tab.undo()),
            n if n < 3 * CHANGES => assert!(tab.redo()),
            n if n < 4 * CHANGES => assert!(tab.undo()),
            _ => add_change(tab, MEDIA),
        }
        self.step += 1;
        if [MEDIA, CHANGES, 2 * CHANGES, 3 * CHANGES, 4 * CHANGES + 1].contains(&self.step) {
            self.settle_until = Instant::now() + Duration::from_millis(400);
        }
    }
}

fn main() -> eframe::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("Indique el PDF de muestra");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280., 800.]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "RedBeam · Memoria al editar",
        options,
        Box::new(move |cc| {
            let mut app = RedBimApp::new(
                cc,
                LaunchConfig {
                    files: vec![],
                    demo: false,
                    light_theme: false,
                    open_measurements_tab: false,
                    screenshot_out: None,
                },
            );
            app.add_and_activate_tab(open_pdf_file(&path).unwrap(), &cc.egui_ctx);
            Ok(Box::new(Edit {
                app,
                step: 0,
                initial: 0,
                edited: 0,
                settle_until: Instant::now() + Duration::from_secs(1),
                started: Instant::now(),
            }))
        }),
    )
}
