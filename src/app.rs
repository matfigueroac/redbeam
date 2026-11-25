use crate::geometry::hit_test::translate_markup;
use crate::geometry::math::{Point2, Rect2};
use crate::geometry::snap::{SnapIndex, snap_markup_points_to_index};
use crate::model::document::{DocumentTab, SplitViewMode};
use crate::model::markup::{
    Markup, MarkupAttachment, MarkupKind, MarkupStyle, MeasurementOptions,
};
use crate::model::settings::{AppSettings, ThemeMode};
use crate::model::tool_chest::{ActiveTool, ToolChest, ToolSet};
use crate::model::units::PT_PER_MM;
use crate::pdf::batch::run_batch_job;
use crate::pdf::color_process::apply_color_process_in_tab;
use crate::pdf::compare::compare_pdf_pages;
use crate::pdf::demo_blueprint::create_blank_pdf_bytes;
use crate::pdf::export::{
    export_markups_csv_file, export_markups_json_file, import_markups_json_file,
};
use crate::pdf::flatten::{
    flatten_in_place_in_tab, flatten_pdf_file, flatten_pdf_to_bytes, unflatten_pdf_in_tab,
};
use crate::pdf::headers_footers::{apply_headers_footers_in_tab, remove_headers_footers_in_tab};
use crate::pdf::io::{
    RotatedPdfPage, apply_deleted_page, apply_rotated_page, apply_text_page_scales,
    combine_pdf_files, delete_pdf_page, extract_pages_to_pdf, insert_blank_page_in_doc,
    open_demo_chile_document, open_pdf_file, open_pdf_from_bytes, replace_page_content_in_doc,
    rotate_pdf_page, save_pdf_file, strip_annots_for_render,
};
use crate::pdf::optimize::reduce_pdf_file_size_in_tab;
use crate::pdf::overlay::{OverlayConfig, overlay_pdf_pages};
use crate::pdf::redact::apply_redactions_in_tab;
use crate::pdf::visual_search::run_visual_search_on_page;
use crate::pdf::renderer::{
    PdfRenderWorker, RenderCommand, RenderResult, default_interpreter_settings, render_tile_rgba,
};
use crate::ui::actions::UiAction;
use crate::ui::canvas::{
    CachedTile, CanvasResources, CanvasState, InlineTextEdit, TileRequest, draw_document_workspace,
    fit_callout_text,
};
use crate::ui::dialogs::{ActiveModal, draw_active_modal};
use crate::ui::document_tabs::draw_document_tabs;
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::left_panels::{LeftPanelTab, draw_left_panel, draw_left_rail};
use crate::ui::markups_list::{MarkupsListState, draw_markups_list_panel};
use crate::ui::measurements_panel::draw_measurements_panel;
use crate::ui::menu_bar::{PanelVisibility, draw_menu_bar};
use crate::ui::notes_panel::draw_notes_panel;
use crate::ui::properties_panel::draw_properties_panel;
use crate::ui::search_panel::draw_search_panel;
use crate::ui::status_bar::draw_status_bar;
use crate::ui::theme::RedBimTheme;
use crate::updates::UpdateCheck;
use crate::ui::toolbars::{
    apply_tool_item_to_selection, draw_main_toolbar,
};
use eframe::egui::{self, Color32, ColorImage, CornerRadius, RichText, Stroke, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

const SIDEBAR_RAIL_WIDTH: f32 = 38.0;
const SIDEBAR_GRIP_WIDTH: f32 = 8.0;
const LEFT_PANEL_MIN_WIDTH: f32 = 172.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 230.0;
const CANVAS_MIN_WIDTH: f32 = 240.0;

#[derive(Clone, Copy)]
enum Sidebar {
    Left,
    Right,
}

fn draw_right_sidebar(
    ui: &mut egui::Ui,
    width: f32,
    theme: &RedBimTheme,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    egui::Panel::right("redbim_right_panel")
        .resizable(false)
        .exact_size(width)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(theme.panel_bg)
                .inner_margin(egui::Margin::same(8))
                .stroke(Stroke::new(1.0, theme.border)),
        )
        .show(ui, |ui| {
            // Wide contents must not shift the fixed edge of the right panel.
            let bounds = ui.available_rect_before_wrap();
            let mut content = ui.new_child(egui::UiBuilder::new().max_rect(bounds));
            content.shrink_clip_rect(bounds);
            contents(&mut content);
        })
        .response
        .rect
}

fn locate_text_markup(tab: &mut DocumentTab, id: Uuid, viewport: (f64, f64)) -> Option<usize> {
    let markup = tab.markups.iter().find(|markup| markup.id == id)?;
    let page = markup.page_index;
    tab.pages.get(page)?;
    let bounds = crate::geometry::hit_test::markup_bounds(markup);
    let hidden_layer = tab
        .layers
        .iter()
        .position(|layer| layer.name == markup.layer && !layer.visible);
    if let Some(index) = hidden_layer {
        tab.push_undo();
        tab.layers[index].visible = true;
    }

    let camera = tab.active_camera();
    if camera.fit_page {
        tab.active_camera_mut().page_index = page;
    } else {
        let zoom = camera.zoom.max(0.05);
        let visible = crate::geometry::math::Rect2::from_xywh(
            camera.pan.x, camera.pan.y, viewport.0 / zoom, viewport.1 / zoom,
        );
        if camera.page_index != page || !visible.contains(bounds.center()) {
            tab.jump_to_rect(page, bounds, viewport);
        }
    }
    tab.select_only(id);
    Some(page)
}

fn draw_sidebar_grip(
    ui: &mut egui::Ui,
    side: Sidebar,
    panel_rect: egui::Rect,
    width: &mut f32,
    limits: egui::Rangef,
    theme: &RedBimTheme,
) {
    let (grip, fixed_edge, direction, label) = match side {
        Sidebar::Left => (
            egui::Panel::left("left_sidebar_grip"),
            panel_rect.left(),
            1.0,
            "Ajustar ancho del panel izquierdo",
        ),
        Sidebar::Right => (
            egui::Panel::right("right_sidebar_grip"),
            panel_rect.right(),
            -1.0,
            "Ajustar ancho del panel derecho",
        ),
    };
    grip.resizable(false)
        .exact_size(SIDEBAR_GRIP_WIDTH)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(theme.canvas_bg))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let response = ui
                .interact(rect, ui.scope_id().with("drag"), egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Splitter, true, label));

            let offset_id = response.id.with("offset");
            if response.drag_started() {
                let offset = ui.input(|i| i.pointer.press_origin())
                    .map_or(0.0, |press| press.x - rect.center().x);
                ui.data_mut(|data| data.insert_temp(offset_id, offset));
            }
            if response.dragged() || response.drag_stopped() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                    let offset = ui.data(|data| data.get_temp::<f32>(offset_id)).unwrap_or(0.0);
                    let requested = direction * (pointer.x - offset - fixed_edge)
                        - SIDEBAR_GRIP_WIDTH * 0.5;
                    let resized = requested.clamp(limits.min, limits.max);
                    if resized != *width {
                        *width = resized;
                        ui.ctx().request_repaint();
                    }
                }
            }

            let color = if response.dragged() {
                theme.accent_red
            } else if response.hovered() {
                theme.border_strong
            } else {
                theme.border
            };
            ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, color));
        });
}

#[cfg(test)]
mod sidebar_tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput, pos2, vec2};

    #[test]
    fn image_textures_follow_edits_panes_and_photo_previews() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = RedBimApp::new(&cc, LaunchConfig {
            files: vec![], demo: false, light_theme: false,
            open_measurements_tab: false, screenshot_out: None,
        });
        let mut tab = open_demo_chile_document().unwrap();
        let bytes = crate::ui::icons::REDBIM_LOGO_PNG;
        let mut mark = Markup::new(1, "Foto", "Test", MarkupStyle::default(), MarkupKind::Image {
            rect: Rect2::from_xywh(1., 1., 10., 10.),
            png_b64: crate::pdf::base64_encode(bytes).into(), caption: String::new(),
        });
        let photo = MarkupAttachment::from_image_bytes("Foto", bytes).unwrap();
        let mid = mark.id;
        let aid = photo.id;
        mark.attachments.push(photo);
        tab.markups.push(mark);
        app.tabs.push(tab);
        app.ensure_image_textures(&ctx);
        assert!(!app.image_textures.contains_key(&mid));
        app.tabs[0].split_mode = SplitViewMode::Vertical;
        app.tabs[0].secondary_camera.page_index = 1;
        app.tabs[0].select_only(mid);
        app.ensure_image_textures(&ctx);
        assert!(app.image_textures.contains_key(&mid) && app.image_textures.contains_key(&aid));
        app.tabs[0].push_undo();
        if let MarkupKind::Image { png_b64, .. } = &mut app.tabs[0].markups.last_mut().unwrap().kind {
            *png_b64 = "invalid".into();
        }
        app.ensure_image_textures(&ctx);
        assert!(!app.image_textures.contains_key(&mid) && app.failed_image_textures.contains(&mid));
        assert!(app.tabs[0].undo());
        app.ensure_image_textures(&ctx);
        assert!(app.image_textures.contains_key(&mid) && !app.failed_image_textures.contains(&mid));
        app.tabs[0].selected_ids.clear();
        app.modal = Some(ActiveModal::PhotoPreview { attachment_id: aid, caption: String::new() });
        app.ensure_image_textures(&ctx);
        assert!(app.image_textures.contains_key(&aid));
        app.modal = None;
        app.ensure_image_textures(&ctx);
        assert!(!app.image_textures.contains_key(&aid));
        app.close_tab_immediate(0);
        app.ensure_image_textures(&ctx);
        assert!(app.image_sources.is_empty() && app.image_textures.is_empty());
    }

    #[test]
    fn render_cache_preserves_neighbors_and_the_second_pane() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = RedBimApp::new(&cc, LaunchConfig {
            files: vec![], demo: false, light_theme: false,
            open_measurements_tab: false, screenshot_out: None,
        });
        let mut tab = open_demo_chile_document().unwrap();
        let doc = tab.id;
        let other = Uuid::new_v4();
        tab.camera.page_index = 0;
        tab.secondary_camera.page_index = 2;
        tab.split_mode = SplitViewMode::Vertical;
        app.tabs.push(tab);
        let keys = [(doc, 0), (doc, 1), (doc, 2), (other, 0)];
        app.requested_base.extend(keys);
        app.requested_snaps.extend(keys);
        let texture = ctx.load_texture("cache-test", ColorImage::example(), TextureOptions::LINEAR);
        app.base_textures.extend(keys.map(|key| (key, texture.clone())));
        app.update_visible_pages();
        assert_eq!(app.base_textures.len(), 3);
        let visible = app.visible_pages.unwrap();
        assert!(visible.keep_tile(doc, 2, true));
        assert!(!visible.keep_tile(doc, 2, false));
        app.tabs[0].split_mode = SplitViewMode::None;
        app.update_visible_pages();
        assert_eq!(app.base_textures.len(), 2);
        assert!(!app.requested_base.contains(&(doc, 2)));
        assert!(!app.requested_snaps.contains(&(doc, 2)));
        app.tabs[0].camera.page_index = 2;
        app.update_visible_pages();
        app.ensure_page_requested(doc, 2);
        assert!(app.requested_base.contains(&(doc, 2)));
        assert!(!app.base_textures.contains_key(&(doc, 0)));
    }

    #[test]
    fn opening_switching_and_closing_tabs_preserves_pending_note_edits() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = RedBimApp::new(&cc, LaunchConfig {
            files: Vec::new(), demo: false, light_theme: false,
            open_measurements_tab: false, screenshot_out: None,
        });
        app.execute_actions(&ctx, vec![UiAction::NewBlankDocument]);
        let note = Markup::new(0, "Nota", "Autor", MarkupStyle::default(),
            MarkupKind::Note { pos: Point2::new(50.0, 50.0) });
        let id = note.id;
        app.tabs[0].markups.push(note);
        app.canvas.inline_edit = Some(InlineTextEdit::new(id, "Primera revisión".into()));
        app.execute_actions(&ctx, vec![UiAction::SelectTab(0)]);
        assert!(app.canvas.inline_edit.is_some());
        app.execute_actions(&ctx, vec![UiAction::NewBlankDocument]);
        assert_eq!(app.tabs[0].markups[0].comment, "Primera revisión");
        assert!(app.canvas.inline_edit.is_none());
        app.execute_actions(&ctx, vec![UiAction::SelectTab(0)]);
        app.canvas.inline_edit = Some(InlineTextEdit::new(id, "Segunda revisión".into()));
        app.execute_actions(&ctx, vec![UiAction::SelectTab(1)]);
        assert_eq!(app.tabs[0].markups[0].comment, "Segunda revisión");
        app.execute_actions(&ctx, vec![UiAction::SelectTab(0)]);
        app.tabs[0].dirty = false;
        app.canvas.inline_edit = Some(InlineTextEdit::new(id, "Revisión final".into()));
        app.execute_actions(&ctx, vec![UiAction::CloseTab(0)]);
        assert!(app.modal.is_some());
        assert!(app.tabs[0].dirty);
        assert_eq!(app.tabs[0].markups[0].comment, "Revisión final");
    }

    #[test]
    fn failed_background_save_preserves_changes_and_can_be_retried() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = RedBimApp::new(
            &cc,
            LaunchConfig {
                files: vec![],
                demo: false,
                light_theme: false,
                open_measurements_tab: false,
                screenshot_out: None,
            },
        );
        app.execute_actions(&ctx, vec![UiAction::NewBlankDocument]);
        app.tabs[0].dirty = true;
        let original = Arc::clone(&app.tabs[0].raw_pdf_bytes);
        let dir = tempfile::tempdir().unwrap();

        let finish_save = |app: &mut RedBimApp| {
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            while app.pdf_job.is_some() {
                assert!(Instant::now() < deadline, "PDF job did not finish");
                app.poll_pdf_job(&ctx);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };

        app.save_document_to(dir.path().to_path_buf(), false, &ctx);
        assert!(app.pdf_job.is_some());
        app.execute_actions(&ctx, vec![UiAction::ForceCloseTab(0)]);
        assert_eq!(app.tabs.len(), 1);
        finish_save(&mut app);
        assert!(app.toast.as_ref().unwrap().0.starts_with("No se pudo guardar"));
        assert!(app.tabs[0].dirty);
        assert!(app.tabs[0].file_path.is_none());
        assert!(Arc::ptr_eq(&original, &app.tabs[0].raw_pdf_bytes));

        let target = dir.path().join("plano.pdf");
        app.save_document_to(target.clone(), false, &ctx);
        finish_save(&mut app);
        assert_eq!(app.tabs[0].file_path.as_ref(), Some(&target));
        assert!(!app.tabs[0].dirty);
        assert!(lopdf::Document::load(target).is_ok());
    }

    #[test]
    fn locating_text_opens_its_page_and_keeps_its_view_when_already_visible() {
        let page = crate::model::document::PageModel::new;
        let mut tab = DocumentTab::new_with_pages(
            "Prueba", None, Vec::new(), vec![page(0, 600.0, 400.0), page(1, 600.0, 400.0)],
        );
        let mut note = Markup::new(
            1, "Nota", "", MarkupStyle::default(),
            MarkupKind::Note { pos: Point2::new(100.0, 100.0) },
        );
        note.layer = "Revisión".into();
        tab.ensure_layer(&note.layer);
        let layer = tab.layers.iter_mut().find(|layer| layer.name == note.layer).unwrap();
        layer.visible = false;
        let id = note.id;
        tab.markups.push(note);
        assert_eq!(locate_text_markup(&mut tab, id, (400.0, 300.0)), Some(1));
        assert_eq!(tab.camera.page_index, 1);
        assert!(tab.camera.fit_page);
        assert!(tab.is_layer_visible("Revisión"));
        assert_eq!(tab.selected_ids, [id]);

        tab.camera.fit_page = false;
        tab.camera.zoom = 1.0;
        tab.camera.pan = Point2::new(10.0, 30.0);
        let visible_camera = tab.camera.clone();
        locate_text_markup(&mut tab, id, (400.0, 300.0));
        assert_eq!(tab.camera, visible_camera);
        tab.camera.pan = Point2::new(300.0, 200.0);
        locate_text_markup(&mut tab, id, (400.0, 300.0));
        assert_eq!(tab.camera.pan, Point2::new(-40.0, -40.0));
        assert_eq!(tab.camera.zoom, 1.0);
    }

    #[test]
    fn note_selection_keeps_the_canvas_rect() {
        let ctx = egui::Context::default();
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        theme.apply(&ctx);
        let mut tab = DocumentTab::new_with_pages(
            "Prueba",
            None,
            Vec::new(),
            vec![crate::model::document::PageModel::new(0, 600.0, 400.0)],
        );
        let mut note = Markup::new(
            0, "Nota de revisión", "Autor", MarkupStyle::default(),
            MarkupKind::Note { pos: Point2::new(100.0, 100.0) },
        );
        note.comment = "Revisar fijaciones".into();
        let id = note.id;
        tab.markups.push(note);
        let mut canvas_rect = egui::Rect::NOTHING;
        let mut render = |selected| {
            tab.selected_ids = if selected { vec![id] } else { Vec::new() };
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1300.0, 800.0))),
                    ..Default::default()
                },
                |ui| {
                    draw_right_sidebar(ui, RIGHT_PANEL_MIN_WIDTH, &theme, |ui| {
                        draw_properties_panel(
                            ui, Some(&mut tab), "Autor", &mut String::new(), &HashMap::new(),
                            &theme, &mut Vec::new(),
                        );
                    });
                    canvas_rect = ui.available_rect_before_wrap();
                },
            );
            output.textures_delta.clear();
            canvas_rect
        };
        assert_eq!(render(false), render(true));
    }

    #[test]
    fn sidebar_width_survives_release_and_reopen() {
        for side in [Sidebar::Left, Sidebar::Right] {
            let ctx = egui::Context::default();
            let theme = RedBimTheme::from_mode(ThemeMode::Dark);
            let mut width = 250.0;
            let mut grip_center = 0.0;
            let mut frame = |events, open: bool| {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            pos2(0.0, 0.0),
                            vec2(960.0, 620.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if !open {
                            return;
                        }
                        let panel = match side {
                            Sidebar::Left => egui::Panel::left("test_panel"),
                            Sidebar::Right => egui::Panel::right("test_panel"),
                        };
                        let panel = panel
                            .resizable(false)
                            .show_separator_line(false)
                            .exact_size(width)
                            .show(ui, |_| ());
                        let rect = panel.response.rect;
                        grip_center = match side {
                            Sidebar::Left => rect.right() + SIDEBAR_GRIP_WIDTH * 0.5,
                            Sidebar::Right => rect.left() - SIDEBAR_GRIP_WIDTH * 0.5,
                        };
                        draw_sidebar_grip(
                            ui,
                            side,
                            rect,
                            &mut width,
                            egui::Rangef::new(172.0, 380.0),
                            &theme,
                        );
                    },
                );
                output.textures_delta.clear();
                (width, grip_center)
            };
            let (_, x) = frame(vec![], true);
            let direction = match side {
                Sidebar::Left => 1.0,
                Sidebar::Right => -1.0,
            };
            let start = pos2(x, 300.0);
            let during = pos2(x + direction * 47.0, 300.0);
            let end = pos2(x + direction * 70.0, 300.0);
            let pointer_button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(vec![Event::PointerMoved(start)], true);
            frame(vec![pointer_button(start, true)], true);
            assert_eq!(frame(vec![Event::PointerMoved(during)], true).0, 297.0);
            assert_eq!(
                frame(vec![Event::PointerMoved(end), pointer_button(end, false)], true).0,
                320.0
            );
            frame(vec![], false);
            assert_eq!(frame(vec![], true), (320.0, end.x));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightPanelTab {
    #[default]
    Properties,
    Measurements,
    Search,
    Notes,
}

pub struct LaunchConfig {
    pub files: Vec<PathBuf>,
    pub demo: bool,
    pub light_theme: bool,
    pub open_measurements_tab: bool,
    pub screenshot_out: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct VisiblePages {
    document: Uuid,
    primary: usize,
    secondary: Option<usize>,
}

impl VisiblePages {
    fn keep_base(self, document: Uuid, page: usize) -> bool {
        self.document == document && (self.primary.abs_diff(page) <= 1 || self.secondary == Some(page))
    }

    fn keep_tile(self, document: Uuid, page: usize, secondary: bool) -> bool {
        let visible = if secondary { self.secondary } else { Some(self.primary) };
        self.document == document && visible == Some(page)
    }
}

struct CachedImageSource {
    source: Arc<str>,
    visible: bool,
}

struct PdfJob {
    label: &'static str,
    receive: std::sync::mpsc::Receiver<Result<PdfJobResult, String>>,
}

enum PdfJobResult {
    Saved {
        doc_id: Uuid,
        path: PathBuf,
        bytes: Vec<u8>,
        remember_destination: bool,
    },
    Exported,
    Flattened {
        doc_id: Uuid,
        bytes: Arc<Vec<u8>>,
        remaining: Vec<Markup>,
        count: usize,
        recoverable: bool,
    },
    PageRotated {
        doc_id: Uuid,
        page_index: usize,
        rotated: RotatedPdfPage,
    },
    PageDeleted {
        doc_id: Uuid,
        page_index: usize,
        bytes: Arc<Vec<u8>>,
    },
    PageExtracted {
        page_index: usize,
    },
    Combined(Box<DocumentTab>),
}

pub struct RedBimApp {
    pdf_job: Option<PdfJob>,
    visible_pages: Option<VisiblePages>,
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
    left_panel_width: f32,
    right_panel_width: f32,
    pub canvas: CanvasState,
    pub markups_list_state: MarkupsListState,
    pub chest_search: String,
    pub search_query: String,
    pub new_layer_name: String,
    pub reply_draft: String,
    pub apply_scale_all_pages: bool,
    pub modal: Option<ActiveModal>,
    pub clipboard: Vec<Markup>,
    pub paste_count: usize,
    pub force_close_allowed: bool,
    pub last_window_title: String,

    pub worker: PdfRenderWorker,
    pub base_textures: HashMap<(Uuid, usize), TextureHandle>,
    pub thumb_textures: HashMap<(Uuid, usize), TextureHandle>,
    pub tile_textures: HashMap<(Uuid, usize, bool), CachedTile>,
    pub in_flight_tiles: HashMap<(Uuid, usize, bool), (Rect2, u16, u16)>,
    image_sources: HashMap<Uuid, CachedImageSource>,
    pub image_textures: HashMap<Uuid, TextureHandle>,
    pub failed_image_textures: HashSet<Uuid>,
    pub logo_texture: Option<TextureHandle>,
    pub snap_indices: HashMap<(Uuid, usize), SnapIndex>,
    pub requested_base: HashSet<(Uuid, usize)>,
    pub requested_thumbs: HashSet<(Uuid, usize)>,
    pub requested_snaps: HashSet<(Uuid, usize)>,
    pub generation_counter: u64,

    pub toast: Option<(String, Instant)>,
    updates: UpdateCheck,
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

        let panels = PanelVisibility {
            left_open: false,
            right_open: false,
            markups_open: false,
        };

        let mut app = Self {
            left_panel_width: settings.panel_left_width,
            right_panel_width: settings.panel_right_width,
            settings,
            theme,
            tool_chest: ToolChest::load_or_default(),
            tabs: Vec::new(),
            active_tab: 0,
            active_tool: ActiveTool::Select,
            active_chest_item: None,
            default_style: MarkupStyle::default(),
            default_meas_opts: MeasurementOptions::default(),
            panels,
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
            paste_count: 0,
            force_close_allowed: false,
            last_window_title: String::new(),
            worker,
            visible_pages: None,
            pdf_job: None,
            base_textures: HashMap::new(),
            thumb_textures: HashMap::new(),
            tile_textures: HashMap::new(),
            in_flight_tiles: HashMap::new(),
            image_sources: HashMap::new(),
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
            updates: UpdateCheck::default(),
            screenshot_out: launch.screenshot_out,
            screenshot_frames_waited: 0,
            screenshot_requested: false,
            screenshot_done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        if launch.demo {
            match open_demo_chile_document() {
                Ok(tab) => app.add_and_activate_tab(tab, &cc.egui_ctx),
                Err(err) => app.show_toast(err),
            }
        }
        for path in launch.files {
            app.open_path(path, &cc.egui_ctx);
        }
        if app.screenshot_out.is_none() {
            app.updates.check_on_startup(&cc.egui_ctx);
        }
        app
    }

    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        match open_pdf_file(&path) {
            Ok(tab) => {
                self.settings.push_recent(path);
                self.add_and_activate_tab(tab, ctx);
            }
            Err(err) => self.show_toast(err),
        }
    }

    fn commit_active_edit(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            self.canvas.commit_inline_edit(tab, ctx);
        }
    }

    fn reset_canvas_interaction(&mut self) {
        self.canvas = CanvasState {
            active_chest_template: self.canvas.active_chest_template.take(),
            last_viewport_size: self.canvas.last_viewport_size,
            ..Default::default()
        };
    }

    pub fn add_and_activate_tab(&mut self, tab: DocumentTab, ctx: &egui::Context) {
        self.commit_active_edit(ctx);
        self.reset_canvas_interaction();
        if self.tabs.is_empty() {
            self.panels.left_open = self.settings.panel_left_open;
            self.panels.right_open = self.settings.panel_right_open;
        }
        let doc_id = tab.id;
        let revision = tab.render_revision;
        let source = Arc::clone(&tab.raw_pdf_bytes);
        self.worker.send(RenderCommand::LoadDocument {
            doc_id,
            revision,
            clean_pdf_bytes: Arc::clone(&source),
        });
        let page_count = tab.pages.len();
        let detect_scales = tab.pages.iter().any(|page| !page.scale.is_set);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.ensure_page_requested(doc_id, 0);
        for p in 0..page_count.min(20) {
            self.ensure_thumb_requested(doc_id, p);
        }
        self.worker.send(RenderCommand::ExtractSearchText {
            doc_id,
            revision,
            raw_pdf_bytes: source,
            detect_scales,
        });
    }

    fn reload_doc_render_cache(&mut self, doc_id: Uuid) {
        let mut page_count = 0usize;
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == doc_id) {
            page_count = tab.pages.len();
            tab.render_revision += 1;
            let revision = tab.render_revision;
            let source = Arc::clone(&tab.raw_pdf_bytes);
            self.worker.send(RenderCommand::LoadDocument {
                doc_id,
                revision,
                clean_pdf_bytes: Arc::clone(&source),
            });
            self.worker.send(RenderCommand::ExtractSearchText {
                doc_id,
                revision,
                raw_pdf_bytes: source,
                detect_scales: false,
            });
            self.clear_render_cache(doc_id);
        }
        for i in 0..page_count.min(20) {
            self.ensure_thumb_requested(doc_id, i);
        }
    }

    fn clear_render_cache(&mut self, doc_id: Uuid) {
        self.base_textures.retain(|(id, _), _| *id != doc_id);
        self.thumb_textures.retain(|(id, _), _| *id != doc_id);
        self.tile_textures.retain(|(id, _, _), _| *id != doc_id);
        self.in_flight_tiles.retain(|(id, _, _), _| *id != doc_id);
        self.snap_indices.retain(|(id, _), _| *id != doc_id);
        self.requested_base.retain(|(id, _)| *id != doc_id);
        self.requested_thumbs.retain(|(id, _)| *id != doc_id);
        self.requested_snaps.retain(|(id, _)| *id != doc_id);
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

    fn update_visible_pages(&mut self) {
        let visible = self.tabs.get(self.active_tab).map(|tab| VisiblePages {
            document: tab.id,
            primary: tab.camera.page_index,
            secondary: (tab.split_mode != SplitViewMode::None).then_some(tab.secondary_camera.page_index),
        });
        if visible == self.visible_pages {
            return;
        }
        self.visible_pages = visible;
        let keep_page = |&(doc, page): &(Uuid, usize)| visible.is_some_and(|v| v.keep_base(doc, page));
        let keep_tile = |&(doc, page, secondary): &(Uuid, usize, bool)| {
            visible.is_some_and(|v| v.keep_tile(doc, page, secondary))
        };
        self.base_textures.retain(|key, _| keep_page(key));
        self.snap_indices.retain(|key, _| keep_page(key));
        self.requested_base.retain(keep_page);
        self.requested_snaps.retain(keep_page);
        self.tile_textures.retain(|key, _| keep_tile(key));
        self.in_flight_tiles.retain(|key, _| keep_tile(key));
    }

    fn process_worker_results(&mut self, ctx: &egui::Context) {
        for res in self.worker.drain_results() {
            // The previous page tree may still be rendering after a page edit.
            if !self.tabs.iter().any(|tab| {
                tab.id == res.document_id() && tab.render_revision == res.document_revision()
            }) {
                continue;
            }
            match res {
                RenderResult::ThumbnailReady {
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
                    if !self.requested_base.contains(&(doc_id, page_index)) {
                        continue;
                    }
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
                    is_secondary,
                    crop_rect_pt,
                    image,
                    ..
                } => {
                    if !self.visible_pages.is_some_and(|v| v.keep_tile(doc_id, page_index, is_secondary)) {
                        continue;
                    }
                    self.in_flight_tiles.remove(&(doc_id, page_index, is_secondary));
                    let ci = ColorImage::from_rgba_premultiplied(
                        [image.width, image.height],
                        &image.premul_rgba,
                    );
                    let tex = ctx.load_texture(
                        format!("tile_{doc_id}_{page_index}_{is_secondary}"),
                        ci,
                        TextureOptions::LINEAR,
                    );
                    let scale_px_per_pt = (image.width as f64 / crop_rect_pt.width().max(1.0))
                        .min(image.height as f64 / crop_rect_pt.height().max(1.0));
                    self.tile_textures.insert(
                        (doc_id, page_index, is_secondary),
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
                    ..
                } => {
                    if self.requested_snaps.contains(&(doc_id, page_index)) {
                        self.snap_indices.insert((doc_id, page_index), index);
                    }
                }
                RenderResult::SearchTextReady {
                    doc_id,
                    text_by_page,
                    scale_detection_source,
                    ..
                } => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == doc_id) {
                        if let Some(source) = scale_detection_source {
                            if !Arc::ptr_eq(&source, &tab.raw_pdf_bytes)
                                && source.as_slice() != tab.raw_pdf_bytes.as_slice()
                            {
                                continue;
                            }
                            apply_text_page_scales(&mut tab.pages, &text_by_page);
                        }
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
        let preview = match &self.modal {
            Some(ActiveModal::PhotoPreview { attachment_id, .. }) => Some(*attachment_id),
            _ => None,
        };
        if let Some(tab) = self.tabs.get(self.active_tab) {
            let selected = tab.selected_ids.first().copied();
            for m in &tab.markups {
                let on_canvas = m.page_index == tab.camera.page_index
                    || (tab.split_mode != SplitViewMode::None && m.page_index == tab.secondary_camera.page_index);
                let image = match &m.kind {
                    MarkupKind::Image { png_b64, .. } if on_canvas => Some((m.id, png_b64)),
                    _ => None,
                };
                let photos = m.attachments.iter()
                    .filter(|a| selected == Some(m.id) || preview == Some(a.id))
                    .map(|a| (a.id, &a.image_b64));
                for (id, source) in image.into_iter().chain(photos) {
                    if source.is_empty() {
                        continue;
                    }
                    if let Some(cached) = self.image_sources.get_mut(&id)
                        && Arc::ptr_eq(&cached.source, source)
                    {
                        cached.visible = true;
                        continue;
                    }
                    self.image_sources.insert(id, CachedImageSource {
                        source: Arc::clone(source),
                        visible: true,
                    });
                    let decoded = crate::pdf::base64_decode(source)
                        .and_then(|raw| image::load_from_memory(&raw).ok());
                    if let Some(img) = decoded {
                        let rgba = img.into_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let ci = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        let tex = ctx.load_texture(format!("img_{id}"), ci, TextureOptions::LINEAR);
                        self.image_textures.insert(id, tex);
                        self.failed_image_textures.remove(&id);
                    } else {
                        self.image_textures.remove(&id);
                        self.failed_image_textures.insert(id);
                    }
                }
            }
        }
        self.image_sources.retain(|id, cached| {
            if std::mem::take(&mut cached.visible) {
                return true;
            }
            self.image_textures.remove(id);
            self.failed_image_textures.remove(id);
            false
        });
    }

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<UiAction>) {
        if self.modal.is_some() || self.pdf_job.is_some() {
            return;
        }
        if self.canvas.inline_edit.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S)) {
                actions.push(UiAction::SaveDocumentAs);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                actions.push(UiAction::SaveDocument);
            }
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;
            let shift = i.modifiers.shift;
            let alt = i.modifiers.alt;
            let has_doc = !self.tabs.is_empty();
            let in_poly_creation = matches!(
                self.canvas.creation,
                Some(crate::ui::canvas::CreationState::Poly { .. })
            );

            if ctrl && i.key_pressed(egui::Key::O) {
                actions.push(UiAction::OpenPdfDialog);
            } else if ctrl && shift && i.key_pressed(egui::Key::S) {
                actions.push(UiAction::SaveDocumentAs);
            } else if ctrl && i.key_pressed(egui::Key::S) {
                actions.push(UiAction::SaveDocument);
            } else if ctrl && shift && i.key_pressed(egui::Key::Z) {
                actions.push(UiAction::Redo);
            } else if ctrl && i.key_pressed(egui::Key::Z) {
                actions.push(UiAction::Undo);
            } else if ctrl && i.key_pressed(egui::Key::Y) {
                actions.push(UiAction::Redo);
            } else if ctrl && i.key_pressed(egui::Key::C) {
                actions.push(UiAction::CopySelected);
            } else if ctrl && shift && i.key_pressed(egui::Key::V) {
                actions.push(UiAction::PasteInPlace);
            } else if ctrl && i.key_pressed(egui::Key::V) {
                actions.push(UiAction::PasteClipboard);
            } else if ctrl && i.key_pressed(egui::Key::D) {
                actions.push(UiAction::DuplicateSelected);
            } else if ctrl && shift && i.key_pressed(egui::Key::G) {
                actions.push(UiAction::UngroupSelected);
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
            } else if has_doc && i.key_pressed(egui::Key::F8) {
                self.panels.markups_open = !self.panels.markups_open;
            } else if i.key_pressed(egui::Key::F9) {
                self.panels.left_open = !self.panels.left_open;
            } else if i.key_pressed(egui::Key::F10) {
                self.panels.right_open = !self.panels.right_open;
            } else if has_doc
                && (i.key_pressed(egui::Key::Delete)
                    || (!in_poly_creation && i.key_pressed(egui::Key::Backspace)))
            {
                actions.push(UiAction::DeleteSelected);
            } else if has_doc && !ctrl && !shift && !alt {
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
                    (egui::Key::I, ActiveTool::Image),
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
            } else if has_doc && !ctrl && shift && !alt {
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

    fn start_pdf_job(
        &mut self,
        label: &'static str,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<PdfJobResult, String> + Send + 'static,
    ) {
        let (send, receive) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        match std::thread::Builder::new()
            .name("redbim-pdf-operation".into())
            .spawn(move || {
                let _ = send.send(work());
                repaint.request_repaint();
            }) {
            Ok(_) => {
                self.pdf_job = Some(PdfJob { label, receive });
                ctx.request_repaint();
            }
            Err(error) => self.show_toast(format!("No se pudo iniciar la operación PDF: {error}")),
        }
    }

    pub fn save_document_to(
        &mut self,
        path: PathBuf,
        remember_destination: bool,
        ctx: &egui::Context,
    ) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let doc_id = tab.id;
        let snapshot = tab.pdf_operation_snapshot(None);
        self.start_pdf_job("Guardando PDF…", ctx, move || {
            let bytes = save_pdf_file(&snapshot, &path)?;
            Ok(PdfJobResult::Saved {
                doc_id,
                path,
                bytes,
                remember_destination,
            })
        });
    }

    pub fn export_flattened_to(&mut self, path: PathBuf, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let snapshot = tab.pdf_operation_snapshot(None);
        self.start_pdf_job("Exportando PDF aplanado…", ctx, move || {
            flatten_pdf_file(&snapshot, &path)?;
            Ok(PdfJobResult::Exported)
        });
    }

    pub fn flatten_document(&mut self, recoverable: bool, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let doc_id = tab.id;
        let mut snapshot = tab.pdf_operation_snapshot(None);
        self.start_pdf_job("Aplanando marcas…", ctx, move || {
            let count = flatten_in_place_in_tab(&mut snapshot, recoverable)?;
            Ok(PdfJobResult::Flattened {
                doc_id,
                bytes: snapshot.raw_pdf_bytes,
                remaining: snapshot.markups,
                count,
                recoverable,
            })
        });
    }

    pub fn combine_documents(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        if paths.len() < 2 {
            self.show_toast("Seleccione al menos 2 archivos PDF para combinar");
            return;
        }
        self.commit_active_edit(ctx);
        self.start_pdf_job("Combinando planos…", ctx, move || {
            let bytes = combine_pdf_files(&paths)?;
            let tab = open_pdf_from_bytes("Combinado_RedBeam.pdf", None, bytes)?;
            Ok(PdfJobResult::Combined(Box::new(tab)))
        });
    }

    pub fn rotate_document_page(&mut self, page_index: usize, delta: i32, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(page) = tab.pages.get(page_index).cloned() else {
            self.show_toast("Página no encontrada");
            return;
        };
        let doc_id = tab.id;
        let raw = Arc::clone(&tab.raw_pdf_bytes);
        self.start_pdf_job("Rotando página…", ctx, move || {
            let rotated = rotate_pdf_page(&raw, &page, page_index, delta)?;
            Ok(PdfJobResult::PageRotated {
                doc_id,
                page_index,
                rotated,
            })
        });
    }

    pub fn delete_document_page(&mut self, page_index: usize, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let doc_id = tab.id;
        let raw = Arc::clone(&tab.raw_pdf_bytes);
        let page_count = tab.pages.len();
        self.start_pdf_job("Eliminando página…", ctx, move || {
            let bytes = delete_pdf_page(&raw, page_count, page_index)?;
            Ok(PdfJobResult::PageDeleted {
                doc_id,
                page_index,
                bytes,
            })
        });
    }

    pub fn extract_document_page(&mut self, page_index: usize, path: PathBuf, ctx: &egui::Context) {
        if self.pdf_job.is_some() {
            return;
        }
        self.commit_active_edit(ctx);
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let snapshot = tab.pdf_operation_snapshot(Some(page_index));
        self.start_pdf_job("Extrayendo página…", ctx, move || {
            let bytes = extract_pages_to_pdf(&snapshot, &[page_index])?;
            crate::storage::atomic_write_file(&path, &bytes)?;
            Ok(PdfJobResult::PageExtracted { page_index })
        });
    }

    fn poll_pdf_job(&mut self, ctx: &egui::Context) {
        use std::sync::mpsc::TryRecvError;

        let Some(job) = &self.pdf_job else { return };
        let result = match job.receive.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("La operación PDF se interrumpió.".into()),
        };
        self.pdf_job = None;
        match result {
            Ok(PdfJobResult::Saved {
                doc_id,
                path,
                bytes,
                remember_destination,
            }) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == doc_id) {
                    tab.raw_pdf_bytes = Arc::new(bytes);
                    tab.title = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Documento.pdf")
                        .to_string();
                    tab.file_path = Some(path.clone());
                    tab.dirty = false;
                    if remember_destination {
                        self.settings.push_recent(path);
                    }
                }
                self.show_toast("PDF guardado correctamente");
            }
            Ok(PdfJobResult::Exported) => self.show_toast("PDF aplanado exportado correctamente"),
            Ok(PdfJobResult::Flattened {
                doc_id,
                bytes,
                remaining,
                count,
                recoverable,
            }) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == doc_id) {
                    tab.raw_pdf_bytes = bytes;
                    tab.markups = remaining;
                    tab.selected_ids.clear();
                    tab.undo_stack.clear();
                    tab.redo_stack.clear();
                    tab.dirty = true;
                    let page = tab.active_page_index();
                    self.reload_doc_render_cache(doc_id);
                    self.ensure_page_requested(doc_id, page);
                }
                let recovery = if recoverable {
                    " (recuperables con Desaplanar)"
                } else {
                    ""
                };
                self.show_toast(format!(
                    "{count} marca(s) aplanada(s) en el plano{recovery}"
                ));
            }
            Ok(PdfJobResult::PageRotated {
                doc_id,
                page_index,
                rotated,
            }) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == doc_id) {
                    apply_rotated_page(tab, page_index, rotated);
                    self.reload_doc_render_cache(doc_id);
                    self.ensure_page_requested(doc_id, page_index);
                    self.show_toast(format!("Página {} rotada", page_index + 1));
                }
            }
            Ok(PdfJobResult::PageDeleted {
                doc_id,
                page_index,
                bytes,
            }) => {
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == doc_id) {
                    apply_deleted_page(tab, page_index, bytes);
                    let current = tab.active_page_index();
                    self.reload_doc_render_cache(doc_id);
                    self.ensure_page_requested(doc_id, current);
                    self.show_toast(format!("Página {} eliminada", page_index + 1));
                }
            }
            Ok(PdfJobResult::PageExtracted { page_index }) => {
                self.show_toast(format!("Página {} extraída correctamente", page_index + 1));
            }
            Ok(PdfJobResult::Combined(tab)) => {
                let pages = tab.pages.len();
                self.add_and_activate_tab(*tab, ctx);
                self.show_toast(format!("PDF combinado creado ({pages} páginas)"));
            }
            Err(error) => self.show_toast(error),
        }
    }

    fn execute_actions(&mut self, ctx: &egui::Context, actions: Vec<UiAction>) {
        for action in actions {
            if self.pdf_job.is_some() {
                break;
            }
            match action {
                UiAction::NewBlankDocument => {
                    // Default Chilean ISO A1 landscape (841 x 594 mm)
                    let bytes = create_blank_pdf_bytes(841.0 * PT_PER_MM, 594.0 * PT_PER_MM, 1);
                    if let Ok(tab) = open_pdf_from_bytes("Nuevo_Plano_A1.pdf", None, bytes) {
                        self.add_and_activate_tab(tab, ctx);
                    }
                }
                UiAction::OpenPdfDialog => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Documentos PDF", &["pdf"])
                        .pick_file()
                    {
                        self.open_path(path, ctx);
                    }
                }
                UiAction::OpenPdfPath(path) => self.open_path(path, ctx),
                UiAction::OpenDemoChile => match open_demo_chile_document() {
                    Ok(tab) => self.add_and_activate_tab(tab, ctx),
                    Err(err) => self.show_toast(err),
                },
                UiAction::SaveDocument | UiAction::SaveDocumentAs => {
                    let Some(tab) = self.tabs.get(self.active_tab) else { continue };
                    let target = if matches!(action, UiAction::SaveDocument) {
                        tab.file_path.clone()
                    } else {
                        None
                    };
                    let target = target.or_else(|| rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name(&tab.title)
                        .save_file());
                    if let Some(path) = target {
                        self.save_document_to(path, true, ctx);
                    }
                }
                UiAction::ExportFlattenedPdf => {
                    if !self.tabs.is_empty()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf"])
                            .set_file_name("Plano_Aplanado_RedBeam.pdf")
                            .save_file()
                    {
                        self.export_flattened_to(path, ctx);
                    }
                }
                UiAction::FlattenInPlace { recoverable } => {
                    self.flatten_document(recoverable, ctx);
                }
                UiAction::UnflattenDocument => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        unflatten_pdf_in_tab(tab).map(|cnt| (doc_id, pg, cnt))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, cnt)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!(
                                "Desaplanado completo: {cnt} marca(s) recuperada(s) como editables"
                            ));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::ReplaceCurrentPageContent => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Documento PDF revisión nueva", &["pdf"])
                        .pick_file()
                    {
                        match std::fs::read(&path)
                            .map_err(|e| format!("Error leyendo PDF: {e}"))
                            .and_then(|src_bytes| {
                                let tab = self
                                    .tabs
                                    .get_mut(self.active_tab)
                                    .ok_or_else(|| "Sin documento activo".to_string())?;
                                let doc_id = tab.id;
                                let target_pg = tab.active_page_index();
                                let src_pg = lopdf::Document::load_mem(&src_bytes)
                                    .map(|d| target_pg.min(d.get_pages().len().saturating_sub(1)))
                                    .unwrap_or(0);
                                replace_page_content_in_doc(tab, target_pg, &src_bytes, src_pg)?;
                                Ok((doc_id, target_pg))
                            }) {
                            Ok((doc_id, target_pg)) => {
                                self.reload_doc_render_cache(doc_id);
                                self.ensure_page_requested(doc_id, target_pg);
                                self.show_toast(format!(
                                    "Contenido de lámina {} reemplazado conservando todas las marcas",
                                    target_pg + 1
                                ));
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::RunOverlay {
                    pdf_b_path,
                    color_a,
                    color_b,
                    alignment,
                    all_pages,
                } => {
                    let source = self
                        .tabs
                        .get(self.active_tab)
                        .map(|t| (t.raw_pdf_bytes.clone(), t.active_page_index()));
                    if let Some((bytes_a, page_a)) = source {
                        match std::fs::read(&pdf_b_path)
                            .map_err(|e| format!("No se pudo leer PDF B: {e}"))
                            .and_then(|bytes_b| {
                                let page_pairs = if all_pages {
                                    Vec::new()
                                } else {
                                    let pb = lopdf::Document::load_mem(&bytes_b)
                                        .map(|d| page_a.min(d.get_pages().len().saturating_sub(1)))
                                        .unwrap_or(0);
                                    vec![(page_a, pb)]
                                };
                                let cfg = OverlayConfig {
                                    color_a,
                                    color_b,
                                    alignment,
                                    max_side_px: 1800,
                                    page_pairs,
                                };
                                overlay_pdf_pages(&bytes_a, &bytes_b, &cfg)
                            }) {
                            Ok((ov_tab, report)) => {
                                self.add_and_activate_tab(ov_tab, ctx);
                                self.show_toast(format!(
                                    "Overlay listo ({} pág., coincidencia {:.0}%)",
                                    report.pages_overlaid,
                                    report.overlap_ratio * 100.0
                                ));
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::RunVisualSearch(rect_pt) => {
                    let params = self.tabs.get(self.active_tab).and_then(|t| {
                        let pg = t.active_page_index();
                        let pm = t.pages.get(pg)?;
                        Some((t.raw_pdf_bytes.clone(), pg, pm.width_pt, pm.height_pt))
                    });
                    if let Some((raw, pg, w_pt, h_pt)) = params {
                        match run_visual_search_on_page(&raw, pg, w_pt, h_pt, rect_pt, 0.58, true) {
                            Ok(result) => {
                                let tab_ref = self.tabs.get(self.active_tab);
                                self.modal = Some(ActiveModal::from_request(
                                    crate::ui::actions::DialogRequest::VisualSearchResults {
                                        result,
                                    },
                                    tab_ref,
                                ));
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::CreateCountFromVisualSearch {
                    page_index,
                    subject,
                    points,
                } => {
                    if !points.is_empty()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        let n = points.len();
                        tab.push_undo();
                        let m = Markup::new(
                            page_index,
                            subject.clone(),
                            &self.settings.author_name,
                            self.default_style.clone(),
                            MarkupKind::MeasureCount {
                                points,
                                opts: self.default_meas_opts.clone(),
                            },
                        );
                        let id = m.id;
                        tab.markups.push(m);
                        tab.select_only(id);
                        self.show_toast(format!(
                            "Conteo automático creado: {subject} ({n} unidades)"
                        ));
                    }
                }
                UiAction::ApplyHeadersFooters(cfg) => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        apply_headers_footers_in_tab(tab, &cfg).map(|n| (doc_id, pg, n))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, n)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!(
                                "Encabezados y pies aplicados en {n} página(s)"
                            ));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::RemoveHeadersFooters => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        remove_headers_footers_in_tab(tab).map(|n| (doc_id, pg, n))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, n)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!("Encabezados/pies removidos ({n})"));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::RunBatchJob(cfg) => match run_batch_job(&cfg) {
                    Ok(rep) => {
                        let csv_note = if rep.consolidated_csv_path.is_some() {
                            format!(" · CSV consolidado ({} marcas)", rep.total_markups_exported)
                        } else {
                            String::new()
                        };
                        let err_note = if rep.failed_files.is_empty() {
                            String::new()
                        } else {
                            format!(" · {} con error", rep.failed_files.len())
                        };
                        self.show_toast(format!(
                            "Lote completado: {} PDF(s), {} páginas{csv_note}{err_note}",
                            rep.processed_files, rep.total_pages_processed
                        ));
                    }
                    Err(e) => self.show_toast(e),
                },
                UiAction::AttachPhotoToSelected => {
                    let target_id = self
                        .tabs
                        .get(self.active_tab)
                        .and_then(|t| t.selected_ids.first().copied());
                    if let Some(mid) = target_id
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Fotos (PNG/JPG)", &["png", "jpg", "jpeg"])
                            .pick_file()
                    {
                        let caption = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Foto terreno")
                            .to_string();
                        match std::fs::read(&path)
                            .map_err(|e| format!("Error leyendo foto: {e}"))
                            .and_then(|bytes| MarkupAttachment::from_image_bytes(caption, &bytes))
                        {
                            Ok(att) => {
                                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                                    tab.push_undo();
                                    if let Some(m) = tab.markups.iter_mut().find(|x| x.id == mid) {
                                        m.attachments.push(att);
                                        m.modified_at = chrono::Utc::now();
                                    }
                                    self.show_toast("Foto de terreno adjuntada a la marca");
                                }
                            }
                            Err(e) => self.show_toast(e),
                        }
                    } else if target_id.is_none() {
                        self.show_toast("Seleccione una marca antes de adjuntar una foto");
                    }
                }
                UiAction::DeleteAttachment {
                    markup_id,
                    attachment_id,
                } => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        if let Some(m) = tab.markups.iter_mut().find(|x| x.id == markup_id) {
                            m.attachments.retain(|a| a.id != attachment_id);
                            m.modified_at = chrono::Utc::now();
                        }
                    }
                }
                UiAction::ConsumeToolSequence { item_id, markup_id } => {
                    if let Some(token) = self.tool_chest.advance_item_sequence(item_id) {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab)
                            && let Some(m) = tab.markups.iter_mut().find(|x| x.id == markup_id)
                        {
                            m.label = token.clone();
                            m.set_text_content(token.clone());
                            fit_callout_text(ctx, m, false);
                            m.modified_at = chrono::Utc::now();
                        }
                        // Keep active_chest_template in sync with the incremented sequence
                        if let Some(ref mut tpl) = self.canvas.active_chest_template
                            && tpl.id == item_id
                            && let Some(ref mut seq) = tpl.sequence
                        {
                            let _ = seq.take_next();
                        }
                        self.show_toast(format!("Secuencia aplicada: {token}"));
                    }
                }
                UiAction::ExportToolSet(set_id) => {
                    if let Some(set) = self.tool_chest.sets.iter().find(|s| s.id == set_id) {
                        let safe_name = format!(
                            "{}.redbeam-tools.json",
                            set.title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
                        );
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Conjunto RedBeam", &["json"])
                            .set_file_name(&safe_name)
                            .save_file()
                        {
                            match set.export_to_file(&path) {
                                Ok(()) => self.show_toast(format!(
                                    "Conjunto '{}' exportado",
                                    set.title
                                )),
                                Err(e) => self.show_toast(e),
                            }
                        }
                    }
                }
                UiAction::ImportToolSet => {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Conjunto RedBeam (.redbeam-tools.json)", &["json"])
                        .pick_file()
                    {
                        match ToolSet::import_from_file(&path) {
                            Ok(set) => {
                                let title = set.title.clone();
                                let cnt = set.items.len();
                                self.tool_chest.sets.push(set);
                                self.tool_chest.save();
                                self.show_toast(format!(
                                    "Conjunto '{title}' importado ({cnt} herramientas)"
                                ));
                            }
                            Err(e) => self.show_toast(e),
                        }
                    }
                }
                UiAction::ApplyRedactions { fill_color } => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        apply_redactions_in_tab(tab, fill_color).map(|rep| (doc_id, pg, rep))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, rep)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!(
                                "Censura aplicada: {} redacción(es) en {} página(s)",
                                rep.redactions_applied, rep.pages_sanitized
                            ));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::RunColorProcess(cfg) => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        apply_color_process_in_tab(tab, &cfg).map(|n| (doc_id, pg, n))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, n)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!(
                                "Color procesado ({}) en {n} página(s)",
                                cfg.mode.label_es()
                            ));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::RunReduceFileSize(preset) => {
                    let res = if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let doc_id = tab.id;
                        let pg = tab.active_page_index();
                        reduce_pdf_file_size_in_tab(tab, preset).map(|rep| (doc_id, pg, rep))
                    } else {
                        continue;
                    };
                    match res {
                        Ok((doc_id, pg, rep)) => {
                            self.reload_doc_render_cache(doc_id);
                            self.ensure_page_requested(doc_id, pg);
                            self.show_toast(format!(
                                "PDF optimizado: {:.1} KB → {:.1} KB (-{:.1}%, {} img)",
                                rep.bytes_before as f64 / 1024.0,
                                rep.bytes_after as f64 / 1024.0,
                                rep.saved_percent(),
                                rep.images_reencoded
                            ));
                        }
                        Err(e) => self.show_toast(e),
                    }
                }
                UiAction::ExportCsv(delim) => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("CSV Excel", &["csv"])
                            .set_file_name("Cubicación_RedBeam.csv")
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
                            .add_filter("Marcas RedBeam", &["json"])
                            .set_file_name("marcas.redbeam.json")
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
                            .add_filter("Marcas RedBeam", &["json"])
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
                        self.combine_documents(paths, ctx);
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
                                self.add_and_activate_tab(diff_tab, ctx);
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
                        let default_name = format!("Pagina_{}_RedBeam.pdf", pg + 1);
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf"])
                            .set_file_name(&default_name)
                            .save_file()
                        {
                            self.extract_document_page(pg, path, ctx);
                        }
                    }
                }
                UiAction::CloseTab(idx) => {
                    if idx == self.active_tab {
                        self.commit_active_edit(ctx);
                    }
                    if let Some(tab) = self.tabs.get(idx) {
                        if tab.dirty {
                            self.modal = Some(ActiveModal::from_request(
                                crate::ui::actions::DialogRequest::ConfirmClose {
                                    tab_idx: Some(idx),
                                },
                                Some(tab),
                            ));
                        } else {
                            self.close_tab_immediate(idx);
                        }
                    }
                }
                UiAction::ForceCloseTab(idx) => {
                    self.close_tab_immediate(idx);
                }
                UiAction::SelectTab(idx) => {
                    if idx < self.tabs.len() && idx != self.active_tab {
                        self.commit_active_edit(ctx);
                        self.reset_canvas_interaction();
                        self.active_tab = idx;
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
                        self.paste_count = 0;
                    }
                }
                UiAction::PasteClipboard => {
                    if !self.clipboard.is_empty()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        self.paste_count += 1;
                        let offset = 16.0 * self.paste_count as f64;
                        let pg = tab.active_page_index();
                        tab.push_undo();
                        let now = chrono::Utc::now();
                        let mut group_remap: HashMap<Uuid, Uuid> = HashMap::new();
                        let mut new_ids = Vec::new();
                        for orig in &self.clipboard {
                            let mut c = orig.clone();
                            c.id = Uuid::new_v4();
                            c.locked = false;
                            c.page_index = pg;
                            c.created_at = now;
                            c.modified_at = now;
                            if let Some(gid) = c.group_id {
                                let new_gid = *group_remap.entry(gid).or_insert_with(Uuid::new_v4);
                                c.group_id = Some(new_gid);
                            }
                            tab.ensure_layer(&c.layer);
                            translate_markup(&mut c, Point2::new(offset, offset));
                            new_ids.push(c.id);
                            tab.markups.push(c);
                        }
                        tab.selected_ids = new_ids;
                    }
                }
                UiAction::PasteInPlace => {
                    if !self.clipboard.is_empty()
                        && let Some(tab) = self.tabs.get_mut(self.active_tab)
                    {
                        let pg = tab.active_page_index();
                        tab.push_undo();
                        let now = chrono::Utc::now();
                        let mut group_remap: HashMap<Uuid, Uuid> = HashMap::new();
                        let mut new_ids = Vec::new();
                        for orig in &self.clipboard {
                            let mut c = orig.clone();
                            c.id = Uuid::new_v4();
                            c.locked = false;
                            c.page_index = pg;
                            c.created_at = now;
                            c.modified_at = now;
                            if let Some(gid) = c.group_id {
                                let new_gid = *group_remap.entry(gid).or_insert_with(Uuid::new_v4);
                                c.group_id = Some(new_gid);
                            }
                            tab.ensure_layer(&c.layer);
                            new_ids.push(c.id);
                            tab.markups.push(c);
                        }
                        tab.selected_ids = new_ids;
                        self.show_toast("Pegado en el mismo lugar (Ctrl+Shift+V)");
                    }
                }
                UiAction::DuplicateSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let sel: Vec<Markup> = tab.selected_markups().into_iter().cloned().collect();
                        if !sel.is_empty() {
                            tab.push_undo();
                            let now = chrono::Utc::now();
                            let mut group_remap: HashMap<Uuid, Uuid> = HashMap::new();
                            let mut new_ids = Vec::new();
                            for orig in sel {
                                let mut c = orig;
                                c.id = Uuid::new_v4();
                                c.locked = false;
                                c.created_at = now;
                                c.modified_at = now;
                                if let Some(gid) = c.group_id {
                                    let new_gid =
                                        *group_remap.entry(gid).or_insert_with(Uuid::new_v4);
                                    c.group_id = Some(new_gid);
                                }
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
                UiAction::SnapSelectedToContent => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let page_idx = tab.active_page_index();
                        let radius_pt = (20.0 / tab.active_camera().zoom.max(0.1)).clamp(8.0, 40.0);
                        if let Some(snap_idx) = self.snap_indices.get(&(tab.id, page_idx)) {
                            let sel = tab.selected_ids.clone();
                            let locked_layers: HashSet<String> = tab
                                .layers
                                .iter()
                                .filter(|l| l.locked)
                                .map(|l| l.name.clone())
                                .collect();
                            let mut total_snapped = 0usize;
                            let mut any_snap = false;
                            for m in &tab.markups {
                                if sel.contains(&m.id)
                                    && m.page_index == page_idx
                                    && !m.locked
                                    && !locked_layers.contains(&m.layer)
                                {
                                    let mut clone = m.clone();
                                    let count = snap_markup_points_to_index(&mut clone, snap_idx, radius_pt);
                                    if count > 0 {
                                        any_snap = true;
                                        total_snapped += count;
                                    }
                                }
                            }
                            if any_snap {
                                tab.push_undo();
                                for m in &mut tab.markups {
                                    if sel.contains(&m.id)
                                        && m.page_index == page_idx
                                        && !m.locked
                                        && !locked_layers.contains(&m.layer)
                                    {
                                        snap_markup_points_to_index(m, snap_idx, radius_pt);
                                    }
                                }
                                self.show_toast(format!(
                                    "{total_snapped} punto(s) ajustado(s) a esquinas del plano"
                                ));
                            } else {
                                self.show_toast("No hay esquinas cercanas dentro del rango");
                            }
                        } else {
                            self.show_toast("Sin índice vectorial en esta página");
                        }
                    }
                }
                UiAction::ToggleLockSelected => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let sel = tab.selected_ids.clone();
                        if !sel.is_empty() {
                            tab.push_undo();
                            let any_unlocked = tab
                                .markups
                                .iter()
                                .any(|m| sel.contains(&m.id) && !m.locked);
                            let new_lock = any_unlocked;
                            for m in &mut tab.markups {
                                if sel.contains(&m.id) {
                                    m.locked = new_lock;
                                    m.modified_at = chrono::Utc::now();
                                }
                            }
                            self.show_toast(if new_lock {
                                "Puntos fijados (marca bloqueada)"
                            } else {
                                "Marca desbloqueada"
                            });
                        }
                    }
                }
                UiAction::ShowMarkupProperties => {
                    self.panels.right_open = true;
                    self.right_tab = RightPanelTab::Properties;
                }
                UiAction::SetTool(tool) => {
                    self.active_tool = tool;
                    self.active_chest_item = None;
                    self.canvas.active_chest_template = None;
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
                        self.canvas.active_chest_template = Some(item.clone());
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
                        tab.active_camera_mut().fit_page = true;
                    }
                }
                UiAction::FitWidth => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let vw = self.canvas.last_viewport_size.0.max(100.0);
                        if let Some(p) = tab.active_page() {
                            let z = ((vw - 40.0) / p.width_pt.max(1.0)).clamp(0.05, 8.0);
                            let is_sec = tab.active_pane_secondary;
                            tab.active_camera_mut().fit_page = false;
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
                        cam.fit_page = false;
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
                        cam.fit_page = false;
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
                        cam.fit_page = true;
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
                UiAction::LocateTextMarkup(id) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        self.canvas.commit_inline_edit(tab, ctx);
                        if let Some(page) = locate_text_markup(tab, id, self.canvas.last_viewport_size) {
                            let doc = tab.id;
                            self.canvas.creation = None;
                            self.canvas.drag = None;
                            self.canvas.smooth_zoom = None;
                            self.ensure_page_requested(doc, page);
                            ctx.request_repaint();
                        }
                    }
                }
                UiAction::JumpToPageText(page_idx) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && page_idx < tab.pages.len()
                    {
                        let cam = tab.active_camera_mut();
                        cam.page_index = page_idx;
                        cam.fit_page = true;
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
                UiAction::ToggleSyncSplitViews => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.sync_split_views = !tab.sync_split_views;
                        if tab.sync_split_views {
                            tab.secondary_camera.zoom = tab.camera.zoom;
                            tab.secondary_camera.pan = tab.camera.pan;
                            tab.secondary_camera.fit_page =
                                tab.camera.fit_page && tab.secondary_camera.page_index == tab.camera.page_index;
                            self.show_toast("Vistas divididas sincronizadas");
                        } else {
                            self.show_toast("Sincronización de vistas desactivada");
                        }
                    }
                }
                UiAction::SetSplitNone => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.split_mode = SplitViewMode::None;
                    }
                }
                UiAction::RotatePage(pg, delta) => self.rotate_document_page(pg, delta, ctx),
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
                                tab.active_camera_mut().fit_page = true;
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
                UiAction::DeletePage(pg) => self.delete_document_page(pg, ctx),
                UiAction::RenamePageLabel { page_index, label } => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab)
                        && let Some(p) = tab.pages.get_mut(page_index)
                    {
                        let trimmed = label.trim().to_string();
                        p.label = if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.clone())
                        };
                        tab.dirty = true;
                        let msg = if trimmed.is_empty() {
                            format!("Etiqueta de página {} restaurada", page_index + 1)
                        } else {
                            format!("Lámina {} etiquetada: {trimmed}", page_index + 1)
                        };
                        self.show_toast(msg);
                    }
                }
                UiAction::FollowHyperlink(target) => match target {
                    crate::model::markup::HyperlinkTarget::Page { page_index } => {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            let max_idx = tab.pages.len().saturating_sub(1);
                            let dest = page_index.min(max_idx);
                            let lbl = tab.page_display_label(dest);
                            let doc_id = tab.id;
                            tab.active_camera_mut().page_index = dest;
                            tab.active_camera_mut().fit_page = true;
                            self.ensure_page_requested(doc_id, dest);
                            self.show_toast(format!("Hipervínculo → {lbl}"));
                        }
                    }
                    crate::model::markup::HyperlinkTarget::Url(url) => {
                        let trimmed = url.trim();
                        let valid_scheme =
                            trimmed.starts_with("https://") || trimmed.starts_with("http://");
                        let has_bad_chars = trimmed.chars().any(|c| {
                            matches!(c, '"' | '&' | '|' | ';' | '<' | '>' | '^' | '\n' | '\r')
                        });
                        if valid_scheme && !has_bad_chars {
                            #[cfg(target_os = "windows")]
                            let res = std::process::Command::new("cmd")
                                .args(["/C", "start", "", trimmed])
                                .spawn();
                            #[cfg(not(target_os = "windows"))]
                            let res = std::process::Command::new("xdg-open").arg(trimmed).spawn();
                            match res {
                                Ok(_) => self.show_toast(format!("Abriendo URL: {trimmed}")),
                                Err(e) => self.show_toast(format!("No se pudo abrir URL: {e}")),
                            }
                        } else {
                            self.show_toast(
                                "URL inválida: debe comenzar con http:// o https://",
                            );
                        }
                    }
                },
                UiAction::DeleteSpace(space_id) => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.push_undo();
                        tab.spaces.retain(|s| s.id != space_id);
                        for sp in &mut tab.spaces {
                            if sp.parent_id == Some(space_id) {
                                sp.parent_id = None;
                            }
                        }
                    }
                }
                UiAction::ToggleHighlightSpaces => {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.highlight_spaces = !tab.highlight_spaces;
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
                                                png_b64: crate::pdf::base64_encode(&png_bytes).into(),
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
                        let locked_layers: HashSet<String> = tab
                            .layers
                            .iter()
                            .filter(|l| l.locked)
                            .map(|l| l.name.clone())
                            .collect();
                        let any_unlocked = tab.markups.iter().any(|m| {
                            sel.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer)
                        });
                        if any_unlocked {
                            tab.push_undo();
                            for m in &mut tab.markups {
                                if sel.contains(&m.id)
                                    && !m.locked
                                    && !locked_layers.contains(&m.layer)
                                {
                                    m.status = st;
                                    m.modified_at = chrono::Utc::now();
                                }
                            }
                        }
                    }
                }
                UiAction::StartInlineEdit(id) => {
                    if let Some(tab) = self.tabs.get(self.active_tab)
                        && let Some(m) = tab.markups.iter().find(|x| x.id == id)
                        && !tab.is_markup_locked(m)
                    {
                        let txt = m
                            .text_content()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| m.comment.clone());
                        self.canvas.inline_edit = Some(InlineTextEdit::new(id, txt));
                    }
                }
                UiAction::RequestRedrawTextures => {}
                UiAction::ShowToast(msg) => self.show_toast(msg),
                UiAction::CheckForUpdates => self.updates.open(ctx),
                UiAction::ExitApp => {
                    self.commit_active_edit(ctx);
                    if self.tabs.iter().any(|t| t.dirty) {
                        self.modal = Some(ActiveModal::from_request(
                            crate::ui::actions::DialogRequest::ConfirmClose { tab_idx: None },
                            self.tabs.get(self.active_tab),
                        ));
                    } else {
                        self.force_close_allowed = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                UiAction::ForceExitApp => {
                    self.force_close_allowed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn close_tab_immediate(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        let doc_id = self.tabs[idx].id;
        let was_active = idx == self.active_tab;
        self.worker.send(RenderCommand::CloseDocument { doc_id });
        self.tabs.remove(idx);
        self.clear_render_cache(doc_id);
        if idx < self.active_tab {
            self.active_tab -= 1;
        }
        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab = self.tabs.len() - 1;
        }
        if self.tabs.is_empty() {
            self.active_tab = 0;
            self.panels.left_open = false;
            self.panels.right_open = false;
        }
        if was_active {
            self.reset_canvas_interaction();
        }
    }

    fn draw_welcome_screen(&self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        let workspace = ui.available_rect_before_wrap();
        let width = (workspace.width() - 32.0).clamp(1.0, 420.0);
        let column = egui::Align2::CENTER_TOP
            .align_size_within_rect(egui::vec2(width, workspace.height()), workspace);
        let top_space = (workspace.height() * 0.14).clamp(16.0, 112.0);

        ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(top_space);
                    let button_width = ui.available_width().min(320.0);
                    if let Some(logo) = &self.logo_texture {
                        ui.add(egui::Image::new((logo.id(), egui::vec2(72.0, 72.0))));
                        ui.add_space(6.0);
                    }
                    ui.label(
                        RichText::new("RedBeam")
                            .strong()
                            .size(32.0)
                            .color(self.theme.accent_red),
                    );
                    ui.label(
                        RichText::new("Marcado, revisión y cubicación de planos PDF")
                            .size(12.5)
                            .color(self.theme.text_secondary),
                    );
                    ui.add_space(18.0);

                    let open_btn = egui::Button::new(
                        RichText::new("Abrir archivo PDF...  (Ctrl+O)")
                            .strong()
                            .size(14.0)
                            .color(self.theme.text_on_accent),
                    )
                    .fill(self.theme.accent_red)
                    .min_size(egui::vec2(button_width, 38.0));
                    if ui.add(open_btn).clicked() {
                        actions.push(UiAction::OpenPdfDialog);
                    }

                    ui.add_space(8.0);
                    let demo_btn = egui::Button::new(
                        RichText::new("Abrir plano de ejemplo (Chile 1:100 · Cubierta FV)")
                            .strong()
                            .size(13.0),
                    )
                    .min_size(egui::vec2(button_width, 34.0));
                    if ui.add(demo_btn).clicked() {
                        actions.push(UiAction::OpenDemoChile);
                    }

                    ui.add_space(8.0);
                    let blank_btn = egui::Button::new(
                        RichText::new("Nuevo plano en blanco (ISO A1 · 841 × 594 mm)")
                            .size(12.5),
                    )
                    .min_size(egui::vec2(button_width, 30.0));
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
                            if ui.button(name).on_hover_text(p.display().to_string()).clicked() {
                                actions.push(UiAction::OpenPdfPath(p.clone()));
                            }
                        }
                    }
                    ui.add_space(16.0);
                });
            });
        });
    }
}

impl eframe::App for RedBimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_pdf_job(&ctx);
        let busy = self.pdf_job.is_some();
        if let Some(job) = &self.pdf_job {
            egui::Modal::new(egui::Id::unique("pdf-operation")).show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(job.label);
                });
            });
        }
        let left_was_open = self.panels.left_open;

        // Intercept OS window close when any tab has unsaved changes
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if busy && close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let closing = close_requested && !busy && !self.force_close_allowed;
        if closing {
            self.commit_active_edit(&ctx);
        }
        if closing && self.tabs.iter().any(|t| t.dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.modal = Some(ActiveModal::from_request(
                crate::ui::actions::DialogRequest::ConfirmClose { tab_idx: None },
                self.tabs.get(self.active_tab),
            ));
        }

        // Keep OS window title synced with active document name & dirty indicator
        let desired_title = if let Some(tab) = self.tabs.get(self.active_tab) {
            let star = if tab.dirty { "*" } else { "" };
            format!("{}{star} — RedBeam", tab.title)
        } else {
            "RedBeam — Visor y Cubicación de Planos PDF (Chile / Métrico)".to_string()
        };
        if desired_title != self.last_window_title {
            self.last_window_title = desired_title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(desired_title));
        }

        self.update_visible_pages();
        if !busy {
            self.process_worker_results(&ctx);
        }
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
        if !busy {
            for p in dropped {
                self.open_path(p, &ctx);
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

        // Window header and menus
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

        // Document tools only apply to an open plan.
        if !self.tabs.is_empty() {
            egui::Panel::top("redbim_main_toolbar")
                .frame(egui::Frame::new().fill(self.theme.toolbar_bg).inner_margin(egui::Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, self.theme.border)))
                .show(ui, |ui| {
                    draw_main_toolbar(
                        ui,
                        self.tabs.get_mut(self.active_tab),
                        self.active_tool,
                        &mut self.default_style,
                        &mut self.default_meas_opts,
                        &mut self.panels,
                        &self.theme,
                        &mut actions,
                    );
                });

            draw_document_tabs(ui, &self.tabs, self.active_tab, &self.theme, &mut actions);
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
                    &self.theme,
                    &mut actions,
                );
            });

        egui::Panel::left("redbim_left_rail")
            .resizable(false)
            .exact_size(SIDEBAR_RAIL_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(self.theme.window_bg)
                    .inner_margin(egui::Margin::symmetric(4, 6))
                    .stroke(Stroke::new(1.0, self.theme.border)),
            )
            .show(ui, |ui| {
                draw_left_rail(
                    ui,
                    &mut self.left_tab,
                    &mut self.panels.left_open,
                    &self.theme,
                );
            });

        let open_sidebar_count =
            u8::from(self.panels.left_open) + u8::from(self.panels.right_open);
        let sidebar_budget = ui.available_width()
            - SIDEBAR_RAIL_WIDTH
            - CANVAS_MIN_WIDTH
            - SIDEBAR_GRIP_WIDTH * f32::from(open_sidebar_count);
        let right_reserve = if self.panels.right_open { RIGHT_PANEL_MIN_WIDTH } else { 0.0 };
        let left_limit = (sidebar_budget - right_reserve).clamp(LEFT_PANEL_MIN_WIDTH, 380.0);
        let left_width = self.left_panel_width.clamp(LEFT_PANEL_MIN_WIDTH, left_limit);

        // Left Panel
        if self.panels.left_open {
            let panel = egui::Panel::left("redbim_left_panel")
                .resizable(false)
                .exact_size(left_width)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(self.theme.panel_bg).inner_margin(egui::Margin::same(8)).stroke(Stroke::new(1.0, self.theme.border)))
                .show(ui, |ui| {
                    let tab_mut = self.tabs.get_mut(self.active_tab);
                    draw_left_panel(
                        ui,
                        self.left_tab,
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
            draw_sidebar_grip(
                ui,
                Sidebar::Left,
                panel.response.rect,
                &mut self.left_panel_width,
                egui::Rangef::new(LEFT_PANEL_MIN_WIDTH, left_limit),
                &self.theme,
            );
        }

        // Right Activity Rail (always visible — 1-click open/close for Right Panel)
        egui::Panel::right("redbim_right_rail")
            .resizable(false)
            .exact_size(SIDEBAR_RAIL_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(self.theme.window_bg)
                    .inner_margin(egui::Margin::symmetric(4, 6))
                    .stroke(Stroke::new(1.0, self.theme.border)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for (kind, icon, tip) in [
                        (
                            RightPanelTab::Properties,
                            IconKind::Properties,
                            "Propiedades de marca y cotas (clic de nuevo para ocultar)",
                        ),
                        (
                            RightPanelTab::Measurements,
                            IconKind::Measurements,
                            "Escala y Mediciones (clic de nuevo para ocultar)",
                        ),
                        (
                            RightPanelTab::Search,
                            IconKind::Search,
                            "Buscar en documento y marcas (clic de nuevo para ocultar)",
                        ),
                        (RightPanelTab::Notes, IconKind::Note, "Notas y textos"),
                    ] {
                        let is_active = self.panels.right_open && self.right_tab == kind;
                        if icon_button(ui, 28.0, icon, is_active, true, tip, &self.theme).clicked()
                        {
                            if is_active {
                                self.panels.right_open = false;
                            } else {
                                self.right_tab = kind;
                                self.panels.right_open = true;
                            }
                        }
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        let arrow = if self.panels.right_open { "»" } else { "«" };
                        let tip = if self.panels.right_open {
                            "Cerrar panel derecho (F10)"
                        } else {
                            "Abrir panel derecho (F10)"
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(arrow)
                                        .size(14.0)
                                        .color(self.theme.text_secondary),
                                )
                                .frame(false)
                                .min_size(egui::vec2(28.0, 26.0)),
                            )
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.panels.right_open = !self.panels.right_open;
                        }
                    });
                });
            });

        // Right Panel (Properties, Measurements & Scale, Search)
        if self.panels.right_open {
            let right_limit = (ui.available_width() - SIDEBAR_GRIP_WIDTH - CANVAS_MIN_WIDTH)
                .clamp(RIGHT_PANEL_MIN_WIDTH, 450.0);
            let right_width = self.right_panel_width.clamp(RIGHT_PANEL_MIN_WIDTH, right_limit);
            let theme = &self.theme;
            let panel_rect = draw_right_sidebar(ui, right_width, theme, |ui| {
                let header_title = match self.right_tab {
                    RightPanelTab::Properties => "Propiedades",
                    RightPanelTab::Measurements => "Escala y Mediciones",
                    RightPanelTab::Search => "Buscar",
                    RightPanelTab::Notes => "Notas y textos",
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(header_title)
                            .strong()
                            .size(12.5)
                            .color(self.theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("»")
                                        .size(13.0)
                                        .color(self.theme.text_secondary),
                                )
                                .frame(false)
                                .min_size(egui::vec2(22.0, 22.0)),
                            )
                            .on_hover_text("Ocultar panel derecho (F10)")
                            .clicked()
                        {
                            self.panels.right_open = false;
                        }
                    });
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
                            &self.image_textures,
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
                    RightPanelTab::Notes => {
                        draw_notes_panel(ui, self.tabs.get(self.active_tab), theme, &mut actions);
                    }
                }
            });
            draw_sidebar_grip(
                ui,
                Sidebar::Right,
                panel_rect,
                &mut self.right_panel_width,
                egui::Rangef::new(RIGHT_PANEL_MIN_WIDTH, right_limit),
                &self.theme,
            );
        }

        // Central Workspace (Welcome screen or PDF Canvas)
        let mut tile_requests: Vec<TileRequest> = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(self.theme.canvas_bg))
            .show(ui, |ui| {
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    if left_was_open != self.panels.left_open {
                        tab.camera.fit_page = true;
                        if tab.split_mode != SplitViewMode::None {
                            tab.secondary_camera.fit_page = true;
                        }
                    }
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
                            in_flight_tiles: &self.in_flight_tiles,
                            thumb_textures: &self.thumb_textures,
                            image_textures: &self.image_textures,
                            snap_indices: &self.snap_indices,
                            modal_open: self.modal.is_some(),
                        },
                        &self.theme,
                        &mut actions,
                        &mut tile_requests,
                    );
                } else {
                    self.draw_welcome_screen(ui, &mut actions);
                }
            });

        // Submit any high-res zoom tile renders needed (deduplicated via in_flight_tiles)
        for req in tile_requests {
            self.in_flight_tiles.insert(
                (req.doc_id, req.page_index, req.is_secondary),
                (req.crop_rect_pt, req.out_w, req.out_h),
            );
            self.generation_counter += 1;
            self.worker.send(RenderCommand::RenderTile {
                doc_id: req.doc_id,
                page_index: req.page_index,
                is_secondary: req.is_secondary,
                crop_rect_pt: req.crop_rect_pt,
                out_w: req.out_w,
                out_h: req.out_h,
                generation: self.generation_counter,
            });
        }

        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            egui::Window::new("Lista de marcas")
                .id(egui::Id::unique("redbim_markups_window"))
                .open(&mut self.panels.markups_open)
                .default_size([860.0, 270.0])
                .min_size([420.0, 170.0])
                .collapsible(false)
                .show(&ctx, |ui| {
                    draw_markups_list_panel(
                        ui,
                        tab,
                        &mut self.markups_list_state,
                        &self.theme,
                        &mut actions,
                    );
                });
        } else {
            self.panels.markups_open = false;
        }

        // Active modal dialog
        {
            let tab_mut = self.tabs.get_mut(self.active_tab);
            draw_active_modal(
                &ctx,
                &mut self.modal,
                tab_mut,
                &mut self.tool_chest,
                &self.image_textures,
                &mut self.settings,
                &self.theme,
                &mut actions,
            );
        }

        self.updates.poll();
        if self.modal.is_none() && !busy && !close_requested && self.updates.show(&ctx) {
            self.commit_active_edit(&ctx);
            if self.tabs.iter().any(|tab| tab.dirty) {
                self.show_toast("Guarda los cambios de tus planos para instalar la actualización.");
            } else {
                match self.updates.launch() {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(message) => self.show_toast(message),
                }
            }
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
                    ctx.request_repaint_after(Duration::from_secs_f32(total - 0.38 - elapsed));
                }

                egui::Area::new(egui::Id::unique("redbim_toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, slide_y])
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(
                                self.theme.panel_bg.gamma_multiply(alpha),
                            )
                            .stroke(Stroke::new(
                                1.0,
                                self.theme.border_strong.gamma_multiply(alpha),
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
                                            .color(self.theme.text_primary.gamma_multiply(alpha))
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

        let commit_panel_widths = !ctx.input(|i| i.pointer.primary_down())
            && (self.settings.panel_left_width != self.left_panel_width
                || self.settings.panel_right_width != self.right_panel_width);
        if commit_panel_widths {
            self.settings.panel_left_width = self.left_panel_width;
            self.settings.panel_right_width = self.right_panel_width;
        }
        if !self.tabs.is_empty() && (commit_panel_widths
            || self.settings.panel_left_open != self.panels.left_open
            || self.settings.panel_right_open != self.panels.right_open)
        {
            self.settings.panel_left_open = self.panels.left_open;
            self.settings.panel_right_open = self.panels.right_open;
            self.settings.save();
        }

        #[cfg(target_os = "windows")]
        crate::ui::menu_bar::handle_window_resize(&ctx);

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
