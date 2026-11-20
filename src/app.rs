use crate::geometry::hit_test::translate_markup;
use crate::geometry::math::Point2;
use crate::geometry::snap::SnapIndex;
use crate::model::document::{DocumentTab, SplitViewMode};
use crate::model::markup::{Markup, MarkupKind, MarkupStyle, MeasurementOptions};
use crate::model::settings::{AppSettings, ThemeMode};
use crate::model::tool_chest::{ActiveTool, ToolChest};
use crate::model::units::PT_PER_MM;
use crate::pdf::compare::compare_pdf_pages;
use crate::pdf::demo_blueprint::create_blank_pdf_bytes;
use crate::pdf::export::{
    export_markups_csv_file, export_markups_json_file, import_markups_json_file,
};
use crate::pdf::flatten::{flatten_pdf_file, flatten_pdf_to_bytes};
use crate::pdf::io::{
    combine_pdf_files, delete_page_in_doc, extract_pages_to_pdf, insert_blank_page_in_doc,
    open_demo_chile_document, open_pdf_file, open_pdf_from_bytes, rotate_page_in_doc,
    save_pdf_file, strip_annots_for_render,
};
use crate::pdf::renderer::{
    PdfRenderWorker, RenderCommand, RenderResult, default_interpreter_settings, render_tile_rgba,
};
use crate::ui::actions::UiAction;
use crate::ui::canvas::{
    CachedTile, CanvasResources, CanvasState, TileRequest, draw_document_workspace,
};
use crate::ui::dialogs::{ActiveModal, draw_active_modal};
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::left_panels::{LeftPanelTab, draw_left_panel};
use crate::ui::markups_list::{MarkupsListState, draw_markups_list_panel};
use crate::ui::measurements_panel::draw_measurements_panel;
use crate::ui::menu_bar::{PanelVisibility, draw_menu_bar};
use crate::ui::properties_panel::draw_properties_panel;
use crate::ui::search_panel::draw_search_panel;
use crate::ui::status_bar::draw_status_bar;
use crate::ui::theme::RedBimTheme;
use crate::ui::toolbars::{
    apply_tool_item_to_selection, draw_dynamic_properties_bar, draw_main_toolbar,
};
use eframe::egui::{self, Color32, ColorImage, CornerRadius, RichText, Stroke, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightPanelTab {
    #[default]
    Properties,
    Measurements,
    Search,
}

pub struct LaunchConfig {
    pub files: Vec<PathBuf>,
    pub demo: bool,
    pub light_theme: bool,
    pub open_measurements_tab: bool,
    pub screenshot_out: Option<PathBuf>,
}

pub struct RedBimApp {
    pub settings: AppSettings,
    pub theme: RedBimTheme,
    pub tool_chest: ToolChest,
    pub tabs: Vec<DocumentTab>,
    pub active_tab: usize,
    pub active_tool: ActiveTool,
    pub active_chest_item: Option<Uuid>,
    pub default_style: MarkupStyle,
    pub default_meas_opts: MeasurementOptions,

    pub panels: PanelVisibility,
    pub left_tab: LeftPanelTab,
    pub right_tab: RightPanelTab,
    pub canvas: CanvasState,
    pub markups_list_state: MarkupsListState,
    pub chest_search: String,
    pub search_query: String,
    pub new_layer_name: String,
    pub reply_draft: String,
    pub apply_scale_all_pages: bool,
    pub modal: Option<ActiveModal>,
    pub clipboard: Vec<Markup>,

    pub worker: PdfRenderWorker,
    pub base_textures: HashMap<(Uuid, usize), TextureHandle>,
    pub thumb_textures: HashMap<(Uuid, usize), TextureHandle>,
    pub tile_textures: HashMap<(Uuid, usize), CachedTile>,
    pub image_textures: HashMap<Uuid, TextureHandle>,
    pub failed_image_textures: HashSet<Uuid>,
    pub logo_texture: Option<TextureHandle>,
    pub snap_indices: HashMap<(Uuid, usize), SnapIndex>,
    pub requested_base: HashSet<(Uuid, usize)>,
    pub requested_thumbs: HashSet<(Uuid, usize)>,
    pub requested_snaps: HashSet<(Uuid, usize)>,
    pub generation_counter: u64,

    pub toast: Option<(String, Instant)>,
    pub screenshot_out: Option<PathBuf>,
    pub screenshot_frames_waited: u32,
    pub screenshot_requested: bool,
    pub screenshot_done: Arc<std::sync::atomic::AtomicBool>,
}

impl RedBimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: LaunchConfig) -> Self {
        let mut settings = AppSettings::load_or_default();
        if launch.light_theme {
            settings.theme = ThemeMode::Light;
        }
        let theme = RedBimTheme::from_mode(settings.theme);
        theme.apply(&cc.egui_ctx);

        let ctx_clone = cc.egui_ctx.clone();
        let worker = PdfRenderWorker::spawn(move || {
            ctx_clone.request_repaint();
        });

        let mut app = Self {
            settings,
            theme,
            tool_chest: ToolChest::load_or_default(),
            tabs: Vec::new(),
            active_tab: 0,
            active_tool: ActiveTool::Select,
            active_chest_item: None,
            default_style: MarkupStyle::default(),
            default_meas_opts: MeasurementOptions::default(),
            panels: PanelVisibility {
                left_open: true,
                right_open: true,
                bottom_open: true,
            },
            left_tab: LeftPanelTab::ToolChest,
            right_tab: if launch.open_measurements_tab {
                RightPanelTab::Measurements
            } else {
                RightPanelTab::Properties
            },
            canvas: CanvasState::default(),
            markups_list_state: MarkupsListState::default(),
            chest_search: String::new(),
            search_query: String::new(),
            new_layer_name: String::new(),
            reply_draft: String::new(),
            apply_scale_all_pages: false,
            modal: None,
            clipboard: Vec::new(),
            worker,
            base_textures: HashMap::new(),
            thumb_textures: HashMap::new(),
            tile_textures: HashMap::new(),
            image_textures: HashMap::new(),
            failed_image_textures: HashSet::new(),
            logo_texture: image::load_from_memory(crate::ui::icons::REDBIM_LOGO_PNG)
                .ok()
                .map(|img| {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let ci = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
                    cc.egui_ctx.load_texture("redbim_app_logo", ci, TextureOptions::LINEAR)
                }),
            snap_indices: HashMap::new(),
            requested_base: HashSet::new(),
            requested_thumbs: HashSet::new(),
            requested_snaps: HashSet::new(),
            generation_counter: 1,
            toast: None,
            screenshot_out: launch.screenshot_out,
            screenshot_frames_waited: 0,
            screenshot_requested: false,
            screenshot_done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        if launch.demo && let Ok(tab) = open_demo_chile_document() {
            app.add_and_activate_tab(tab);
        }
        for path in launch.files {
            if let Ok(tab) = open_pdf_file(&path) {
                app.settings.push_recent(path);
                app.add_and_activate_tab(tab);
            }
        }
        app
    }

    fn add_and_activate_tab(&mut self, tab: DocumentTab) {
        let doc_id = tab.id;
        let clean_bytes = Arc::new(strip_annots_for_render(&tab.raw_pdf_bytes));
        self.worker.send(RenderCommand::LoadDocument {
            doc_id,
            clean_pdf_bytes: clean_bytes,
        });
        let page_count = tab.pages.len();
        let raw_for_search = Arc::new(tab.raw_pdf_bytes.clone());
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        // Request initial renders for active page, background thumbnails, & async search text
        for p in 0..page_count.min(20) {
            self.ensure_thumb_requested(doc_id, p);
        }
        self.worker.send(RenderCommand::ExtractSearchText {
            doc_id,
            raw_pdf_bytes: raw_for_search,
        });
        self.ensure_page_requested(doc_id, 0);
    }

    fn reload_doc_render_cache(&mut self, doc_id: Uuid) {
        if let Some(tab) = self.tabs.iter().find(|t| t.id == doc_id) {
            let clean = Arc::new(strip_annots_for_render(&tab.raw_pdf_bytes));
            self.worker.send(RenderCommand::LoadDocument {
                doc_id,
                clean_pdf_bytes: clean,
            });
            self.worker.send(RenderCommand::ExtractSearchText {
                doc_id,
                raw_pdf_bytes: Arc::new(tab.raw_pdf_bytes.clone()),
            });
            self.base_textures.retain(|(id, _), _| *id != doc_id);
            self.thumb_textures.retain(|(id, _), _| *id != doc_id);
            self.tile_textures.retain(|(id, _), _| *id != doc_id);
            self.snap_indices.retain(|(id, _), _| *id != doc_id);
            self.requested_base.retain(|(id, _)| *id != doc_id);
            self.requested_thumbs.retain(|(id, _)| *id != doc_id);
            self.requested_snaps.retain(|(id, _)| *id != doc_id);
        }
    }

    fn ensure_thumb_requested(&mut self, doc_id: Uuid, page_index: usize) {
        if self.requested_thumbs.insert((doc_id, page_index)) {
            self.worker.send(RenderCommand::RenderThumbnail {
                doc_id,
                page_index,
                max_side_px: 220,
            });
        }
    }

    fn ensure_page_requested(&mut self, doc_id: Uuid, page_index: usize) {
        if self.requested_base.insert((doc_id, page_index)) {
            self.generation_counter += 1;
            self.worker.send(RenderCommand::RenderBasePage {
                doc_id,
                page_index,
                max_side_px: 2200,
                generation: self.generation_counter,
            });
        }
        if self.requested_snaps.insert((doc_id, page_index)) {
            self.worker
                .send(RenderCommand::ExtractSnap { doc_id, page_index });
        }
        self.ensure_thumb_requested(doc_id, page_index);
    }

    fn process_worker_results(&mut self, ctx: &egui::Context) {
        for res in self.worker.drain_results() {
            match res {
                RenderResult::ThumbnailReady {
                    doc_id,
                    page_index,
                    image,
                } => {
                    let ci = ColorImage::from_rgba_premultiplied(
                        [image.width, image.height],
                        &image.premul_rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("thumb_{doc_id}_{page_index}"),
                        ci,
                        TextureOptions::LINEAR,
                    );
                    self.thumb_textures.insert((doc_id, page_index), tex);
                }
                RenderResult::BasePageReady {
                    doc_id,
                    page_index,
                    image,
                    ..
                } => {
                    let ci = ColorImage::from_rgba_premultiplied(
                        [image.width, image.height],
                        &image.premul_rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("base_{doc_id}_{page_index}"),
                        ci,
                        TextureOptions::LINEAR,
                    );
                    self.base_textures.insert((doc_id, page_index), tex);
                }
                RenderResult::TileReady {
                    doc_id,
                    page_index,
                    crop_rect_pt,
                    image,
                    ..
                } => {
                    let ci = ColorImage::from_rgba_premultiplied(
                        [image.width, image.height],
                        &image.premul_rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("tile_{doc_id}_{page_index}"),
                        ci,
                        TextureOptions::LINEAR,
                    );
                    let scale_px_per_pt = image.width as f64 / crop_rect_pt.width().max(1.0);
                    self.tile_textures.insert(
                        (doc_id, page_index),
                        CachedTile {
                            crop_rect_pt,
                            texture: tex,
                            scale_px_per_pt,
                        },
                    );
                }
                RenderResult::SnapReady {
                    doc_id,
                    page_index,
                    index,
                } => {
                    self.snap_indices.insert((doc_id, page_index), index);
                }
                RenderResult::SearchTextReady {
                    doc_id,
                    text_by_page,
                } => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == doc_id) {
                        tab.search_text_by_page = text_by_page;
                    }
                }
                RenderResult::Error { message, .. } => {
                    self.show_toast(message);
                }
            }
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn ensure_image_textures(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let to_decode: Vec<(Uuid, String)> = tab
            .markups
            .iter()
            .filter_map(|m| {
                if let MarkupKind::Image { png_b64, .. } = &m.kind
                    && !png_b64.is_empty()
                    && !self.image_textures.contains_key(&m.id)
                    && !self.failed_image_textures.contains(&m.id)
                {
                    Some((m.id, png_b64.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (id, b64) in to_decode {
            let decoded = crate::pdf::base64_decode(&b64)
                .and_then(|raw| image::load_from_memory(&raw).ok())
                .map(|img| img.to_rgba8());
            if let Some(rgba) = decoded {
                let w = rgba.width() as usize;
                let h = rgba.height() as usize;
                let ci = ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
                let tex = ctx.load_texture(format!("img_{id}"), ci, TextureOptions::LINEAR);
                self.image_textures.insert(id, tex);
            } else {
                self.failed_image_textures.insert(id);
            }
        }
    }

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<UiAction>) {
        if ctx.egui_wants_keyboard_input() || self.modal.is_some() || self.canvas.inline_edit.is_some() {
            return;
        }
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;
            let shift = i.modifiers.shift;

            if ctrl && i.key_pressed(egui::Key::O) {
                actions.push(UiAction::OpenPdfDialog);
            } else if ctrl && i.key_pressed(egui::Key::S) {
                actions.push(UiAction::SaveDocument);
            } else if ctrl && i.key_pressed(egui::Key::Z) {
                actions.push(UiAction::Undo);
            } else if ctrl && i.key_pressed(egui::Key::Y) {
                actions.push(UiAction::Redo);
            } else if ctrl && i.key_pressed(egui::Key::C) {
                actions.push(UiAction::CopySelected);
            } else if ctrl && i.key_pressed(egui::Key::V) {
                actions.push(UiAction::PasteClipboard);
            } else if ctrl && i.key_pressed(egui::Key::D) {
                actions.push(UiAction::DuplicateSelected);
            } else if ctrl && i.key_pressed(egui::Key::G) {
                actions.push(UiAction::GroupSelected);
            } else if ctrl && i.key_pressed(egui::Key::A) {
                actions.push(UiAction::SelectAllOnPage);
            } else if ctrl && i.key_pressed(egui::Key::F) {
                self.panels.right_open = true;
                self.right_tab = RightPanelTab::Search;
            } else if ctrl && i.key_pressed(egui::Key::K) {
                actions.push(UiAction::OpenDialog(
                    crate::ui::actions::DialogRequest::CommandPalette,
                ));
            } else if ctrl && i.key_pressed(egui::Key::Num0) {
                actions.push(UiAction::FitPage);
            } else if ctrl && i.key_pressed(egui::Key::Num1) {
                actions.push(UiAction::Zoom100);
            } else if ctrl && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)) {
                actions.push(UiAction::ZoomStep(1.25));
            } else if ctrl && i.key_pressed(egui::Key::Minus) {
                actions.push(UiAction::ZoomStep(0.8));
            } else if i.key_pressed(egui::Key::PageDown) {
                if let Some(t) = self.tabs.get(self.active_tab) {
                    let p = t.active_page_index();
                    if p + 1 < t.pages.len() {
                        actions.push(UiAction::GoToPage(p + 1));
                    }
                }
            } else if i.key_pressed(egui::Key::PageUp) {
                if let Some(t) = self.tabs.get(self.active_tab) {
                    let p = t.active_page_index();
                    if p > 0 {
                        actions.push(UiAction::GoToPage(p - 1));
                    }
                }
            } else if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                actions.push(UiAction::DeleteSelected);
            } else if !ctrl && !shift {
                let single_map = [
                    (egui::Key::V, ActiveTool::Select),
                    (egui::Key::Z, ActiveTool::ZoomWindow),
                    (egui::Key::N, ActiveTool::Note),
                    (egui::Key::T, ActiveTool::TextBox),
                    (egui::Key::W, ActiveTool::Typewriter),
                    (egui::Key::Q, ActiveTool::Callout),
                    (egui::Key::C, ActiveTool::Cloud),
                    (egui::Key::K, ActiveTool::CloudPlus),
                    (egui::Key::A, ActiveTool::Arrow),
                    (egui::Key::L, ActiveTool::Line),
                    (egui::Key::R, ActiveTool::Rectangle),
                    (egui::Key::E, ActiveTool::Ellipse),
                    (egui::Key::P, ActiveTool::Polygon),
                    (egui::Key::H, ActiveTool::Highlight),
                    (egui::Key::S, ActiveTool::Stamp),
                    (egui::Key::G, ActiveTool::Snapshot),
                ];
                for (k, t) in single_map {
                    if i.key_pressed(k) {
                        actions.push(UiAction::SetTool(t));
                    }
                }
                let num_keys = [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
                    egui::Key::Num9,
                ];
                for (idx, k) in num_keys.into_iter().enumerate() {
                    if i.key_pressed(k)
                        && let Some(my) = self.tool_chest.sets.iter().find(|s| s.is_my_tools)
                        && let Some(item) = my.items.get(idx)
                    {
                        actions.push(UiAction::ActivateToolChestItem(item.clone()));
                    }
                }
            } else if !ctrl && shift {
                let shift_map = [
                    (egui::Key::N, ActiveTool::Polyline),
                    (egui::Key::C, ActiveTool::Arc),
                    (egui::Key::L, ActiveTool::MeasureLength),
                    (egui::Key::P, ActiveTool::MeasurePerimeter),
                    (egui::Key::A, ActiveTool::MeasureArea),
                    (egui::Key::V, ActiveTool::MeasureVolume),
                    (egui::Key::G, ActiveTool::MeasureAngle),
                    (egui::Key::U, ActiveTool::MeasureRadius),
                    (egui::Key::O, ActiveTool::MeasureCount),
                ];
                for (k, t) in shift_map {
                    if i.key_pressed(k) {
                        actions.push(UiAction::SetTool(t));
                    }
                }
            }
        });
    }

    fn execute_actions(&mut self, ctx: &egui::Context, actions: Vec<UiAction>) {
        for action in actions {
            match action {
                UiAction::NewBlankDocument => {
                    // Default Chilean ISO A1 landscape (841 x 594 mm)
                    let bytes = create_blank_pdf_bytes(841.0 * PT_PER_MM, 594.0 * PT_PER_MM, 1);
                    if let Ok(tab) = open_pdf_from_bytes("Nuevo_Plano_A1.pdf", None, bytes) {
                        self.add_and_activate_tab(tab);
                    }
                }
                UiAction::OpenPdfDialog => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Documentos PDF", &["pdf"])
                        .pick_file()
                    {
                        match open_pdf_file(&path) {
                            Ok(tab) => {
                                self.settings.push_recent(path);
                                self.add_and_activate_tab(tab);
                            }
                            Err(err) => self.show_toast(err),
                        }
                    }
                }
                UiAction::OpenPdfPath(path) => match open_pdf_file(&path) {
                    Ok(tab) => {
                        self.settings.push_recent(path);
                        self.add_and_activate_tab(tab);
                    }
                    Err(err) => self.show_toast(err),
                },
                UiAction::OpenDemoChile => match open_demo_chile_document() {
                    Ok(tab) => self.add_and_activate_tab(tab),
                    Err(err) => self.show_toast(err),
                },
                UiAction::SaveDocument => {
                    let target = self
                        .tabs
                        .get(self.active_tab)
                        .and_then(|t| t.file_path.clone())
                        .or_else(|| {
                            rfd::FileDialog::new()
                                .add_filter("PDF", &["pdf"])
                                .set_file_name("Plano_RedBIM.pdf")
                                .save_file()
                        });
                    if let Some(path) = target
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        match save_pdf_file(tab, &path) {
                            Ok(new_bytes) => {
                                tab.raw_pdf_bytes = new_bytes;
                                tab.file_path = Some(path.clone());
                                tab.title = path
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Documento.pdf")
                                    .to_string();
                                tab.dirty = false;
                                self.settings.push_recent(path);
                                self.show_toast("PDF guardado correctamente");
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::SaveDocumentAs => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name("Plano_RedBIM.pdf")
                        .save_file()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        match save_pdf_file(tab, &path) {
                            Ok(new_bytes) => {
                                tab.raw_pdf_bytes = new_bytes;
                                tab.file_path = Some(path.clone());
                                tab.title = path
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Documento.pdf")
                                    .to_string();
                                tab.dirty = false;
                                self.settings.push_recent(path);
                                self.show_toast("PDF guardado");
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ExportFlattenedPdf => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf"])
                            .set_file_name("Plano_Aplanado_RedBIM.pdf")
                            .save_file()
                    {
                        match flatten_pdf_file(tab, &path) {
                            Ok(()) => self.show_toast("PDF aplanado exportado correctamente"),
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ExportCsv(delim) => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("CSV Excel", &["csv"])
                            .set_file_name("Cubicación_RedBIM.csv")
                            .save_file()
                    {
                        match export_markups_csv_file(tab, &path, delim) {
                            Ok(()) => self.show_toast("CSV exportado correctamente"),
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ExportMarkupsJson => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Marcas RedBIM", &["json"])
                            .set_file_name("marcas.redbim.json")
                            .save_file()
                    {
                        match export_markups_json_file(tab, &path) {
                            Ok(()) => self.show_toast("Marcas exportadas"),
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ImportMarkupsJson => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Marcas RedBIM", &["json"])
                            .pick_file()
                    {
                        match import_markups_json_file(tab, &path) {
                            Ok(n) => self.show_toast(format!("{n} marcas importadas")),
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::CombinePdfs => {
                    if let Some(paths) = rfd::FileDialog::new()
                        .add_filter("Documentos PDF", &["pdf"])
                        .pick_files()
                    {
                        if paths.len() < 2 {
                            self.show_toast("Seleccione al menos 2 archivos PDF para combinar");
                        } else {
                            match combine_pdf_files(&paths).and_then(|bytes| {
                                open_pdf_from_bytes("Combinado_RedBIM.pdf", None, bytes)
                            }) {
                                Ok(new_tab) => {
                                    let total_p = new_tab.pages.len();
                                    self.add_and_activate_tab(new_tab);
                                    self.show_toast(format!(
                                        "PDF combinado creado ({total_p} páginas)"
                                    ));
                                }
                                Err(e) => self.show_toast(e),
                            }
                        }
                    }
                }
                UiAction::CompareWithPdf => {
                    let source = self
                        .tabs
                        .get(self.active_tab)
                        .map(|t| (t.raw_pdf_bytes.clone(), t.active_page_index()));
                    if let Some((bytes_a, page_a)) = source
                        && let Some(path_b) = rfd::FileDialog::new()
                            .add_filter("Documento PDF revisión B", &["pdf"])
                            .pick_file()
                    {
                        match std::fs::read(&path_b)
                            .map_err(|e| format!("No se pudo leer PDF B: {e}"))
                            .and_then(|bytes_b| {
                                let page_b = lopdf::Document::load_mem(&bytes_b)
                                    .map(|d| page_a.min(d.get_pages().len().saturating_sub(1)))
                                    .unwrap_or(0);
                                compare_pdf_pages(&bytes_a, page_a, &bytes_b, page_b, 1800)
                            }) {
                            Ok((diff_tab, report)) => {
                                self.add_and_activate_tab(diff_tab);
                                self.show_toast(format!(
                                    "Comparación completada: {} región(es) con cambios",
                                    report.diff_regions_count
                                ));
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ExtractCurrentPage => {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let pg = tab.active_page_index();
                        let default_name = format!("Pagina_{}_RedBIM.pdf", pg + 1);
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf"])
                            .set_file_name(&default_name)
                            .save_file()
                        {
                            match extract_pages_to_pdf(tab, &[pg])
                                .and_then(|bytes| {
                                    std::fs::write(&path, bytes)
                                        .map_err(|e| format!("Error guardando extracción: {e}"))
                                }) {
                                Ok(()) => self.show_toast(format!(
                                    "Página {} extraída correctamente",
                                    pg + 1
                                )),
                                Err(e) => self.show_toast(e),
                            }
                        }
                    }
                }
                UiAction::CloseTab(idx) => {
                    if idx < self.tabs.len() {
                        let doc_id = self.tabs[idx].id;
                        let markup_ids: Vec<Uuid> =
                            self.tabs[idx].markups.iter().map(|m| m.id).collect();
                        for mid in markup_ids {
                            self.image_textures.remove(&mid);
                            self.failed_image_textures.remove(&mid);
                        }
                        self.worker.send(RenderCommand::CloseDocument { doc_id });
                        self.tabs.remove(idx);
                        self.base_textures.retain(|(id, _), _| *id != doc_id);
                        self.thumb_textures.retain(|(id, _), _| *id != doc_id);
                        self.tile_textures.retain(|(id, _), _| *id != doc_id);
                        self.snap_indices.retain(|(id, _), _| *id != doc_id);
                        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                            self.active_tab = self.tabs.len() - 1;
                        }
                    }
                }
                UiAction::SelectTab(idx) => {
                    if idx < self.tabs.len() {
                        self.active_tab = idx;
                        self.canvas.creation = None;
                        self.canvas.inline_edit = None;
                    }
                }
                UiAction::Undo => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.undo();
                    }
                }
                UiAction::Redo => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.redo();
                    }
                }
                UiAction::CopySelected => {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        self.clipboard = tab.selected_markups().into_iter().cloned().collect();
                    }
                }
                UiAction::PasteClipboard => {
                    if !self.clipboard.is_empty()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        let pg = tab.active_page_index();
                        tab.push_undo();
                        let mut new_ids = Vec::new();
                        for orig in &self.clipboard {
                            let mut c = orig.clone();
                            c.id = Uuid::new_v4();
                            c.page_index = pg;
                            translate_markup(&mut c, Point2::new(16.0, 16.0));
                            new_ids.push(c.id);
                            tab.markups.push(c);
                        }
                        tab.selected_ids = new_ids;
                    }
                }
                UiAction::DuplicateSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let sel: Vec<Markup> = tab.selected_markups().into_iter().cloned().collect();
                        if !sel.is_empty() {
                            tab.push_undo();
                            let mut new_ids = Vec::new();
                            for orig in sel {
                                let mut c = orig;
                                c.id = Uuid::new_v4();
                                translate_markup(&mut c, Point2::new(18.0, 18.0));
                                new_ids.push(c.id);
                                tab.markups.push(c);
                            }
                            tab.selected_ids = new_ids;
                        }
                    }
                }
                UiAction::DeleteSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.delete_selected();
                    }
                }
                UiAction::SelectAllOnPage => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let pg = tab.active_page_index();
                        tab.selected_ids = tab
                            .markups
                            .iter()
                            .filter(|m| m.page_index == pg && tab.is_layer_visible(&m.layer))
                            .map(|m| m.id)
                            .collect();
                    }
                }
                UiAction::GroupSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.group_selected();
                    }
                }
                UiAction::UngroupSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.ungroup_selected();
                    }
                }
                UiAction::AlignSelected(kind) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && tab.align_selected(kind)
                    {
                        self.show_toast("Marcas alineadas");
                    }
                }
                UiAction::BringToFront => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.bring_selected_to_front();
                    }
                }
                UiAction::SendToBack => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.send_selected_to_back();
                    }
                }
                UiAction::SetTool(tool) => {
                    self.active_tool = tool;
                    self.active_chest_item = None;
                    self.canvas.creation = None;
                    if tool.is_measurement() || tool == ActiveTool::Calibrate {
                        self.panels.right_open = true;
                        self.right_tab = RightPanelTab::Measurements;
                    }
                }
                UiAction::ActivateToolChestItem(item) => {
                    if item.properties_mode {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            apply_tool_item_to_selection(tab, &item.style);
                        }
                    } else {
                        self.active_tool = item.tool;
                        self.active_chest_item = Some(item.id);
                        self.default_style = item.style.clone();
                        if let Some(opts) = &item.measurement_opts {
                            self.default_meas_opts = opts.clone();
                        }
                        self.canvas.creation = None;
                        self.tool_chest.push_recent(item);
                    }
                }
                UiAction::AddSelectedToToolChest => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(first) = tab.selected_markups().first()
                    {
                        let item = crate::model::tool_chest::ToolChestItem::from_markup(first);
                        self.tool_chest.add_to_my_tools(item);
                        self.show_toast("Agregado a Mis herramientas");
                    }
                }
                UiAction::SetCurrentStyleAsDefault => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(first) = tab.selected_markups().first()
                    {
                        self.default_style = first.style.clone();
                        if let Some(o) = first.measurement_opts() {
                            self.default_meas_opts = o.clone();
                        }
                        self.show_toast("Estilo guardado como predeterminado");
                    }
                }
                UiAction::MarkupsStyleChanged => {}
                UiAction::FitPage => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let (vw, vh) = self.canvas.last_viewport_size;
                        if vw > 30.0
                            && vh > 30.0
                            && let Some(p) = tab.active_page()
                        {
                            let margin = 28.0;
                            let zx = (vw - margin * 2.0) / p.width_pt.max(1.0);
                            let zy = (vh - margin * 2.0) / p.height_pt.max(1.0);
                            let z = zx.min(zy).clamp(0.05, 8.0);
                            let pad_x = ((vw - p.width_pt * z) * 0.5) / z;
                            let pad_y = ((vh - p.height_pt * z) * 0.5) / z;
                            let is_sec = tab.active_pane_secondary;
                            tab.active_camera_mut().fit_pending = false;
                            self.canvas.glide_to(is_sec, z, Point2::new(-pad_x, -pad_y));
                        } else {
                            tab.active_camera_mut().fit_pending = true;
                        }
                    }
                }
                UiAction::FitWidth => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let vw = self.canvas.last_viewport_size.0.max(100.0);
                        if let Some(p) = tab.active_page() {
                            let z = ((vw - 40.0) / p.width_pt.max(1.0)).clamp(0.05, 8.0);
                            let is_sec = tab.active_pane_secondary;
                            tab.active_camera_mut().fit_pending = false;
                            self.canvas
                                .glide_to(is_sec, z, Point2::new(-20.0 / z, -12.0 / z));
                        }
                    }
                }
                UiAction::Zoom100 => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let (vw, vh) = self.canvas.last_viewport_size;
                        let is_sec = tab.active_pane_secondary;
                        let cam = tab.active_camera_mut();
                        let center = Point2::new(
                            cam.pan.x + (vw * 0.5) / cam.zoom.max(0.05),
                            cam.pan.y + (vh * 0.5) / cam.zoom.max(0.05),
                        );
                        cam.fit_pending = false;
                        let pan = Point2::new(center.x - vw * 0.5, center.y - vh * 0.5);
                        self.canvas.glide_to(is_sec, 1.0, pan);
                    }
                }
                UiAction::ZoomStep(factor) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let (vw, vh) = self.canvas.last_viewport_size;
                        let is_sec = tab.active_pane_secondary;
                        let cam = tab.active_camera_mut();
                        let center = Point2::new(
                            cam.pan.x + (vw * 0.5) / cam.zoom.max(0.05),
                            cam.pan.y + (vh * 0.5) / cam.zoom.max(0.05),
                        );
                        let nz = (cam.zoom * factor).clamp(0.05, 16.0);
                        cam.fit_pending = false;
                        let pan = Point2::new(
                            center.x - (vw * 0.5) / nz,
                            center.y - (vh * 0.5) / nz,
                        );
                        self.canvas.glide_to(is_sec, nz, pan);
                    }
                }
                UiAction::GoToPage(idx) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && idx < tab.pages.len()
                    {
                        let cam = tab.active_camera_mut();
                        cam.page_index = idx;
                        cam.fit_pending = true;
                        let doc_id = tab.id;
                        self.ensure_page_requested(doc_id, idx);
                    }
                }
                UiAction::JumpToMarkup(id) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && let Some(m) = tab.markups.iter().find(|x| x.id == id).cloned()
                    {
                        let b = crate::geometry::hit_test::markup_bounds(&m);
                        tab.select_only(id);
                        tab.jump_to_rect(m.page_index, b, self.canvas.last_viewport_size);
                        let doc_id = tab.id;
                        self.ensure_page_requested(doc_id, m.page_index);
                        self.panels.right_open = true;
                        self.right_tab = RightPanelTab::Properties;
                    }
                }
                UiAction::JumpToPageText(page_idx) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && page_idx < tab.pages.len()
                    {
                        let cam = tab.active_camera_mut();
                        cam.page_index = page_idx;
                        cam.fit_pending = true;
                        let doc_id = tab.id;
                        self.ensure_page_requested(doc_id, page_idx);
                    }
                }
                UiAction::ToggleSplitVertical => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.split_mode = if tab.split_mode == SplitViewMode::Vertical {
                            SplitViewMode::None
                        } else {
                            tab.secondary_camera = tab.camera.clone();
                            SplitViewMode::Vertical
                        };
                    }
                }
                UiAction::ToggleSplitHorizontal => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.split_mode = if tab.split_mode == SplitViewMode::Horizontal {
                            SplitViewMode::None
                        } else {
                            tab.secondary_camera = tab.camera.clone();
                            SplitViewMode::Horizontal
                        };
                    }
                }
                UiAction::SetSplitNone => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.split_mode = SplitViewMode::None;
                    }
                }
                UiAction::RotatePage(pg, delta) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        if let Err(e) = rotate_page_in_doc(tab, pg, delta) {
                            self.show_toast(e);
                        } else {
                            tab.active_camera_mut().fit_pending = true;
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                        }
                    }
                }
                UiAction::InsertBlankPage(at) => {
                    let inserted = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let (w, h) = tab
                            .active_page()
                            .map(|p| (p.width_pt, p.height_pt))
                            .unwrap_or((841.0 * PT_PER_MM, 594.0 * PT_PER_MM));
                        match insert_blank_page_in_doc(tab, at, w, h) {
                            Ok(()) => {
                                let idx = at.min(tab.pages.len().saturating_sub(1));
                                tab.active_camera_mut().page_index = idx;
                                tab.active_camera_mut().fit_pending = true;
                                Ok((doc_id, idx))
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        continue;
                    };
                    match inserted {
                        Ok((doc_id, idx)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, idx);
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::DeletePage(pg) => {
                    let deleted = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        match delete_page_in_doc(tab, pg) {
                            Ok(()) => Ok((doc_id, tab.active_page_index())),
                            Err(e) => Err(e),
                        }
                    } else {
                        continue;
                    };
                    match deleted {
                        Ok((doc_id, cur)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, cur);
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::ApplyPageScale { scale, apply_all } => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        if apply_all {
                            for p in &mut tab.pages {
                                p.scale = scale.clone();
                            }
                        } else {
                            let idx = tab.active_page_index();
                            if let Some(p) = tab.pages.get_mut(idx) {
                                p.scale = scale.clone();
                            }
                        }
                        self.show_toast(format!("Escala aplicada: {}", scale.label()));
                    }
                }
                UiAction::ApplyScaleToRange { scale, pages } => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        let mut applied = 0usize;
                        for idx in pages {
                            if let Some(p) = tab.pages.get_mut(idx) {
                                p.scale = scale.clone();
                                applied += 1;
                            }
                        }
                        self.show_toast(format!(
                            "Escala {} aplicada a {applied} página(s)",
                            scale.label()
                        ));
                    }
                }
                UiAction::SaveViewportEdit { id, name, scale } => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        for p in &mut tab.pages {
                            if let Some(vp) = p.viewports.iter_mut().find(|v| v.id == id) {
                                vp.name = name.clone();
                                vp.scale = scale.clone();
                                break;
                            }
                        }
                        self.show_toast("Viewport actualizado");
                    }
                }
                UiAction::CaptureSnapshot(rect_pt) => {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        let pg = tab.active_page_index();
                        let render_bytes = flatten_pdf_to_bytes(tab)
                            .unwrap_or_else(|_| strip_annots_for_render(&tab.raw_pdf_bytes));
                        let max_dim = rect_pt.width().max(rect_pt.height()).max(1.0);
                        let scale = 2.5_f64.min(1800.0 / max_dim);
                        let out_w = ((rect_pt.width() * scale).round() as u32).clamp(32, 1800) as u16;
                        let out_h =
                            ((rect_pt.height() * scale).round() as u32).clamp(32, 1800) as u16;
                        match render_tile_rgba(
                            &render_bytes,
                            pg,
                            rect_pt,
                            out_w,
                            out_h,
                            &default_interpreter_settings(),
                            &hayro::RenderSettings::default(),
                        ) {
                            Ok(img) => {
                                let mut straight = Vec::with_capacity(img.premul_rgba.len());
                                for px in img.premul_rgba.as_chunks::<4>().0 {
                                    let a = px[3];
                                    if a == 0 {
                                        straight.extend_from_slice(&[0, 0, 0, 0]);
                                    } else if a == 255 {
                                        straight.extend_from_slice(px);
                                    } else {
                                        let af = a as u32;
                                        straight.push(((px[0] as u32 * 255) / af).min(255) as u8);
                                        straight.push(((px[1] as u32 * 255) / af).min(255) as u8);
                                        straight.push(((px[2] as u32 * 255) / af).min(255) as u8);
                                        straight.push(a);
                                    }
                                }
                                ctx.copy_image(ColorImage::from_rgba_unmultiplied(
                                    [img.width, img.height],
                                    &straight,
                                ));
                                if let Some(rgba_img) = image::RgbaImage::from_raw(
                                    img.width as u32,
                                    img.height as u32,
                                    straight,
                                ) {
                                    let mut png_bytes = Vec::new();
                                    let mut cursor = std::io::Cursor::new(&mut png_bytes);
                                    if image::DynamicImage::ImageRgba8(rgba_img)
                                        .write_to(&mut cursor, image::ImageFormat::Png)
                                        .is_ok()
                                    {
                                        let snap_m = Markup::new(
                                            pg,
                                            "Instantánea",
                                            &self.settings.author_name,
                                            self.default_style.clone(),
                                            MarkupKind::Image {
                                                rect: rect_pt,
                                                png_b64: crate::pdf::base64_encode(&png_bytes),
                                                caption: "Instantánea".to_string(),
                                            },
                                        );
                                        self.clipboard = vec![snap_m];
                                    }
                                }
                                self.show_toast(
                                    "Instantánea copiada al portapapeles (Ctrl+V para pegar)",
                                );
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::StartTwoPointCalibration => {
                    self.active_tool = ActiveTool::Calibrate;
                    self.canvas.creation = None;
                    self.show_toast("Haga clic en 2 puntos de una cota conocida del plano");
                }
                UiAction::OpenDialog(req) => {
                    let tab_ref = self.tabs.get(self.active_tab);
                    self.modal = Some(ActiveModal::from_request(req, tab_ref));
                }
                UiAction::SetTheme(mode) => {
                    self.settings.theme = mode;
                    self.settings.save();
                    self.theme = RedBimTheme::from_mode(mode);
                    self.theme.apply(ctx);
                }
                UiAction::InsertLegendOnCurrentPage(pos) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let pg = tab.active_page_index();
                        tab.push_undo();
                        let m = Markup::new(
                            pg,
                            "Leyenda de cubicación",
                            &self.settings.author_name,
                            MarkupStyle {
                                stroke_color: [37, 99, 235, 255],
                                ..Default::default()
                            },
                            MarkupKind::Legend {
                                pos,
                                title: "Resumen de Mediciones (Chile)".to_string(),
                                width_pt: 280.0,
                            },
                        );
                        let id = m.id;
                        tab.markups.push(m);
                        tab.select_only(id);
                    }
                }
                UiAction::CreateCutoutForMarkup(id) => {
                    self.canvas.active_cutout_target = Some(id);
                    self.active_tool = ActiveTool::CutoutPoly;
                    self.canvas.creation = None;
                    self.show_toast("Dibuje el polígono de descuento (doble clic para cerrar)");
                }
                UiAction::DeleteViewport(vp_id) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        let idx = tab.active_page_index();
                        if let Some(p) = tab.pages.get_mut(idx) {
                            p.viewports.retain(|v| v.id != vp_id);
                        }
                    }
                }
                UiAction::ChangeSelectedStatus(st) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let sel = tab.selected_ids.clone();
                        tab.push_undo();
                        for m in &mut tab.markups {
                            if sel.contains(&m.id) {
                                m.status = st;
                                m.modified_at = chrono::Utc::now();
                            }
                        }
                    }
                }
                UiAction::StartInlineEdit(id) => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(m) = tab.markups.iter().find(|x| x.id == id)
                    {
                        let txt = m
                            .text_content()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| m.comment.clone());
                        self.canvas.inline_edit = Some((id, txt, true));
                    }
                }
                UiAction::RequestRedrawTextures => {}
                UiAction::ShowToast(msg) => self.show_toast(msg),
                UiAction::ExitApp => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn draw_welcome_screen(&self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        ui.centered_and_justified(|ui| {
            egui::Frame::new()
                .fill(self.theme.panel_bg)
                .stroke(Stroke::new(1.0, self.theme.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(28))
                .show(ui, |ui| {
                    ui.set_max_width(480.0);
                    ui.vertical_centered(|ui| {
                        if let Some(logo) = &self.logo_texture {
                            ui.add(egui::Image::new((logo.id(), egui::vec2(72.0, 72.0))));
                            ui.add_space(6.0);
                        }
                        ui.label(
                            RichText::new("RedBIM")
                                .strong()
                                .size(32.0)
                                .color(self.theme.accent_red),
                        );
                        ui.label(
                            RichText::new("Marcado, revisión técnica y cubicación de planos PDF · Enfoque Métrico Chile (m / cm / mm) + Imperial EE.UU.")
                                .size(12.5)
                                .color(self.theme.text_secondary),
                        );
                        ui.add_space(18.0);

                        let open_btn = egui::Button::new(
                            RichText::new("Abrir archivo PDF...  (Ctrl+O)")
                                .strong()
                                .size(14.0)
                                .color(Color32::WHITE),
                        )
                        .fill(self.theme.accent_red)
                        .min_size(egui::vec2(320.0, 38.0));
                        if ui.add(open_btn).clicked() {
                            actions.push(UiAction::OpenPdfDialog);
                        }

                        ui.add_space(8.0);
                        let demo_btn = egui::Button::new(
                            RichText::new("Abrir plano de ejemplo (Chile 1:100 · Cubierta FV)")
                                .strong()
                                .size(13.0),
                        )
                        .min_size(egui::vec2(320.0, 34.0));
                        if ui.add(demo_btn).clicked() {
                            actions.push(UiAction::OpenDemoChile);
                        }

                        ui.add_space(8.0);
                        let blank_btn = egui::Button::new(
                            RichText::new("Nuevo plano en blanco (ISO A1 · 841 × 594 mm)")
                                .size(12.5),
                        )
                        .min_size(egui::vec2(320.0, 30.0));
                        if ui.add(blank_btn).clicked() {
                            actions.push(UiAction::NewBlankDocument);
                        }

                        if !self.settings.recent_files.is_empty() {
                            ui.add_space(16.0);
                            ui.separator();
                            ui.label(RichText::new("Archivos recientes").strong().size(12.0));
                            for p in self.settings.recent_files.iter().take(5) {
                                let name = p
                                    .file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Archivo.pdf");
                                if ui.button(name).clicked() {
                                    actions.push(UiAction::OpenPdfPath(p.clone()));
                                }
                            }
                        }
                    });
                });
        });
    }
}

impl eframe::App for RedBimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_worker_results(&ctx);
        self.ensure_image_textures(&ctx);

        // Handle dropped PDFs
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| {
                    let p = f.path();
                    (!p.as_os_str().is_empty()).then(|| p.to_path_buf())
                })
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .collect()
        });
        for p in dropped {
            if let Ok(tab) = open_pdf_file(&p) {
                self.settings.push_recent(p);
                self.add_and_activate_tab(tab);
            }
        }

        let mut actions = Vec::new();
        self.handle_global_shortcuts(&ctx, &mut actions);

        // Ensure active page & thumbnails are requested
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let doc_id = tab.id;
            let p0 = tab.camera.page_index;
            let p1 = tab.secondary_camera.page_index;
            let split = tab.split_mode;
            let page_count = tab.pages.len();
            self.ensure_page_requested(doc_id, p0);
            if split != SplitViewMode::None {
                self.ensure_page_requested(doc_id, p1);
            }
            // Prefetch neighbour pages once the current one is ready → instant page flips
            if self.base_textures.contains_key(&(doc_id, p0)) {
                if p0 + 1 < page_count {
                    self.ensure_page_requested(doc_id, p0 + 1);
                }
                if p0 > 0 {
                    self.ensure_page_requested(doc_id, p0 - 1);
                }
            }
        }

        // Top Menu Bar
        egui::Panel::top("redbim_menu_bar")
            .frame(egui::Frame::new().fill(self.theme.window_bg).inner_margin(egui::Margin::symmetric(8, 3)))
            .show(ui, |ui| {
                let tab_ref = self.tabs.get(self.active_tab);
                draw_menu_bar(
                    ui,
                    tab_ref,
                    &mut self.settings,
                    &mut self.panels,
                    self.logo_texture.as_ref().map(|t| t.id()),
                    &self.theme,
                    &mut actions,
                );
            });

        // Main Toolbar
        egui::Panel::top("redbim_main_toolbar")
            .frame(egui::Frame::new().fill(self.theme.toolbar_bg).inner_margin(egui::Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, self.theme.border)))
            .show(ui, |ui| {
                let tab_ref = self.tabs.get(self.active_tab);
                draw_main_toolbar(ui, tab_ref, self.active_tool, &self.theme, &mut actions);
            });

        // Dynamic Properties Toolbar
        egui::Panel::top("redbim_dpt_toolbar")
            .frame(egui::Frame::new().fill(self.theme.panel_bg).inner_margin(egui::Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, self.theme.border)))
            .show(ui, |ui| {
                let tab_mut = self.tabs.get_mut(self.active_tab);
                draw_dynamic_properties_bar(
                    ui,
                    tab_mut,
                    self.active_tool,
                    &mut self.default_style,
                    &mut self.default_meas_opts,
                    &self.theme,
                    &mut actions,
                );
            });

        // Sleek Modern Document Tabs Strip
        if !self.tabs.is_empty() {
            egui::Panel::top("redbim_doc_tabs")
                .frame(
                    egui::Frame::new()
                        .fill(self.theme.window_bg)
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .stroke(Stroke::new(1.0, self.theme.border)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let tab_meta: Vec<(String, bool, usize)> = self
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(i, t)| (t.title.clone(), t.dirty, i))
                            .collect();
                        for (title, dirty, idx) in tab_meta {
                            let is_act = idx == self.active_tab;
                            let bg = if is_act {
                                self.theme.panel_bg
                            } else {
                                self.theme.control_bg.gamma_multiply(0.55)
                            };
                            let frame_resp = egui::Frame::new()
                                .fill(bg)
                                .stroke(Stroke::new(
                                    1.0,
                                    if is_act {
                                        self.theme.border_strong
                                    } else {
                                        self.theme.border.gamma_multiply(0.5)
                                    },
                                ))
                                .corner_radius(CornerRadius::same(5))
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        if dirty {
                                            ui.label(
                                                RichText::new("●")
                                                    .size(9.0)
                                                    .color(self.theme.accent_amber),
                                            );
                                        }
                                        let title_resp = ui.add(
                                            egui::Label::new(
                                                RichText::new(&title)
                                                    .size(11.8)
                                                    .strong()
                                                    .color(if is_act {
                                                        self.theme.text_primary
                                                    } else {
                                                        self.theme.text_secondary
                                                    }),
                                            )
                                            .sense(egui::Sense::click()),
                                        );
                                        if title_resp.clicked() {
                                            actions.push(UiAction::SelectTab(idx));
                                        }
                                        let close_btn = ui.add(
                                            egui::Button::new(
                                                RichText::new("×")
                                                    .size(12.0)
                                                    .color(self.theme.text_muted),
                                            )
                                            .frame(false),
                                        );
                                        if close_btn.clicked() {
                                            actions.push(UiAction::CloseTab(idx));
                                        }
                                    });
                                });
                            if is_act {
                                let r = frame_resp.response.rect;
                                let bar = egui::Rect::from_min_max(
                                    egui::pos2(r.left() + 6.0, r.bottom() - 2.2),
                                    egui::pos2(r.right() - 6.0, r.bottom()),
                                );
                                ui.painter().rect_filled(
                                    bar,
                                    CornerRadius::same(1),
                                    self.theme.accent_red,
                                );
                            }
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("+").strong().size(13.0))
                                    .min_size(egui::vec2(24.0, 22.0)),
                            )
                            .on_hover_text("Abrir otro PDF")
                            .clicked()
                        {
                            actions.push(UiAction::OpenPdfDialog);
                        }
                    });
                });
        }

        // Bottom Status Bar (outermost bottom!)
        egui::Panel::bottom("redbim_status_bar")
            .frame(egui::Frame::new().fill(self.theme.window_bg).inner_margin(egui::Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, self.theme.border)))
            .show(ui, |ui| {
                let tab_ref = self.tabs.get(self.active_tab);
                draw_status_bar(
                    ui,
                    tab_ref,
                    self.active_tool,
                    &mut self.settings,
                    &mut self.panels.bottom_open,
                    &self.theme,
                    &mut actions,
                );
            });

        // Left Panel (Tool Chest, Thumbnails, Layers, Bookmarks)
        if self.panels.left_open {
            egui::Panel::left("redbim_left_panel")
                .resizable(true)
                .default_size(250.0)
                .size_range(190.0..=420.0)
                .frame(egui::Frame::new().fill(self.theme.panel_bg).inner_margin(egui::Margin::same(8)).stroke(Stroke::new(1.0, self.theme.border)))
                .show(ui, |ui| {
                    let tab_mut = self.tabs.get_mut(self.active_tab);
                    draw_left_panel(
                        ui,
                        &mut self.left_tab,
                        tab_mut,
                        &mut self.tool_chest,
                        &mut self.chest_search,
                        self.active_chest_item,
                        &mut self.new_layer_name,
                        &self.thumb_textures,
                        &mut self.settings,
                        &self.theme,
                        &mut actions,
                    );
                });
        }

        // Right Panel (Properties, Measurements & Scale, Search)
        if self.panels.right_open {
            egui::Panel::right("redbim_right_panel")
                .resizable(true)
                .default_size(285.0)
                .size_range(220.0..=450.0)
                .frame(egui::Frame::new().fill(self.theme.panel_bg).inner_margin(egui::Margin::same(8)).stroke(Stroke::new(1.0, self.theme.border)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if icon_button(
                            ui,
                            28.0,
                            IconKind::Properties,
                            self.right_tab == RightPanelTab::Properties,
                            true,
                            "Propiedades de marca",
                            &self.theme,
                        )
                        .clicked()
                        {
                            self.right_tab = RightPanelTab::Properties;
                        }
                        if icon_button(
                            ui,
                            28.0,
                            IconKind::Measurements,
                            self.right_tab == RightPanelTab::Measurements,
                            true,
                            "Escala y Mediciones (Chile / EE.UU.)",
                            &self.theme,
                        )
                        .clicked()
                        {
                            self.right_tab = RightPanelTab::Measurements;
                        }
                        if icon_button(
                            ui,
                            28.0,
                            IconKind::Search,
                            self.right_tab == RightPanelTab::Search,
                            true,
                            "Buscar en documento y marcas",
                            &self.theme,
                        )
                        .clicked()
                        {
                            self.right_tab = RightPanelTab::Search;
                        }
                    });
                    ui.separator();

                    match self.right_tab {
                        RightPanelTab::Properties => {
                            let tab_mut = self.tabs.get_mut(self.active_tab);
                            draw_properties_panel(
                                ui,
                                tab_mut,
                                &self.settings.author_name,
                                &mut self.reply_draft,
                                &self.theme,
                                &mut actions,
                            );
                        }
                        RightPanelTab::Measurements => {
                            let tab_mut = self.tabs.get_mut(self.active_tab);
                            draw_measurements_panel(
                                ui,
                                tab_mut,
                                self.active_tool,
                                &mut self.apply_scale_all_pages,
                                &self.theme,
                                &mut actions,
                            );
                        }
                        RightPanelTab::Search => {
                            let tab_ref = self.tabs.get(self.active_tab);
                            draw_search_panel(
                                ui,
                                tab_ref,
                                &mut self.search_query,
                                &self.theme,
                                &mut actions,
                            );
                        }
                    }
                });
        }

        // Bottom Markups List Panel
        if self.panels.bottom_open && !self.tabs.is_empty() {
            egui::Panel::bottom("redbim_markups_list")
                .resizable(true)
                .default_size(175.0)
                .size_range(100.0..=360.0)
                .frame(egui::Frame::new().fill(self.theme.panel_bg).inner_margin(egui::Margin::same(8)).stroke(Stroke::new(1.0, self.theme.border)))
                .show(ui, |ui| {
                    let tab_mut = self.tabs.get_mut(self.active_tab);
                    draw_markups_list_panel(
                        ui,
                        tab_mut,
                        &mut self.markups_list_state,
                        &self.theme,
                        &mut actions,
                    );
                });
        }

        // Central Workspace (Welcome screen or PDF Canvas)
        let mut tile_requests: Vec<TileRequest> = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(self.theme.canvas_bg))
            .show(ui, |ui| {
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    draw_document_workspace(
                        ui,
                        tab,
                        &mut self.canvas,
                        &mut self.active_tool,
                        &self.default_style,
                        &self.default_meas_opts,
                        &self.settings,
                        CanvasResources {
                            base_textures: &self.base_textures,
                            tile_textures: &self.tile_textures,
                            thumb_textures: &self.thumb_textures,
                            image_textures: &self.image_textures,
                            snap_indices: &self.snap_indices,
                        },
                        &self.theme,
                        &mut actions,
                        &mut tile_requests,
                    );
                } else {
                    self.draw_welcome_screen(ui, &mut actions);
                }
            });

        // Submit any high-res zoom tile renders needed
        for req in tile_requests {
            self.generation_counter += 1;
            self.worker.send(RenderCommand::RenderTile {
                doc_id: req.doc_id,
                page_index: req.page_index,
                crop_rect_pt: req.crop_rect_pt,
                out_w: req.out_w,
                out_h: req.out_h,
                generation: self.generation_counter,
            });
        }

        // Active modal dialog
        {
            let tab_mut = self.tabs.get_mut(self.active_tab);
            draw_active_modal(
                &ctx,
                &mut self.modal,
                tab_mut,
                &mut self.settings,
                &self.theme,
                &mut actions,
            );
        }

        // Animated Floating Toast Banner (smooth slide-up + fade-in/out)
        if let Some((msg, t0)) = &self.toast {
            let elapsed = t0.elapsed().as_secs_f32();
            let total = 3.8_f32;
            if elapsed < total {
                let fade_in = (elapsed / 0.18).clamp(0.0, 1.0);
                let fade_out = ((total - elapsed) / 0.35).clamp(0.0, 1.0);
                let ease = |t: f32| 1.0 - (1.0 - t).powi(3);
                let alpha = (ease(fade_in) * fade_out).clamp(0.0, 1.0);
                let slide_y = -44.0 + (1.0 - ease(fade_in)) * 12.0 + (1.0 - fade_out) * 8.0;

                if elapsed < 0.22 || elapsed > total - 0.38 {
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(Duration::from_millis(200));
                }

                egui::Area::new(egui::Id::unique("redbim_toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, slide_y])
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(
                                Color32::from_rgba_unmultiplied(15, 23, 42, (238.0 * alpha) as u8),
                            )
                            .stroke(Stroke::new(
                                1.0,
                                self.theme.accent_cyan.gamma_multiply(alpha * 0.85),
                            ))
                            .corner_radius(CornerRadius::same(7))
                            .shadow(egui::Shadow {
                                offset: [0, 4],
                                blur: 14,
                                spread: 0,
                                color: Color32::from_black_alpha((110.0 * alpha) as u8),
                            })
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("●")
                                            .size(10.0)
                                            .color(self.theme.accent_red.gamma_multiply(alpha)),
                                    );
                                    ui.label(
                                        RichText::new(msg)
                                            .color(Color32::WHITE.gamma_multiply(alpha))
                                            .strong(),
                                    );
                                });
                            });
                    });
            } else {
                self.toast = None;
            }
        }

        self.execute_actions(&ctx, actions);

        // Optional automated screenshot capture for visual verification (`--screenshot path.png`)
        if let Some(ref out_path) = self.screenshot_out {
            let ready = self.tabs.first().is_none_or(|t| {
                self.base_textures.contains_key(&(t.id, t.active_page_index()))
            });
            if ready {
                self.screenshot_frames_waited += 1;
            }
            if ready && self.screenshot_frames_waited >= 4 && !self.screenshot_requested {
                self.screenshot_requested = true;
                let p = out_path.clone();
                let done_flag = self.screenshot_done.clone();
                let ctx_close = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let w = img.width() as u32;
                    let h = img.height() as u32;
                    let raw = img.as_raw().to_vec();
                    if let Some(buf) = image::RgbaImage::from_raw(w, h, raw) {
                        let _ = buf.save(&p);
                    }
                    done_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    ctx_close.send_viewport_cmd(egui::ViewportCommand::Close);
                });
                ctx.request_repaint();
            } else if self.screenshot_done.load(std::sync::atomic::Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(60));
            }
        }
    }
}
