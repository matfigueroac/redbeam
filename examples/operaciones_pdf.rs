//! Medidor nativo, recibe una operación, el PDF A0, el PDF A1 y la carpeta temporal.
use eframe::egui;
use redbim::app::{LaunchConfig, RedBimApp};
use redbim::geometry::math::Rect2;
use redbim::model::document::DocumentTab;
use redbim::model::markup::{Markup, MarkupKind, MarkupStyle};
use redbim::pdf::flatten::unflatten_pdf_in_tab;
use redbim::pdf::io::open_pdf_file;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const MARKS: usize = 16;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Save,
    Export,
    Flatten,
    Combine,
}

struct Bench {
    app: RedBimApp,
    operation: Operation,
    paths: Vec<PathBuf>,
    output: PathBuf,
    ready: Option<Instant>,
    start: Option<Instant>,
    last_frame: Instant,
    idle_gap: Duration,
    operation_gap: Duration,
    dispatch: Duration,
    frames: usize,
    done: bool,
}

fn mark(tab: &mut DocumentTab) {
    for page in 0..tab.pages.len() {
        for index in 0..MARKS {
            tab.markups.push(Markup::new(
                page,
                "Rendimiento",
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
    tab.dirty = true;
}

impl Bench {
    fn destination(&self) -> PathBuf {
        self.output.join("resultado.pdf")
    }

    fn verify(&self) -> usize {
        let tab = &self.app.tabs[self.app.active_tab];
        match self.operation {
            Operation::Save => {
                let saved = open_pdf_file(&self.destination()).unwrap();
                assert_eq!(saved.markups, tab.markups);
                assert!(!tab.dirty);
                saved.pages.len()
            }
            Operation::Export => {
                let flattened = open_pdf_file(&self.destination()).unwrap();
                assert!(flattened.markups.is_empty());
                let doc = lopdf::Document::load_mem(&flattened.raw_pdf_bytes).unwrap();
                assert!(
                    doc.objects
                        .values()
                        .filter_map(|object| object.as_stream().ok())
                        .filter(|stream| stream.dict.has(b"RedBIMFlattenedOverlay"))
                        .count()
                        == flattened.pages.len()
                );
                flattened.pages.len()
            }
            Operation::Flatten => {
                assert!(tab.markups.is_empty() && tab.dirty);
                let mut copy = tab.clone();
                let count = unflatten_pdf_in_tab(&mut copy).unwrap();
                assert_eq!(count, tab.pages.len() * MARKS);
                tab.pages.len()
            }
            Operation::Combine => {
                assert_eq!(self.app.tabs.len(), 2);
                assert_eq!(tab.pages.len(), 41);
                assert!(tab.pages[0].width_pt > tab.pages[40].width_pt);
                tab.pages.len()
            }
        }
    }
}

impl eframe::App for Bench {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let gap = now.duration_since(self.last_frame);
        self.last_frame = now;
        self.app.ui(ui, frame);
        if self.done {
            return;
        }
        let ctx = ui.ctx();
        ctx.request_repaint_after(Duration::from_millis(8));

        if let Some(start) = self.start {
            assert!(
                start.elapsed() < Duration::from_secs(90),
                "Operación sin resultado"
            );
            self.frames += 1;
            self.operation_gap = self.operation_gap.max(gap);
            let Some((message, finished)) = &self.app.toast else {
                return;
            };
            assert!(
                message.contains(match self.operation {
                    Operation::Save => "guardado correctamente",
                    Operation::Export => "exportado correctamente",
                    Operation::Flatten => "aplanada(s)",
                    Operation::Combine => "combinado creado",
                }),
                "{message}"
            );
            let elapsed = finished.duration_since(start);
            let pages = self.verify();
            assert_eq!(
                pages,
                if matches!(self.operation, Operation::Combine) {
                    41
                } else {
                    27
                }
            );
            let ms = |d: Duration| d.as_secs_f64() * 1000.;
            println!(
                "{}",
                json!({"operation":format!("{:?}", self.operation), "pages":pages,
                "total_ms":ms(elapsed), "dispatch_ms":ms(self.dispatch), "max_ui_gap_ms":ms(self.operation_gap),
                "idle_gap_ms":ms(self.idle_gap), "frames": self.frames})
            );
            self.done = true;
            self.app.force_close_allowed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let tab = &self.app.tabs[0];
        if self.ready.is_none() {
            if self.app.base_textures.contains_key(&(tab.id, 0))
                && self.app.thumb_textures.len() == self.app.requested_thumbs.len()
                && self.app.snap_indices.len() == self.app.requested_snaps.len()
                && tab.search_text_by_page.iter().any(|text| !text.is_empty())
            {
                self.ready = Some(now);
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
            Operation::Save => self.app.save_document_to(self.destination(), false, ctx),
            Operation::Export => self.app.export_flattened_to(self.destination(), ctx),
            Operation::Flatten => self.app.flatten_document(true, ctx),
            Operation::Combine => self.app.combine_documents(self.paths.clone(), ctx),
        }
        self.dispatch = start.elapsed();
    }
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let operation = match args.next().as_deref().and_then(|s| s.to_str()) {
        Some("save") => Operation::Save,
        Some("export") => Operation::Export,
        Some("flatten") => Operation::Flatten,
        Some("combine") => Operation::Combine,
        _ => panic!("Operación: save, export, flatten o combine"),
    };
    let paths = vec![
        PathBuf::from(args.next().unwrap()),
        PathBuf::from(args.next().unwrap()),
    ];
    let output = PathBuf::from(args.next().unwrap());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280., 800.]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "RedBIM · Operaciones PDF",
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
            let mut tab = open_pdf_file(&paths[0]).unwrap();
            assert_eq!(tab.pages.len(), 27);
            assert!(tab.markups.is_empty());
            mark(&mut tab);
            app.add_and_activate_tab(tab, &cc.egui_ctx);
            Ok(Box::new(Bench {
                app,
                operation,
                paths,
                output,
                ready: None,
                start: None,
                last_frame: Instant::now(),
                idle_gap: Duration::ZERO,
                operation_gap: Duration::ZERO,
                dispatch: Duration::ZERO,
                frames: 0,
                done: false,
            }))
        }),
    )
}
