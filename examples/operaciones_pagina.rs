//! Medidor de página en la ventana nativa de RedBIM.
use eframe::egui;
use redbim::app::{LaunchConfig, RedBimApp};
use redbim::geometry::math::Rect2;
use redbim::model::document::DocumentTab;
use redbim::model::markup::{Markup, MarkupKind, MarkupStyle};
use redbim::pdf::io::open_pdf_file;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MARKS: usize = 16;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Rotate,
    Delete,
    Extract,
}

struct Bench {
    app: RedBimApp,
    operation: Operation,
    destination: PathBuf,
    input: Arc<Vec<u8>>,
    page: usize,
    pages: usize,
    initial_size: (f64, f64),
    ready: Option<Instant>,
    start: Option<Instant>,
    last_frame: Instant,
    feedback: Option<Instant>,
    idle_gap: Duration,
    operation_gap: Duration,
    dispatch: Duration,
    updates: usize,
    done: bool,
}

fn mark(tab: &mut DocumentTab) {
    for page in 0..tab.pages.len() {
        for index in 0..MARKS {
            tab.markups.push(Markup::new(
                page,
                format!("Medición de página {}", page + 1),
                "RedBIM",
                MarkupStyle::default(),
                MarkupKind::Rectangle {
                    rect: Rect2::from_xywh(
                        100. + (index % 4) as f64 * 150.,
                        150. + (index / 4) as f64 * 95.,
                        120.,
                        70.,
                    ),
                },
            ));
        }
    }
}

fn page_contents(doc: &lopdf::Document, index: usize) -> Vec<u8> {
    doc.get_page_content(doc.get_pages()[&(index as u32 + 1)])
}

impl Bench {
    fn verify(&self) -> (usize, usize) {
        let current = &self.app.tabs[self.app.active_tab];
        let source = lopdf::Document::load_mem(&self.input).unwrap();
        let output_page = match self.operation {
            Operation::Extract => {
                assert_eq!(current.pages.len(), self.pages);
                assert!(Arc::ptr_eq(&current.raw_pdf_bytes, &self.input));
                assert!(!current.dirty);
                0
            }
            Operation::Rotate | Operation::Delete => {
                assert!(current.dirty);
                self.page
            }
        };
        let extracted = matches!(self.operation, Operation::Extract)
            .then(|| open_pdf_file(&self.destination).unwrap());
        let result = extracted.as_ref().unwrap_or(current);
        let result_pdf = lopdf::Document::load_mem(&result.raw_pdf_bytes).unwrap();
        let source_page = if matches!(self.operation, Operation::Delete) {
            self.page + 1
        } else {
            self.page
        };
        let expected_pages = match self.operation {
            Operation::Rotate => self.pages,
            Operation::Delete => self.pages - 1,
            Operation::Extract => 1,
        };
        assert_eq!(result.pages.len(), expected_pages);
        let subject = format!("Medición de página {}", source_page + 1);
        assert_eq!(
            result
                .markups
                .iter()
                .filter(|m| m.page_index == output_page && m.subject == subject)
                .count(),
            MARKS
        );
        assert_eq!(
            page_contents(&source, source_page),
            page_contents(&result_pdf, output_page)
        );
        if matches!(self.operation, Operation::Rotate) {
            let rotated = &result.pages[self.page];
            assert!((rotated.width_pt - self.initial_size.1).abs() < 0.01);
            assert!((rotated.height_pt - self.initial_size.0).abs() < 0.01);
        }
        (expected_pages, result.raw_pdf_bytes.len())
    }
}

impl eframe::App for Bench {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.done {
            self.app.ui(ui, frame);
            return;
        }
        let now = Instant::now();
        let gap = now.duration_since(self.last_frame);
        self.last_frame = now;
        self.app.ui(ui, frame);
        let updated = Instant::now();
        let ctx = ui.ctx();
        ctx.request_repaint_after(Duration::from_millis(8));
        let tab = &self.app.tabs[0];
        let first_render = self.app.base_textures.contains_key(&(tab.id, self.page));

        if let Some(start) = self.start {
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "Operación sin resultado"
            );
            if self.feedback.is_none() {
                self.updates += 1;
                self.operation_gap = self.operation_gap.max(gap).max(updated - now);
                if let Some((message, finished)) = &self.app.toast {
                    let expected = match self.operation {
                        Operation::Rotate => "rotada",
                        Operation::Delete => "eliminada",
                        Operation::Extract => "extraída correctamente",
                    };
                    assert!(message.contains(expected), "{message}");
                    self.feedback = Some(*finished);
                }
            }
            let Some(finished) = self.feedback else {
                return;
            };
            if !first_render && !matches!(self.operation, Operation::Extract) {
                return;
            }
            let (pages, bytes) = self.verify();
            let ms = |d: Duration| d.as_secs_f64() * 1000.;
            let render_ms =
                (!matches!(self.operation, Operation::Extract)).then(|| ms(updated - start));
            println!(
                "{}",
                json!({
                    "operation": format!("{:?}", self.operation), "source_pages": self.pages,
                    "page": self.page + 1, "pages": pages, "bytes": bytes, "input_bytes": self.input.len(),
                    "total_ms": ms(finished - start), "render_ms": render_ms,
                    "dispatch_ms": ms(self.dispatch), "max_ui_gap_ms": ms(self.operation_gap),
                    "idle_gap_ms": ms(self.idle_gap), "frames": self.updates,
                })
            );
            self.done = true;
            self.app.force_close_allowed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if self.ready.is_none() {
            if first_render
                && self.app.thumb_textures.len() == self.app.requested_thumbs.len()
                && self.app.snap_indices.len() == self.app.requested_snaps.len()
                && tab.search_text_by_page.iter().any(|text| !text.is_empty())
            {
                self.ready = Some(updated);
            }
            return;
        }
        self.idle_gap = self.idle_gap.max(gap);
        if self.ready.unwrap().elapsed() < Duration::from_millis(400) {
            return;
        }
        self.app.toast = None;
        let start = Instant::now();
        self.start = Some(start);
        match self.operation {
            Operation::Rotate => self.app.rotate_document_page(self.page, 90, ctx),
            Operation::Delete => self.app.delete_document_page(self.page, ctx),
            Operation::Extract => {
                self.app
                    .extract_document_page(self.page, self.destination.clone(), ctx)
            }
        }
        self.dispatch = start.elapsed();
    }
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let operation = match args.next().as_deref().and_then(|s| s.to_str()) {
        Some("rotate") => Operation::Rotate,
        Some("delete") => Operation::Delete,
        Some("extract") => Operation::Extract,
        _ => panic!("Operación: rotate, delete o extract"),
    };
    let source = PathBuf::from(args.next().unwrap());
    let destination = PathBuf::from(args.next().unwrap()).join("resultado.pdf");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280., 800.]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "RedBIM · Operaciones de página",
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
            let mut tab = open_pdf_file(&source).unwrap();
            let pages = tab.pages.len();
            assert!(pages >= 10);
            let page = pages / 2;
            let initial_size = (tab.pages[page].width_pt, tab.pages[page].height_pt);
            let input = Arc::clone(&tab.raw_pdf_bytes);
            mark(&mut tab);
            tab.camera.page_index = page;
            app.add_and_activate_tab(tab, &cc.egui_ctx);
            Ok(Box::new(Bench {
                app,
                operation,
                destination,
                input,
                page,
                pages,
                initial_size,
                ready: None,
                start: None,
                last_frame: Instant::now(),
                feedback: None,
                idle_gap: Duration::ZERO,
                operation_gap: Duration::ZERO,
                dispatch: Duration::ZERO,
                updates: 0,
                done: false,
            }))
        }),
    )
}
