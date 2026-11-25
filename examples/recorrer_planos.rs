//! Windows: cargo run --release --example recorrer_planos -- archivo.pdf
use eframe::egui;
use redbim::app::{LaunchConfig, RedBimApp};
use redbim::pdf::io::open_pdf_file;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod support;
use support::memory;

struct Walk {
    app: RedBimApp,
    path: PathBuf,
    warm_frames: u8,
    start: Option<Instant>,
    requested: Instant,
    ready: Option<Instant>,
    returning: bool,
    done: bool,
}

impl Walk {
    fn point(&self, event: &str) {
        let textures = self
            .app
            .base_textures
            .values()
            .map(|t| t.byte_size())
            .sum::<usize>();
        let tab = &self.app.tabs[self.app.active_tab];
        println!(
            "{}",
            json!({
                "event": event, "elapsed_ms": self.start.unwrap().elapsed().as_secs_f64() * 1000.0,
                "page": tab.active_page_index() + 1, "pages": tab.pages.len(),
                "base_pages": self.app.base_textures.len(), "base_bytes": textures,
                "memory": memory()
            })
        );
    }
}

impl eframe::App for Walk {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.app.ui(ui, frame);
        if self.done {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        if self.warm_frames < 3 {
            self.warm_frames += 1;
            return;
        }
        if self.start.is_none() {
            self.requested = Instant::now();
            self.start = Some(self.requested);
            let tab = open_pdf_file(&self.path).expect("abrir PDF de prueba");
            let parse_ms = self.requested.elapsed().as_secs_f64() * 1000.0;
            self.app.add_and_activate_tab(tab, &ctx);
            println!(
                "{}",
                json!({"event": "open", "parse_ms": parse_ms,
                "setup_ms": self.requested.elapsed().as_secs_f64() * 1000.0})
            );
            return;
        }
        assert!(
            self.requested.elapsed() < Duration::from_secs(120),
            "La página no quedó lista"
        );
        let tab = &self.app.tabs[self.app.active_tab];
        let page = tab.active_page_index();
        if !self.app.base_textures.contains_key(&(tab.id, page)) {
            return;
        }
        let Some(ready) = self.ready else {
            self.point(if self.returning { "revisit" } else { "ready" });
            self.ready = Some(Instant::now());
            return;
        };
        if ready.elapsed() < Duration::from_millis(200) {
            return;
        }
        if self.returning {
            let settled = self.app.requested_thumbs.len() == self.app.thumb_textures.len()
                && self.app.requested_snaps.len() == self.app.snap_indices.len();
            if !settled || ready.elapsed() < Duration::from_secs(1) {
                return;
            }
            self.point("end");
            self.done = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let next = if page + 1 < tab.pages.len() {
            page + 1
        } else {
            self.point("last");
            self.returning = true;
            0
        };
        let cam = self.app.tabs[self.app.active_tab].active_camera_mut();
        cam.page_index = next;
        cam.fit_page = true;
        self.requested = Instant::now();
        self.ready = None;
        ctx.request_repaint();
    }
}

fn main() -> eframe::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("Indique un PDF");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "RedBIM · Rendimiento",
        options,
        Box::new(move |cc| {
            let app = RedBimApp::new(
                cc,
                LaunchConfig {
                    files: vec![],
                    demo: false,
                    light_theme: false,
                    open_measurements_tab: false,
                    screenshot_out: None,
                },
            );
            Ok(Box::new(Walk {
                app,
                path,
                warm_frames: 0,
                start: None,
                requested: Instant::now(),
                ready: None,
                returning: false,
                done: false,
            }))
        }),
    )
}
