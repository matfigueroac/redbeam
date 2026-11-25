use crate::geometry::hit_test::{
    HandleKind, apply_handle_drag, box_rect_anchor, callout_landing, hit_test_handle, hit_test_markup,
    is_measure_length_offset_drag_hit, list_handles, markup_bounds, translate_markup,
};
use crate::geometry::math::{
    PageTransform, Point2, Rect2, angle_three_points_deg, clip_hatch_lines,
    dimension_offset_segment, generate_cloud_polyline, polygon_centroid, sample_ellipse,
    sample_three_point_arc, triangulate_polygon_with_holes,
};
use crate::geometry::snap::{
    SnapIndex, SnapKind, SnapResult, apply_ortho_constraint, apply_soft_hv_snap,
    find_best_snap_excluding,
};
use crate::model::document::{AlignKind, DocumentTab, SpaceRegion, SplitViewMode};
use crate::model::markup::{
    BoxShape, CountStyle, HatchPattern, HyperlinkTarget, LineEnd, LineStyle, Markup, MarkupKind,
    MarkupStatus, MarkupStyle, MeasurementOptions, NoteIcon,
};
use crate::model::scale::{PageScale, ViewportScale, resolve_scale_at};
use crate::model::settings::AppSettings;
use crate::model::tool_chest::ActiveTool;
use crate::model::units::{format_angle, format_area, format_length, format_volume};
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Painter, Pos2, Rect, Sense, Stroke,
    TextureHandle, Vec2,
};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Clone)]
pub struct CachedTile {
    pub crop_rect_pt: Rect2,
    pub texture: TextureHandle,
    pub scale_px_per_pt: f64,
}

#[derive(Debug, Clone)]
pub enum CreationState {
    TwoPoint {
        tool: ActiveTool,
        start: Point2,
        current: Point2,
        from_drag: bool,
    },
    ThreePoint {
        tool: ActiveTool,
        points: Vec<Point2>,
        current: Point2,
    },
    Callout {
        arrow_tip: Point2,
        knee: Option<Point2>,
        current: Point2,
    },
    CloudPlus {
        cloud_rect: Rect2,
        current: Point2,
    },
    Poly {
        tool: ActiveTool,
        points: Vec<Point2>,
        current: Point2,
        target_cutout_id: Option<Uuid>,
    },
    Freehand {
        tool: ActiveTool,
        points: Vec<Point2>,
    },
    Count {
        markup_id: Uuid,
    },
}

#[derive(Debug, Clone)]
pub enum DragMode {
    Pan {
        last_screen: Pos2,
    },
    Marquee {
        start_pt: Point2,
        current_pt: Point2,
    },
    MoveSelected {
        last_pt: Point2,
        pushed_undo: bool,
    },
    MoveHandle {
        markup_id: Uuid,
        handle: HandleKind,
        callout_grab_offset: Option<Point2>,
        pushed_undo: bool,
    },
}

#[derive(Debug, Clone)]
pub enum CameraAnim {
    WheelZoom {
        is_secondary: bool,
        target_zoom: f64,
        anchor_page_pt: Point2,
        anchor_screen: Pos2,
    },
    GlideTo {
        is_secondary: bool,
        target_zoom: f64,
        target_pan: Point2,
    },
}

#[derive(Debug)]
pub struct InlineTextEdit {
    id: Uuid,
    text: String,
    subtitle: bool,
    needs_focus: bool,
    changed: bool,
}

impl InlineTextEdit {
    pub fn new(id: Uuid, text: String) -> Self {
        Self {
            id,
            text,
            subtitle: false,
            needs_focus: true,
            changed: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct CanvasState {
    pub creation: Option<CreationState>,
    pub drag: Option<DragMode>,
    pub inline_edit: Option<InlineTextEdit>,
    pub active_cutout_target: Option<Uuid>,
    pub active_chest_template: Option<crate::model::tool_chest::ToolChestItem>,
    pub last_viewport_size: (f64, f64),
    pub hover_page_pt: Option<Point2>,
    pub hover_snap: Option<SnapResult>,
    pub smooth_zoom: Option<CameraAnim>,
    pub last_zoom_activity: Option<std::time::Instant>,
}

impl CanvasState {
    pub fn commit_inline_edit(&mut self, tab: &mut DocumentTab, ctx: &egui::Context) {
        let Some(edit) = self.inline_edit.take() else {
            return;
        };
        let Some(index) = tab.markups.iter().position(|m| m.id == edit.id) else {
            return;
        };
        let markup = &tab.markups[index];
        if tab.is_markup_locked(markup) {
            return;
        }
        let changed = if matches!(markup.kind, MarkupKind::Note { .. }) {
            edit.text != markup.comment
        } else {
            edit.changed
        };
        if changed {
            tab.push_undo();
            let markup = &mut tab.markups[index];
            if edit.subtitle && let MarkupKind::Stamp { subtitle, .. } = &mut markup.kind {
                *subtitle = edit.text;
                markup.modified_at = chrono::Utc::now();
            } else {
                markup.set_text_content(edit.text);
                fit_callout_text(ctx, markup, false);
            }
        }
    }

    pub fn glide_to(&mut self, is_secondary: bool, target_zoom: f64, target_pan: Point2) {
        self.smooth_zoom = Some(CameraAnim::GlideTo {
            is_secondary,
            target_zoom,
            target_pan,
        });
        self.last_zoom_activity = Some(std::time::Instant::now());
    }
}

pub struct TileRequest {
    pub doc_id: Uuid,
    pub page_index: usize,
    pub is_secondary: bool,
    pub crop_rect_pt: Rect2,
    pub out_w: u16,
    pub out_h: u16,
}

pub struct CanvasResources<'a> {
    pub base_textures: &'a HashMap<(Uuid, usize), TextureHandle>,
    pub tile_textures: &'a HashMap<(Uuid, usize, bool), CachedTile>,
    pub in_flight_tiles: &'a HashMap<(Uuid, usize, bool), (Rect2, u16, u16)>,
    pub thumb_textures: &'a HashMap<(Uuid, usize), TextureHandle>,
    pub image_textures: &'a HashMap<Uuid, TextureHandle>,
    pub snap_indices: &'a HashMap<(Uuid, usize), SnapIndex>,
    pub modal_open: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_document_workspace(
    ui: &mut egui::Ui,
    tab: &mut DocumentTab,
    canvas: &mut CanvasState,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
    res: CanvasResources<'_>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
    tile_requests: &mut Vec<TileRequest>,
) {
    let full_rect = ui.available_rect_before_wrap();
    let prev_cam = (tab.camera.zoom, tab.camera.pan);
    let prev_sec = (tab.secondary_camera.zoom, tab.secondary_camera.pan);

    match tab.split_mode {
        SplitViewMode::None => {
            canvas.last_viewport_size = (full_rect.width() as f64, full_rect.height() as f64);
            draw_canvas_pane(
                ui,
                full_rect,
                false,
                tab,
                canvas,
                active_tool,
                default_style,
                default_meas_opts,
                settings,
                &res,
                theme,
                actions,
                tile_requests,
            );
        }
        SplitViewMode::Vertical => {
            let half = full_rect.width() * 0.5;
            let left_r = Rect::from_min_max(full_rect.min, Pos2::new(full_rect.min.x + half - 2.0, full_rect.max.y));
            let right_r = Rect::from_min_max(Pos2::new(full_rect.min.x + half + 2.0, full_rect.min.y), full_rect.max);
            let active_r = if tab.active_pane_secondary { right_r } else { left_r };
            canvas.last_viewport_size = (active_r.width() as f64, active_r.height() as f64);
            draw_canvas_pane(
                ui, left_r, false, tab, canvas, active_tool, default_style, default_meas_opts,
                settings, &res, theme, actions, tile_requests,
            );
            ui.painter().line_segment(
                [Pos2::new(full_rect.min.x + half, full_rect.min.y), Pos2::new(full_rect.min.x + half, full_rect.max.y)],
                Stroke::new(2.0, theme.border_strong),
            );
            draw_canvas_pane(
                ui, right_r, true, tab, canvas, active_tool, default_style, default_meas_opts,
                settings, &res, theme, actions, tile_requests,
            );
        }
        SplitViewMode::Horizontal => {
            let half = full_rect.height() * 0.5;
            let top_r = Rect::from_min_max(full_rect.min, Pos2::new(full_rect.max.x, full_rect.min.y + half - 2.0));
            let bot_r = Rect::from_min_max(Pos2::new(full_rect.min.x, full_rect.min.y + half + 2.0), full_rect.max);
            let active_r = if tab.active_pane_secondary { bot_r } else { top_r };
            canvas.last_viewport_size = (active_r.width() as f64, active_r.height() as f64);
            draw_canvas_pane(
                ui, top_r, false, tab, canvas, active_tool, default_style, default_meas_opts,
                settings, &res, theme, actions, tile_requests,
            );
            ui.painter().line_segment(
                [Pos2::new(full_rect.min.x, full_rect.min.y + half), Pos2::new(full_rect.max.x, full_rect.min.y + half)],
                Stroke::new(2.0, theme.border_strong),
            );
            draw_canvas_pane(
                ui, bot_r, true, tab, canvas, active_tool, default_style, default_meas_opts,
                settings, &res, theme, actions, tile_requests,
            );
        }
    }

    // Bluebeam-style Sync Views: mirror pan & zoom from the modified pane to the sibling pane
    if tab.split_mode != SplitViewMode::None && tab.sync_split_views {
        let cur_cam = (tab.camera.zoom, tab.camera.pan);
        let cur_sec = (tab.secondary_camera.zoom, tab.secondary_camera.pan);
        if tab.camera.fit_page && tab.secondary_camera.fit_page {
            return;
        }
        if cur_cam != prev_cam {
            tab.secondary_camera.zoom = tab.camera.zoom;
            tab.secondary_camera.pan = tab.camera.pan;
            tab.secondary_camera.fit_page =
                tab.camera.fit_page && tab.camera.page_index == tab.secondary_camera.page_index;
        } else if cur_sec != prev_sec {
            tab.camera.zoom = tab.secondary_camera.zoom;
            tab.camera.pan = tab.secondary_camera.pan;
            tab.camera.fit_page =
                tab.secondary_camera.fit_page && tab.secondary_camera.page_index == tab.camera.page_index;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_canvas_pane(
    ui: &mut egui::Ui,
    pane_rect: Rect,
    is_secondary: bool,
    tab: &mut DocumentTab,
    canvas: &mut CanvasState,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
    res: &CanvasResources<'_>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
    tile_requests: &mut Vec<TileRequest>,
) {
    let mut pane_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(pane_rect)
            .id_salt(("canvas_pane", is_secondary)),
    );
    pane_ui.set_clip_rect(pane_rect);

    let resp = pane_ui.allocate_rect(pane_rect, Sense::click_and_drag());
    if !res.modal_open && (resp.clicked() || resp.drag_started()) && tab.active_pane_secondary != is_secondary {
        tab.active_pane_secondary = is_secondary;
        canvas.creation = None;
        canvas.drag = None;
    }

    let cam = if is_secondary {
        &mut tab.secondary_camera
    } else {
        &mut tab.camera
    };
    let page_idx = cam.page_index.min(tab.pages.len().saturating_sub(1));
    cam.page_index = page_idx;

    let Some(page) = tab.pages.get(page_idx).cloned() else {
        return;
    };

    let width = f64::from(pane_rect.width());
    let height = f64::from(pane_rect.height());
    if cam.fit_page && width > 0.0 && height > 0.0 {
        let margin = (width.min(height) * 0.04).min(20.0);
        let zx = (width - margin * 2.0) / page.width_pt.max(1.0);
        let zy = (height - margin * 2.0) / page.height_pt.max(1.0);
        cam.zoom = zx.min(zy).min(8.0);
        let drawn_w = page.width_pt * cam.zoom;
        let drawn_h = page.height_pt * cam.zoom;
        let pad_x = ((width - drawn_w) * 0.5) / cam.zoom;
        let pad_y = ((height - drawn_h) * 0.5) / cam.zoom;
        cam.pan = Point2::new(-pad_x, -pad_y);
    }

    // Handle smooth mouse wheel zoom anchored at cursor (60/120fps exponential ease)
    let pointer_pos = pane_ui.input(|i| i.pointer.hover_pos());
    let hovered = !res.modal_open && resp.contains_pointer();
    if hovered && canvas.inline_edit.is_none() {
        let scroll = pane_ui.input(|i| i.smooth_scroll_delta);
        if scroll.y.abs() > 0.1 {
            cam.fit_page = false;
            let anchor_screen = pointer_pos.unwrap_or(pane_rect.center());
            let before_xf = PageTransform {
                page_width_pt: page.width_pt,
                page_height_pt: page.height_pt,
                viewport_origin_x: pane_rect.left() as f64,
                viewport_origin_y: pane_rect.top() as f64,
                scale: cam.zoom,
                pan_x: cam.pan.x,
                pan_y: cam.pan.y,
            };
            let before_pt = before_xf.screen_to_page(anchor_screen.x as f64, anchor_screen.y as f64);
            let factor = (1.0 + scroll.y as f64 * 0.0022).clamp(0.72, 1.38);
            let base_target = canvas
                .smooth_zoom
                .as_ref()
                .and_then(|s| match s {
                    CameraAnim::WheelZoom {
                        is_secondary: sec,
                        target_zoom,
                        ..
                    }
                    | CameraAnim::GlideTo {
                        is_secondary: sec,
                        target_zoom,
                        ..
                    } if *sec == is_secondary => Some(*target_zoom),
                    _ => None,
                })
                .unwrap_or(cam.zoom);
            canvas.smooth_zoom = Some(CameraAnim::WheelZoom {
                is_secondary,
                target_zoom: (base_target * factor).clamp(0.05, 16.0),
                anchor_page_pt: before_pt,
                anchor_screen,
            });
            canvas.last_zoom_activity = Some(std::time::Instant::now());
        }
    }

    if let Some(anim) = canvas.smooth_zoom.clone() {
        let dt = pane_ui.input(|i| i.stable_dt).clamp(0.001, 0.05) as f64;
        let alpha = 1.0 - (-18.0 * dt).exp();
        match anim {
            CameraAnim::WheelZoom { is_secondary: sec, .. }
            | CameraAnim::GlideTo { is_secondary: sec, .. }
                if sec == is_secondary && cam.fit_page =>
            {
                canvas.smooth_zoom = None;
            }
            CameraAnim::WheelZoom {
                is_secondary: sec,
                target_zoom,
                anchor_screen,
                anchor_page_pt,
            } if sec == is_secondary => {
                let next_zoom = cam.zoom + (target_zoom - cam.zoom) * alpha;
                if (target_zoom - next_zoom).abs() < 0.0005 * next_zoom.max(0.1) {
                    cam.zoom = target_zoom;
                    canvas.smooth_zoom = None;
                } else {
                    cam.zoom = next_zoom;
                    pane_ui.ctx().request_repaint();
                    canvas.last_zoom_activity = Some(std::time::Instant::now());
                }
                cam.pan.x = anchor_page_pt.x
                    - (anchor_screen.x as f64 - pane_rect.left() as f64) / cam.zoom;
                cam.pan.y = anchor_page_pt.y
                    - (anchor_screen.y as f64 - pane_rect.top() as f64) / cam.zoom;
            }
            CameraAnim::GlideTo {
                is_secondary: sec,
                target_zoom,
                target_pan,
            } if sec == is_secondary => {
                let next_zoom = cam.zoom + (target_zoom - cam.zoom) * alpha;
                let next_pan = cam.pan.lerp(target_pan, alpha);
                let zoom_done = (target_zoom - next_zoom).abs() < 0.0005 * next_zoom.max(0.1);
                let pan_done = next_pan.dist(target_pan) < 0.5;
                if zoom_done && pan_done {
                    cam.zoom = target_zoom;
                    cam.pan = target_pan;
                    canvas.smooth_zoom = None;
                } else {
                    cam.zoom = next_zoom;
                    cam.pan = next_pan;
                    pane_ui.ctx().request_repaint();
                    canvas.last_zoom_activity = Some(std::time::Instant::now());
                }
            }
            _ => {}
        }
    }

    let xf = PageTransform {
        page_width_pt: page.width_pt,
        page_height_pt: page.height_pt,
        viewport_origin_x: pane_rect.left() as f64,
        viewport_origin_y: pane_rect.top() as f64,
        scale: cam.zoom,
        pan_x: cam.pan.x,
        pan_y: cam.pan.y,
    };

    // Paint canvas background & page shadow
    let painter = pane_ui.painter_at(pane_rect);
    painter.rect_filled(pane_rect, CornerRadius::ZERO, theme.canvas_bg);

    let (tlx, tly) = xf.page_to_screen(Point2::new(0.0, 0.0));
    let (brx, bry) = xf.page_to_screen(Point2::new(page.width_pt, page.height_pt));
    let page_screen_rect = Rect::from_min_max(Pos2::new(tlx, tly), Pos2::new(brx, bry));

    // Soft multi-layer elevation shadow + crisp white paper sheet
    let is_dark = theme.mode == crate::model::settings::ThemeMode::Dark;
    for (expand, dy, alpha) in [
        (7.0, 5.0, if is_dark { 22 } else { 10 }),
        (3.5, 3.0, if is_dark { 38 } else { 18 }),
        (1.0, 1.5, if is_dark { 58 } else { 28 }),
    ] {
        painter.rect_filled(
            page_screen_rect.expand(expand).translate(Vec2::new(0.0, dy)),
            CornerRadius::same(2),
            Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
        );
    }
    painter.rect_filled(page_screen_rect, CornerRadius::ZERO, Color32::WHITE);

    // Base page raster texture
    let base_tex = res.base_textures.get(&(tab.id, page_idx));
    let full_uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    if let Some(tex) = base_tex {
        // Smooth fade-in of the crisp base render over the low-res preview
        let fade = pane_ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique(("base_fade", tab.id, page_idx)), true, 0.18);
        if fade < 1.0
            && let Some(thumb) = res.thumb_textures.get(&(tab.id, page_idx))
        {
            painter.image(thumb.id(), page_screen_rect, full_uv, Color32::WHITE);
        }
        painter.image(tex.id(), page_screen_rect, full_uv, Color32::WHITE.gamma_multiply(fade));
    } else {
        // Instant low-res preview while the high-quality page renders in background
        if let Some(thumb) = res.thumb_textures.get(&(tab.id, page_idx)) {
            painter.image(thumb.id(), page_screen_rect, full_uv, Color32::WHITE);
        }
        let t = pane_ui.input(|i| i.time) as f32;
        let c = page_screen_rect.center();
        let r = 14.0;
        let start = t * 5.0;
        let pts: Vec<Pos2> = (0..=24)
            .map(|k| {
                let a = start + k as f32 / 24.0 * std::f32::consts::PI * 1.4;
                Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
            })
            .collect();
        painter.circle_filled(c, r + 8.0, Color32::from_rgba_unmultiplied(15, 23, 42, 150));
        painter.add(egui::Shape::line(pts, Stroke::new(3.0, theme.accent_red)));
        pane_ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }

    // High-resolution viewport tile when zoomed in deeply
    let pixels_per_point = cam.zoom * pane_ui.ctx().pixels_per_point() as f64;
    if pixels_per_point > 1.35 && pane_rect.intersects(page_screen_rect) {
        let vis_screen = pane_rect.intersect(page_screen_rect);
        let vis_min = xf.screen_to_page(vis_screen.left() as f64, vis_screen.top() as f64);
        let vis_max = xf.screen_to_page(vis_screen.right() as f64, vis_screen.bottom() as f64);
        let crop_rect = Rect2::from_min_max(
            Point2::new(vis_min.x.max(0.0), vis_min.y.max(0.0)),
            Point2::new(vis_max.x.min(page.width_pt), vis_max.y.min(page.height_pt)),
        );

        let padded = crop_rect.expand(crop_rect.width().max(crop_rect.height()) * 0.15);
        let clamped = Rect2::from_min_max(
            Point2::new(padded.min_x.max(0.0), padded.min_y.max(0.0)),
            Point2::new(padded.max_x.min(page.width_pt), padded.max_y.min(page.height_pt)),
        );
        let out_w = ((clamped.width() * pixels_per_point).round() as u32).clamp(64, 2200) as u16;
        let out_h = ((clamped.height() * pixels_per_point).round() as u32).clamp(64, 2200) as u16;
        // Compare against post-clamp effective resolution so HiDPI/large sheets never loop forever!
        let effective_ppp = (out_w as f64 / clamped.width().max(1.0))
            .min(out_h as f64 / clamped.height().max(1.0));

        let tile_key = (tab.id, page_idx, is_secondary);
        let mut need_tile = true;
        if let Some(tile) = res.tile_textures.get(&tile_key) {
            let covers = tile.crop_rect_pt.min_x <= crop_rect.min_x + 1.0
                && tile.crop_rect_pt.min_y <= crop_rect.min_y + 1.0
                && tile.crop_rect_pt.max_x >= crop_rect.max_x - 1.0
                && tile.crop_rect_pt.max_y >= crop_rect.max_y - 1.0;
            let res_ok =
                (tile.scale_px_per_pt - effective_ppp).abs() / effective_ppp.max(0.1) < 0.25;
            if covers && res_ok {
                need_tile = false;
            }
            if covers {
                let (ttl_x, ttl_y) = xf.page_to_screen(tile.crop_rect_pt.min());
                let (tbr_x, tbr_y) = xf.page_to_screen(tile.crop_rect_pt.max());
                painter.image(
                    tile.texture.id(),
                    Rect::from_min_max(Pos2::new(ttl_x, ttl_y), Pos2::new(tbr_x, tbr_y)),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        let already_in_flight = res.in_flight_tiles.get(&tile_key).is_some_and(|(r, w, h)| {
            (r.min_x - clamped.min_x).abs() < 2.0
                && (r.min_y - clamped.min_y).abs() < 2.0
                && (r.max_x - clamped.max_x).abs() < 2.0
                && (r.max_y - clamped.max_y).abs() < 2.0
                && *w == out_w
                && *h == out_h
        });
        let zoom_settled = canvas.smooth_zoom.is_none()
            && canvas
                .last_zoom_activity
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(90));
        if need_tile && !already_in_flight && crop_rect.width() > 2.0 && crop_rect.height() > 2.0 {
            if zoom_settled {
                tile_requests.push(TileRequest {
                    doc_id: tab.id,
                    page_index: page_idx,
                    is_secondary,
                    crop_rect_pt: clamped,
                    out_w,
                    out_h,
                });
            } else {
                pane_ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(95));
            }
        }
    }

    // Dimmer overlay (fades blueprint lines so markups stand out!)
    if settings.dimmer_enabled {
        let alpha = ((1.0 - settings.dimmer_opacity) * 200.0).clamp(20.0, 210.0) as u8;
        painter.rect_filled(
            page_screen_rect,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
        );
    }

    // Optional grid overlay
    let grid_step_pt = page.scale.meters_to_pt_avg(settings.grid_spacing_meters.max(0.05));
    if settings.show_grid && grid_step_pt * cam.zoom >= 8.0 {
        let gcol = Color32::from_rgba_unmultiplied(56, 189, 248, 45);
        let mut x = 0.0;
        while x <= page.width_pt {
            let (sx, _) = xf.page_to_screen(Point2::new(x, 0.0));
            painter.line_segment(
                [Pos2::new(sx, page_screen_rect.top()), Pos2::new(sx, page_screen_rect.bottom())],
                Stroke::new(1.0, gcol),
            );
            x += grid_step_pt;
        }
        let mut y = 0.0;
        while y <= page.height_pt {
            let (_, sy) = xf.page_to_screen(Point2::new(0.0, y));
            painter.line_segment(
                [Pos2::new(page_screen_rect.left(), sy), Pos2::new(page_screen_rect.right(), sy)],
                Stroke::new(1.0, gcol),
            );
            y += grid_step_pt;
        }
    }

    // Draw Viewport Scale boxes
    for vp in &page.viewports {
        let (x0, y0) = xf.page_to_screen(vp.rect.min());
        let (x1, y1) = xf.page_to_screen(vp.rect.max());
        let vr = Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1));
        painter.rect_filled(
            vr,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(37, 99, 235, 16),
        );
        draw_dashed_rect(&painter, vr, Stroke::new(1.3, Color32::from_rgb(37, 99, 235)), 8.0, 5.0);
        let badge = format!("Viewport: {} ({})", vp.name, vp.scale.label());
        painter.text(
            Pos2::new(vr.left() + 6.0, vr.top() + 4.0),
            egui::Align2::LEFT_TOP,
            badge,
            FontId::proportional(11.0),
            Color32::from_rgb(37, 99, 235),
        );
    }

    // Draw Spaces / Recintos (Bluebeam-style polygonal spatial zones)
    if tab.highlight_spaces {
        for sp in &tab.spaces {
            if sp.page_index != page_idx || sp.points.len() < 3 {
                continue;
            }
            let c = sp.fill_color;
            let fill_a = (sp.fill_opacity * 255.0).clamp(14.0, 140.0) as u8;
            let fill_c = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], fill_a);
            let border_c = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 195);
            let tris = triangulate_polygon_with_holes(&sp.points, &[]);
            for [a, b, d] in tris {
                let (ax, ay) = xf.page_to_screen(a);
                let (bx, by) = xf.page_to_screen(b);
                let (dx, dy) = xf.page_to_screen(d);
                painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(ax, ay), Pos2::new(bx, by), Pos2::new(dx, dy)],
                    fill_c,
                    Stroke::NONE,
                ));
            }
            let spts: Vec<Pos2> = sp
                .points
                .iter()
                .map(|pt| {
                    let (sx, sy) = xf.page_to_screen(*pt);
                    Pos2::new(sx, sy)
                })
                .collect();
            for i in 0..spts.len() {
                let a = spts[i];
                let b = spts[(i + 1) % spts.len()];
                draw_dashed_segment(&painter, a, b, Stroke::new(1.4, border_c), 7.0, 4.0);
            }
            let full_title = tab.space_full_title(sp.id);
            let cent = polygon_centroid(&sp.points);
            let (cx, cy) = xf.page_to_screen(cent);
            let tag_w = (full_title.chars().count() as f32 * 6.5 + 14.0).clamp(64.0, 260.0);
            let tag_r = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(tag_w, 18.0));
            painter.rect_filled(
                tag_r,
                CornerRadius::same(4),
                Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 210),
            );
            painter.text(
                tag_r.center(),
                egui::Align2::CENTER_CENTER,
                full_title,
                FontId::proportional(10.5),
                Color32::WHITE,
            );
        }
    }

    // Page border outline
    painter.rect_stroke(
        page_screen_rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, Color32::from_rgb(140, 148, 160)),
        egui::StrokeKind::Outside,
    );

    // Draw all visible markups for this page (compute Legend summary lazily only if needed!)
    let has_legend = tab.markups.iter().any(|m| {
        m.page_index == page_idx
            && matches!(m.kind, MarkupKind::Legend { .. })
            && tab.is_layer_visible(&m.layer)
    });
    let doc_totals = if has_legend {
        tab.compute_totals(Some(page_idx))
    } else {
        crate::model::document::DocumentTotals::default()
    };
    let legend_rows = if has_legend {
        build_legend_summary(tab, page_idx)
    } else {
        Vec::new()
    };
    let is_active_pane = tab.split_mode == SplitViewMode::None || tab.active_pane_secondary == is_secondary;
    for m in &tab.markups {
        if m.page_index != page_idx || !tab.is_layer_visible(&m.layer) {
            continue;
        }
        let is_sel = tab.selected_ids.contains(&m.id);
        draw_markup_on_canvas(
            &painter,
            xf,
            m,
            &page.scale,
            &page.viewports,
            is_sel,
            &legend_rows,
            &doc_totals,
            res.image_textures,
            theme,
            canvas.inline_edit.as_ref().filter(|edit| is_active_pane && edit.id == m.id),
        );
    }

    // Subtle live coordinate HUD pill in bottom-right corner when drawing or measuring
    if hovered
        && *active_tool != ActiveTool::Select
        && *active_tool != ActiveTool::Pan
        && let Some(hp) = canvas.hover_page_pt
        && hp.x >= 0.0
        && hp.y >= 0.0
        && hp.x <= page.width_pt
        && hp.y <= page.height_pt
    {
        let sc = resolve_scale_at(&page.scale, &page.viewports, hp);
        let xm = hp.x * sc.pt_to_meters_x();
        let ym = hp.y * sc.pt_to_meters_y();
        let txt = format!(
            "X: {}  ·  Y: {}",
            sc.format_length_val(xm),
            sc.format_length_val(ym)
        );
        let pill_w = (txt.chars().count() as f32 * 6.5 + 18.0).max(120.0);
        let pill_r = Rect::from_min_size(
            Pos2::new(pane_rect.right() - pill_w - 10.0, pane_rect.bottom() - 28.0),
            Vec2::new(pill_w, 20.0),
        );
        painter.rect_filled(
            pill_r,
            CornerRadius::same(5),
            Color32::from_rgba_unmultiplied(15, 23, 42, 205),
        );
        painter.rect_stroke(
            pill_r,
            CornerRadius::same(5),
            Stroke::new(1.0, theme.border_strong),
            egui::StrokeKind::Inside,
        );
        painter.text(
            pill_r.center(),
            egui::Align2::CENTER_CENTER,
            txt,
            FontId::monospace(10.5),
            Color32::from_rgb(226, 232, 240),
        );
    }

    // Draw active pane outline in Split mode
    if tab.split_mode != SplitViewMode::None && tab.active_pane_secondary == is_secondary {
        painter.rect_stroke(
            pane_rect.shrink(1.0),
            CornerRadius::ZERO,
            Stroke::new(2.0, theme.accent_red),
            egui::StrokeKind::Inside,
        );
    }

    // Only process editing interactions in active pane (skip while modal or inline editor is active)
    if is_active_pane && !res.modal_open && canvas.inline_edit.is_none() {
        if active_tool.is_measurement() && !page.scale.is_set {
            draw_uncalibrated_scale_banner(pane_ui.ctx(), pane_rect, theme, actions);
        }
        handle_canvas_input(
            &mut pane_ui,
            pane_rect,
            &resp,
            is_secondary,
            tab,
            canvas,
            active_tool,
            default_style,
            default_meas_opts,
            settings,
            res.snap_indices.get(&(tab.id, page_idx)),
            base_tex,
            grid_step_pt,
            xf,
            theme,
            actions,
        );
    }

    if is_active_pane && !res.modal_open {
        if canvas.inline_edit.is_none() && canvas.drag.is_none() && canvas.creation.is_none() {
            draw_selected_arrow_style(
                pane_ui.ctx(), pane_rect, xf, page_idx, tab, theme, actions,
            );
        }
        draw_inline_text_editor(&mut pane_ui, pane_rect, xf, page_idx, tab, canvas, theme);
    }

    if hovered && canvas.inline_edit.is_none() && canvas.drag.is_none() && canvas.creation.is_none()
        && let Some(pointer) = pointer_pos
        && !pane_ui.input(|i| i.pointer.any_down())
    {
        draw_note_preview(&resp, tab, xf, page_idx, pointer);
    }
}

fn note_pin_rect(xf: PageTransform, pos: Point2) -> Rect {
    let (x, y) = xf.page_to_screen(pos);
    Rect::from_center_size(Pos2::new(x, y), Vec2::splat(22.0))
}

fn draw_note_preview(
    canvas: &egui::Response,
    tab: &DocumentTab,
    xf: PageTransform,
    page_index: usize,
    pointer: Pos2,
) {
    for note in tab.markups.iter().rev() {
        let MarkupKind::Note { pos } = note.kind else { continue };
        if note.page_index != page_index || !tab.is_layer_visible(&note.layer) {
            continue;
        }
        let pin = note_pin_rect(xf, pos).intersect(canvas.interact_rect);
        if pin.contains(pointer) {
            let text = note.comment.trim();
            if !text.is_empty() {
                egui::Tooltip::always_open(
                    canvas.ctx.clone(), canvas.layer_id, canvas.id.with(("note_preview", note.id)), pin,
                )
                .width(280.0)
                .show(|ui| ui.add(egui::Label::new(text).wrap()));
            }
            break;
        }
    }
}

fn draw_selected_arrow_style(
    ctx: &egui::Context,
    pane: Rect,
    xf: PageTransform,
    page_index: usize,
    tab: &mut DocumentTab,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let [id] = tab.selected_ids.as_slice() else {
        return;
    };
    let Some(index) = tab.markups.iter().position(|m| {
        m.id == *id
            && m.page_index == page_index
            && matches!(m.kind, MarkupKind::Arrow { .. })
            && tab.is_layer_visible(&m.layer)
    }) else {
        return;
    };
    let arrow = &tab.markups[index];
    let layer_locked = tab.layers.iter().any(|layer| layer.name == arrow.layer && layer.locked);
    let locked = tab.is_markup_locked(arrow);
    let [r, g, b, _] = arrow.style.stroke_color;
    let mut rgb = [r, g, b];
    let bounds = screen_rect(xf, markup_bounds(arrow)).expand(4.0).intersect(pane);
    if !bounds.is_positive() || pane.width() < 166.0 {
        return;
    }

    let above = bounds.top() - pane.top() >= 46.0;
    let anchor_y = if above { bounds.top() - 5.0 } else { bounds.bottom() + 5.0 };
    let anchor = egui::pos2(bounds.center().x, anchor_y);
    let mut color_changed = false;
    egui::Area::new(egui::Id::unique(("selected_arrow_style", tab.id, *id)))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(anchor)
        .pivot(if above { egui::Align2::CENTER_BOTTOM } else { egui::Align2::CENTER_TOP })
        .constrain_to(pane.shrink(5.0))
        .default_width(152.0)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme.panel_bg)
                .inner_margin(egui::Margin::symmetric(5, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        color_changed = ui.add_enabled_ui(!locked, |ui| {
                            ui.spacing_mut().interact_size = egui::vec2(27.0, 26.0);
                            ui.color_edit_button_srgb(&mut rgb)
                                .on_hover_text("Color de la flecha")
                                .changed()
                        }).inner;
                        for (icon, label, enabled, action) in [
                            (IconKind::Duplicate, "Duplicar", !locked, UiAction::DuplicateSelected),
                            (IconKind::Lock, if locked { "Desbloquear" } else { "Bloquear" }, !layer_locked, UiAction::ToggleLockSelected),
                            (IconKind::Properties, "Propiedades de la flecha", true, UiAction::ShowMarkupProperties),
                            (IconKind::Trash, "Eliminar", !locked, UiAction::DeleteSelected),
                        ] {
                            if icon_button(ui, 26.0, icon, locked && icon == IconKind::Lock, enabled, label, theme).clicked() {
                                actions.push(action);
                            }
                        }
                    });
                });
        });
    if color_changed {
        tab.push_undo();
        let markup = &mut tab.markups[index];
        markup.style.stroke_color = [rgb[0], rgb[1], rgb[2], 255];
        markup.style.text_color = markup.style.stroke_color;
        markup.modified_at = chrono::Utc::now();
        actions.push(UiAction::MarkupsStyleChanged);
    }
}

fn draw_inline_text_editor(
    ui: &mut egui::Ui,
    pane_rect: Rect,
    xf: PageTransform,
    page_idx: usize,
    tab: &mut DocumentTab,
    canvas: &mut CanvasState,
    theme: &RedBimTheme,
) {
    let Some(edit) = &mut canvas.inline_edit else {
        return;
    };
    let Some(idx) = tab.markups.iter().position(|m| m.id == edit.id) else {
        canvas.inline_edit = None;
        return;
    };
    let m = &tab.markups[idx];
    if tab.is_markup_locked(m) {
        canvas.inline_edit = None;
        return;
    }
    let mut cancel = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let commit_key = ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
    let mut commit = commit_key;

    let on_current_page = m.page_index == page_idx && tab.is_layer_visible(&m.layer);
    if !cancel && on_current_page && let MarkupKind::Note { pos } = m.kind {
        let (x, y) = xf.page_to_screen(pos);
        (commit, cancel) = draw_note_editor(ui, pane_rect, egui::pos2(x, y), edit, theme);
        commit |= commit_key;
    } else if !cancel && !commit && on_current_page {
        let (rect, font_px, color, single_line) =
            inline_text_geometry(m, edit, xf, pane_rect, theme);
        let visible = pane_rect.intersect(rect);
        if visible.is_positive() {
            let focus_on_open = std::mem::take(&mut edit.needs_focus);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("markup_text", edit.id))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |ui| {
                    ui.set_clip_rect(visible);
                    if m.text_content().is_none() {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(3), theme.panel_bg);
                    }
                    egui::ScrollArea::vertical()
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .max_height(rect.height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let widget = if single_line {
                                egui::TextEdit::singleline(&mut edit.text).return_key(None)
                            } else {
                                egui::TextEdit::multiline(&mut edit.text).desired_rows(1)
                            };
                            let output = widget
                                .font(FontId::proportional(font_px))
                                .text_color(color)
                                .frame(egui::Frame::NONE)
                                .margin(egui::Margin::ZERO)
                                .desired_width(rect.width())
                                .show(ui);
                            if focus_on_open {
                                output.response.request_focus();
                            }
                            edit.changed |= output.response.changed();
                            if let Some(cursor) = output.cursor_range
                                && (focus_on_open
                                    || output.response.changed()
                                    || ui.input(|i| {
                                        i.events.iter().any(|e| {
                                            matches!(e, egui::Event::Key { pressed: true, .. })
                                        })
                                    }))
                            {
                                let caret = output
                                    .galley
                                    .pos_from_cursor(cursor.primary)
                                    .translate(output.galley_pos.to_vec2());
                                ui.scroll_to_rect(caret, None);
                            }
                        });
                },
            );
            commit = !focus_on_open
                && ui.input(|i| {
                    i.pointer.any_pressed()
                        && i.pointer
                            .interact_pos()
                            .is_some_and(|pos| !visible.contains(pos))
                });
        } else {
            commit = true;
        }
    } else if !on_current_page {
        commit = true;
    }

    if cancel {
        canvas.inline_edit = None;
    } else if commit {
        canvas.commit_inline_edit(tab, ui.ctx());
    }
}

fn draw_note_editor(
    ui: &mut egui::Ui,
    pane: Rect,
    pin: Pos2,
    edit: &mut InlineTextEdit,
    theme: &RedBimTheme,
) -> (bool, bool) {
    let width = (pane.width() - 12.0).min(200.0);
    if width <= 0.0 || !pane.expand(11.0).contains(pin) {
        return (true, false);
    }
    let above = pane.bottom() - pin.y < 96.0;
    let anchor = pin + egui::vec2(14.0, if above { -12.0 } else { 12.0 });
    let opening = edit.needs_focus;
    let field_id = egui::Id::unique(("note_text", edit.id));
    let response = egui::Area::new(egui::Id::unique(("note_editor", edit.id)))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(anchor)
        .pivot(if above { egui::Align2::LEFT_BOTTOM } else { egui::Align2::LEFT_TOP })
        .constrain_to(pane.shrink(5.0))
        .default_width(width)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme.panel_bg)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(width - 14.0);
                    // The release of the click that placed the pin can take focus from the field.
                    let released = ui.input(|i| i.pointer.any_released());
                    let focus = edit.needs_focus
                        && !ui.is_sizing_pass()
                        && !released
                        && ui.input(|i| !i.pointer.primary_down());
                    if edit.needs_focus && released {
                        ui.ctx().request_repaint();
                    }
                    if focus {
                        ui.memory_mut(|mem| mem.request_focus(field_id));
                    }
                    let field = egui::ScrollArea::vertical()
                        .max_height(68.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut edit.text)
                                    .id(field_id)
                                    .hint_text("Escribe una nota…")
                                    .text_color(theme.text_primary)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            )
                        })
                        .inner;
                    if focus {
                        field.request_focus();
                        edit.needs_focus = false;
                    }
                    edit.changed |= field.changed();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save = ui.button("Guardar").on_hover_text("Ctrl+Enter").clicked();
                        let cancel = ui.button("Cancelar").clicked();
                        (save, cancel)
                    }).inner
                }).inner
        });
    let outside = !opening && ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|pos| !response.response.rect.contains(pos))
    });
    let (save, cancel) = response.inner;
    (save || outside, cancel)
}

#[cfg(test)]
mod markup_overlay_tests {
    use super::*;
    use crate::model::document::PageModel;
    use crate::model::settings::ThemeMode;

    fn fixture(kind: MarkupKind) -> (DocumentTab, PageTransform, Rect) {
        let mut tab = DocumentTab::new_with_pages(
            "Prueba",
            None,
            Vec::new(),
            vec![PageModel::new(0, 600.0, 400.0)],
        );
        let markup = Markup::new(0, "Marca", "Autor", MarkupStyle::default(), kind);
        tab.select_only(markup.id);
        tab.markups.push(markup);
        let xf = PageTransform {
            page_width_pt: 600.0,
            page_height_pt: 400.0,
            viewport_origin_x: 0.0,
            viewport_origin_y: 0.0,
            scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        (tab, xf, Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0)))
    }

    #[test]
    fn callout_click_stays_put_and_corner_drag_keeps_leader_and_text() {
        let tip = Point2::new(50.0, 190.0);
        let knee = Point2::new(115.0, 129.0);
        let original = Rect2::from_xywh(130.0, 100.0, 165.0, 36.0);
        let (mut tab, xf, pane) = fixture(MarkupKind::Callout {
            arrow_tip: tip,
            knee,
            box_rect: original,
            text: "Detalle".into(),
        });
        let settings = AppSettings::default();
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        let ctx = egui::Context::default();
        let mut canvas = CanvasState::default();
        let mut tool = ActiveTool::Select;
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let grab = Pos2::new(292.0, 134.0);
        let mut frame = |tab: &mut DocumentTab, events| {
            let mut output = ctx.run_ui(egui::RawInput { screen_rect: Some(pane), events, ..Default::default() }, |ui| {
                let response = ui.allocate_rect(pane, Sense::click_and_drag());
                handle_canvas_input(
                    ui, pane, &response, false, tab, &mut canvas, &mut tool,
                    &MarkupStyle::default(), &MeasurementOptions::default(), &settings,
                    None, None, 0.0, xf, &theme, &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
        };
        frame(&mut tab, vec![egui::Event::PointerMoved(grab)]);
        frame(&mut tab, vec![button(grab, true)]);
        frame(&mut tab, vec![]);
        assert!(matches!(tab.markups[0].kind, MarkupKind::Callout { box_rect, .. } if box_rect == original));
        frame(&mut tab, vec![button(grab, false)]);
        let target = Pos2::new(126.0, 97.0);
        frame(&mut tab, vec![button(grab, true), egui::Event::PointerMoved(target)]);
        frame(&mut tab, vec![button(target, false)]);
        assert!(matches!(tab.markups[0].kind, MarkupKind::Callout { arrow_tip, knee: elbow, box_rect, .. }
            if arrow_tip == tip && elbow == knee && box_rect.min() == original.min()
                && box_rect.width() == 54.0 && box_rect.max_y >= knee.y), "{:?}", tab.markups[0].kind);

        let text = "Verificar fijaciones del perfil\nRevisar el encuentro con la cubierta";
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let mut edit = InlineTextEdit::new(tab.markups[0].id, text.into());
            edit.changed = true;
            canvas.inline_edit = Some(edit);
            canvas.commit_inline_edit(&mut tab, ui.ctx());
        });
        output.textures_delta.clear();
        let MarkupKind::Callout { arrow_tip, knee: elbow, box_rect, text: saved } = &tab.markups[0].kind else {
            panic!("expected a callout");
        };
        assert_eq!((*arrow_tip, *elbow, box_rect.min()), (tip, knee, original.min()));
        assert_eq!(callout_landing(*box_rect, *elbow), (knee, Point2::new(original.min_x, knee.y)));
        assert!(box_rect.height() > original.height());
        assert_eq!(saved, text);
        tab.raw_pdf_bytes = crate::pdf::demo_blueprint::create_blank_pdf_bytes(600.0, 400.0, 1).into();
        let bytes = crate::pdf::io::save_pdf_to_bytes(&tab).unwrap();
        let reopened = crate::pdf::io::open_pdf_from_bytes("detalle.pdf", None, bytes).unwrap();
        assert!(matches!(&reopened.markups[0].kind, MarkupKind::Callout { knee: restored_knee, box_rect: restored, text: restored_text, .. }
            if *restored_knee == knee && restored == box_rect && restored_text == text));
    }

    #[test]
    fn note_preview_shows_only_over_the_visible_pin() {
        let (mut tab, xf, pane) = fixture(MarkupKind::Note { pos: Point2::new(100.0, 100.0) });
        tab.markups[0].comment = "Revisar fijaciones".into();
        let ctx = egui::Context::default();
        let preview_is_visible = |pointer, tab: &DocumentTab| {
            let mut output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(pane), ..Default::default() },
                |ui| {
                    let response = ui.allocate_rect(pane, Sense::click_and_drag());
                    draw_note_preview(&response, tab, xf, 0, pointer);
                },
            );
            output.textures_delta.clear();
            output.shapes.iter().any(|entry| {
                matches!(&entry.shape, egui::Shape::Text(text) if text.galley.text() == "Revisar fijaciones")
            })
        };
        let pin = egui::pos2(100.0, 100.0);
        preview_is_visible(pin, &tab);
        assert!(preview_is_visible(pin, &tab));
        assert!(!preview_is_visible(egui::pos2(112.0, 100.0), &tab));
    }

    #[test]
    fn note_editor_shows_a_prompt_and_saves_typed_text() {
        let (mut tab, xf, pane) = fixture(MarkupKind::Note { pos: Point2::new(480.0, 360.0) });
        tab.raw_pdf_bytes = crate::pdf::demo_blueprint::create_blank_pdf_bytes(600.0, 400.0, 1).into();
        let mut canvas = CanvasState {
            inline_edit: Some(InlineTextEdit::new(tab.markups[0].id, String::new())),
            ..Default::default()
        };
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        let ctx = egui::Context::default();
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(pane), events, ..Default::default() },
                |ui| draw_inline_text_editor(ui, pane, xf, 0, &mut tab, &mut canvas, &theme),
            );
            output.textures_delta.clear();
            output.shapes
        };
        let pin = egui::pos2(480.0, 360.0);
        for pressed in [true, false] {
            frame(vec![egui::Event::PointerButton {
                pos: pin, button: egui::PointerButton::Primary, pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        let shapes = frame(vec![]);
        assert!(shapes.iter().any(|entry| matches!(&entry.shape, egui::Shape::Text(text) if text.galley.text() == "Escribe una nota…")));
        frame(vec![egui::Event::Text("Revisar fijaciones".into())]);
        let shapes = frame(vec![]);
        let save_pos = shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Text(text) if text.galley.text() == "Guardar" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        }).unwrap();
        let mut click = vec![egui::Event::PointerMoved(save_pos)];
        for pressed in [true, false] {
            click.push(egui::Event::PointerButton {
                pos: save_pos, button: egui::PointerButton::Primary, pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        frame(click);
        assert!(canvas.inline_edit.is_none());
        assert_eq!(tab.markups[0].comment, "Revisar fijaciones");
        let bytes = crate::pdf::io::save_pdf_to_bytes(&tab).unwrap();
        let reopened = crate::pdf::io::open_pdf_from_bytes("prueba-nota.pdf", None, bytes).unwrap();
        assert_eq!(reopened.markups[0].comment, "Revisar fijaciones");
    }

    #[test]
    fn arrow_toolbar_is_compact_and_opens_the_color_picker() {
        let (mut tab, xf, pane) = fixture(MarkupKind::Arrow {
            start: Point2::new(200.0, 204.0),
            end: Point2::new(300.0, 250.0),
        });
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        let ctx = egui::Context::default();
        let [r, g, b, _] = tab.markups[0].style.stroke_color;
        let color = Color32::from_rgb(r, g, b);
        let id = egui::Id::unique(("selected_arrow_style", tab.id, tab.markups[0].id));
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(pane), events, ..Default::default() },
                |_| draw_selected_arrow_style(
                    &ctx, pane, xf, 0, &mut tab, &theme, &mut Vec::new(),
                ),
            );
            output.textures_delta.clear();
            output.shapes
        };
        frame(vec![]);
        let shapes = frame(vec![]);
        let bar = ctx.memory(|mem| mem.area_rect(id)).unwrap();
        assert!(bar.width() <= 165.0 && bar.height() <= 40.0);
        assert!(pane.contains_rect(bar) && bar.bottom() < 204.0);
        let swatch = shapes.iter().find_map(|entry| match &entry.shape {
            egui::Shape::Rect(shape) if shape.fill == color && bar.contains_rect(shape.rect) => {
                Some(shape.rect.center())
            }
            _ => None,
        }).unwrap();
        assert!(pane.contains(swatch) && swatch.y < 204.0);
        frame(vec![egui::Event::PointerMoved(swatch)]);
        for pressed in [true, false] {
            frame(vec![egui::Event::PointerButton {
                pos: swatch, button: egui::PointerButton::Primary, pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        assert!(egui::Popup::is_any_open(&ctx));
    }
}

fn inline_text_geometry(
    m: &Markup,
    edit: &InlineTextEdit,
    xf: PageTransform,
    pane: Rect,
    theme: &RedBimTheme,
) -> (Rect, f32, Color32, bool) {
    let font_px = (m.style.font_size * xf.scale as f32).clamp(6.0, 48.0);
    let [r, g, b, _] = m.style.text_color;
    let color = Color32::from_rgb(r, g, b);
    match &m.kind {
        MarkupKind::Callout { box_rect, .. } => (
            callout_text_rect(screen_rect(xf, *box_rect), xf.scale as f32),
            m.style.font_size * xf.scale as f32,
            color,
            false,
        ),
        MarkupKind::TextBox { rect, .. } | MarkupKind::CloudPlus { box_rect: rect, .. } => {
            let inner = screen_rect(xf, *rect).shrink2(Vec2::new(5.0, 3.0));
            (
                inner,
                font_px.clamp(6.5, (inner.height() * 0.55).max(7.0)),
                color,
                false,
            )
        }
        MarkupKind::Typewriter { pos, .. } => {
            let (x, y) = xf.page_to_screen(*pos);
            let bounds = screen_rect(xf, markup_bounds(m));
            let width = bounds.width().max(120.0).min((pane.right() - x).max(1.0));
            let height = (edit.text.split('\n').count() as f32 + 2.0) * font_px * 1.4;
            (
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height)),
                font_px,
                color,
                false,
            )
        }
        MarkupKind::Stamp { rect, .. } | MarkupKind::Redaction { rect, .. } => {
            let bounds = screen_rect(xf, *rect);
            let (center_y, size, ink) = match &m.kind {
                MarkupKind::Stamp { .. } if edit.subtitle => {
                    (0.72, (bounds.height() * 0.20).clamp(5.5, 14.0), color)
                }
                MarkupKind::Stamp { .. } => {
                    (0.35, (bounds.height() * 0.34).clamp(7.0, 28.0), color)
                }
                _ => (
                    0.5,
                    (bounds.height() * 0.34).clamp(8.5, 16.0),
                    Color32::WHITE,
                ),
            };
            let center = egui::pos2(bounds.center().x, bounds.top() + bounds.height() * center_y);
            let field = Rect::from_center_size(center, Vec2::new(bounds.width() - 6.0, size * 1.3));
            (field.intersect(bounds), size, ink, true)
        }
        MarkupKind::Legend { pos, width_pt, .. } => {
            let (x, y) = xf.page_to_screen(*pos);
            let rect = Rect::from_min_size(
                egui::pos2(x + 8.0, y + 4.0),
                Vec2::new((*width_pt * xf.scale).max(200.0) as f32 - 16.0, 17.0),
            );
            (rect, 12.0, Color32::WHITE, true)
        }
        MarkupKind::Hyperlink { .. } => {
            let (rect, _) = hyperlink_badge(m, xf).unwrap();
            (
                rect.shrink2(Vec2::new(4.0, 1.0)),
                10.0,
                Color32::WHITE,
                true,
            )
        }
        _ => {
            let bounds = screen_rect(xf, markup_bounds(m));
            let width = pane.width().min(220.0);
            let pos = egui::pos2(
                bounds.left().clamp(pane.left(), pane.right() - width),
                (bounds.top() + 12.0).clamp(pane.top(), (pane.bottom() - 48.0).max(pane.top())),
            );
            (
                Rect::from_min_size(pos, Vec2::new(width, 48.0)),
                13.0,
                theme.text_primary,
                false,
            )
        }
    }
}

fn hyperlink_badge(m: &Markup, xf: PageTransform) -> Option<(Rect, String)> {
    let MarkupKind::Hyperlink {
        rect,
        target,
        description,
    } = &m.kind
    else {
        return None;
    };
    let text = if !description.trim().is_empty() {
        format!("🔗 {}", description.trim())
    } else {
        match target {
            HyperlinkTarget::Page { page_index } => format!("🔗 Ir a Pág. {}", page_index + 1),
            HyperlinkTarget::Url(url) => format!("🔗 {url}"),
        }
    };
    let bounds = screen_rect(xf, *rect);
    let width = (text.chars().count() as f32 * 6.0 + 12.0).clamp(60.0, 220.0);
    let pos = egui::pos2(bounds.left(), (bounds.top() - 18.0).max(0.0));
    Some((Rect::from_min_size(pos, Vec2::new(width, 17.0)), text))
}

#[allow(clippy::too_many_arguments)]
fn handle_canvas_input(
    ui: &mut egui::Ui,
    pane_rect: Rect,
    resp: &egui::Response,
    is_secondary: bool,
    tab: &mut DocumentTab,
    canvas: &mut CanvasState,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
    snap_index: Option<&SnapIndex>,
    base_tex: Option<&TextureHandle>,
    grid_step_pt: f64,
    xf: PageTransform,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let page_idx = xf.page_height_pt;
    let _ = page_idx;
    let cam_page_idx = if is_secondary {
        tab.secondary_camera.page_index
    } else {
        tab.camera.page_index
    };
    let Some(page) = tab.pages.get(cam_page_idx).cloned() else {
        return;
    };

    let space_down = ui.input(|i| i.key_down(egui::Key::Space));
    let shift_down = ui.input(|i| i.modifiers.shift);
    let ctrl_down = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
    let alt_down = ui.input(|i| i.modifiers.alt);
    let middle_down = ui.input(|i| i.pointer.middle_down());
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_pressed = resp.hovered() && ui.input(|i| i.pointer.primary_pressed());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let double_clicked = resp.double_clicked();
    let secondary_clicked = resp.secondary_clicked();

    let panning = matches!(canvas.drag, Some(DragMode::Pan { .. }))
        && (primary_down || middle_down);
    if resp.hovered() || resp.dragged() || panning {
        let cursor = if panning || middle_down {
            CursorIcon::Grabbing
        } else if space_down || *active_tool == ActiveTool::Pan {
            CursorIcon::Grab
        } else {
            match *active_tool {
                ActiveTool::Select => CursorIcon::Default,
                ActiveTool::ZoomWindow => CursorIcon::ZoomIn,
                ActiveTool::TextBox | ActiveTool::Typewriter => CursorIcon::Text,
                ActiveTool::Note | ActiveTool::Stamp | ActiveTool::Image | ActiveTool::Legend => {
                    CursorIcon::Copy
                }
                _ => CursorIcon::Crosshair,
            }
        };
        ui.ctx().set_cursor_icon(cursor);
    }

    // Pan with Middle mouse, Space+drag, or Pan tool
    if (middle_down || (space_down && primary_down) || (*active_tool == ActiveTool::Pan && primary_down))
        && let Some(pos) = pointer_pos
    {
        match &mut canvas.drag {
            Some(DragMode::Pan { last_screen }) => {
                let delta = pos - *last_screen;
                *last_screen = pos;
                let cam = if is_secondary {
                    &mut tab.secondary_camera
                } else {
                    &mut tab.camera
                };
                cam.pan.x -= delta.x as f64 / cam.zoom;
                cam.pan.y -= delta.y as f64 / cam.zoom;
                cam.fit_page = false;
                return;
            }
            None if resp.hovered() || resp.dragged() => {
                canvas.smooth_zoom = None;
                canvas.drag = Some(DragMode::Pan { last_screen: pos });
                return;
            }
            _ => {}
        }
    }
    if matches!(canvas.drag, Some(DragMode::Pan { .. })) && !primary_down && !middle_down {
        canvas.drag = None;
    }

    // Clear active pointer drags cleanly if mouse button was released outside the window
    if !primary_down
        && matches!(
            canvas.drag,
            Some(DragMode::MoveHandle { .. } | DragMode::MoveSelected { .. })
        )
        && !(primary_released && matches!(canvas.drag, Some(DragMode::MoveHandle {
            callout_grab_offset: Some(_), pushed_undo: true, ..
        })))
    {
        canvas.drag = None;
    }

    // Compute current cursor page coordinate + snap
    let Some(screen_pos) = pointer_pos else {
        if !primary_down {
            canvas.drag = None;
        }
        canvas.hover_page_pt = None;
        canvas.hover_snap = None;
        return;
    };
    let in_pane = pane_rect.contains(screen_pos);
    let raw_pt = xf.screen_to_page(screen_pos.x as f64, screen_pos.y as f64);
    let tol_pt = 10.0 / xf.scale.max(0.05);

    let dragging_point_handle = match &canvas.drag {
        Some(DragMode::MoveHandle {
            markup_id, handle, callout_grab_offset, pushed_undo,
        }) if *handle != HandleKind::MeasureOffset
            && !alt_down
            && (callout_grab_offset.is_none()
                || (*handle == HandleKind::CalloutTip && (*pushed_undo || resp.dragged()))) => Some(*markup_id),
        _ => None,
    };

    let snapping_creation = *active_tool != ActiveTool::Select
        && *active_tool != ActiveTool::Pan
        && !matches!(canvas.creation, Some(CreationState::Callout { .. }));
    let snap_res = if snapping_creation || dragging_point_handle.is_some() {
        find_best_snap_excluding(
            raw_pt,
            tol_pt,
            &settings.snap,
            snap_index,
            &tab.markups,
            cam_page_idx,
            grid_step_pt,
            dragging_point_handle,
        )
    } else {
        None
    };
    let mut pt = snap_res.map(|s| s.point).unwrap_or(raw_pt);
    canvas.hover_page_pt = Some(pt);
    canvas.hover_snap = snap_res;

    // Paint Snap indicator + Magnifier Loupe during Calibrate, Measurement tools, or Handle editing
    if in_pane
        && ((*active_tool != ActiveTool::Select && *active_tool != ActiveTool::Pan)
            || dragging_point_handle.is_some())
    {
        let overlay_painter = ui.painter_at(pane_rect);
        if let Some(sr) = snap_res {
            let (sx, sy) = xf.page_to_screen(sr.point);
            let sp = Pos2::new(sx, sy);
            let scol = theme.accent_amber;
            match sr.kind {
                SnapKind::Endpoint => {
                    overlay_painter.rect_stroke(
                        Rect::from_center_size(sp, Vec2::splat(9.0)),
                        CornerRadius::ZERO,
                        Stroke::new(1.8, scol),
                        egui::StrokeKind::Inside,
                    );
                }
                SnapKind::Midpoint => {
                    let r = 5.5;
                    overlay_painter.add(egui::Shape::closed_line(
                        vec![
                            Pos2::new(sp.x, sp.y - r),
                            Pos2::new(sp.x + r, sp.y + r),
                            Pos2::new(sp.x - r, sp.y + r),
                        ],
                        Stroke::new(1.8, scol),
                    ));
                }
                _ => {
                    overlay_painter.circle_stroke(sp, 5.0, Stroke::new(1.8, scol));
                }
            }
        }
        if (dragging_point_handle.is_some()
            || (matches!(
                *active_tool,
                ActiveTool::Calibrate
                    | ActiveTool::MeasureLength
                    | ActiveTool::MeasurePolylength
                    | ActiveTool::MeasurePerimeter
                    | ActiveTool::MeasureArea
                    | ActiveTool::MeasureVolume
                    | ActiveTool::MeasureAngle
            ) && (snap_res.is_some() || canvas.creation.is_some())))
            && let Some(tex) = base_tex
        {
            draw_magnifier_loupe(
                &overlay_painter,
                pane_rect,
                screen_pos,
                pt,
                &page,
                tex,
                snap_res,
                theme,
            );
        }
    }

    // Backspace during Poly creation removes the last placed vertex (Bluebeam-style vertex undo)
    if ui.input(|i| i.key_pressed(egui::Key::Backspace))
        && let Some(CreationState::Poly { ref mut points, .. }) = canvas.creation
    {
        if points.len() > 1 {
            points.pop();
        } else {
            canvas.creation = None;
        }
        return;
    }

    // Cancel creation on Escape
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if canvas.creation.take().is_some() {
            return;
        }
        if *active_tool != ActiveTool::Select {
            *active_tool = ActiveTool::Select;
            return;
        }
        tab.selected_ids.clear();
    }

    let open_note = match *active_tool {
        ActiveTool::Select => resp.clicked(),
        ActiveTool::Note => primary_pressed,
        _ => false,
    };
    if open_note && in_pane && !ctrl_down && !shift_down
        && let Some(note) = tab.markups.iter().rev().find(|m| {
            m.page_index == cam_page_idx
                && tab.is_layer_visible(&m.layer)
                && match m.kind {
                    MarkupKind::Note { pos } => note_pin_rect(xf, pos).contains(screen_pos),
                    _ => hit_test_markup(m, raw_pt, tol_pt),
                }
        })
        && matches!(note.kind, MarkupKind::Note { .. })
        && !tab.is_markup_locked(note)
    {
        canvas.inline_edit = Some(InlineTextEdit::new(note.id, note.comment.clone()));
        tab.select_only(note.id);
        canvas.drag = None;
        return;
    }

    // If Select tool active: handle selection, handles, dragging, double-click inline edit & context menu
    if *active_tool == ActiveTool::Select {
        // Smart hover cursor in Select mode for MeasureLength offset slide & endpoint handles
        if resp.hovered() && !space_down && canvas.drag.is_none() {
            let mut set_cursor = false;
            for &sid in &tab.selected_ids {
                if let Some(m) = tab.markups.iter().find(|m| {
                    m.id == sid && m.page_index == cam_page_idx && !tab.is_markup_locked(m)
                }) && let Some(h) = hit_test_handle(m, raw_pt, tol_pt * 1.1)
                {
                    if h == HandleKind::MeasureOffset {
                        if let MarkupKind::MeasureLength { start, end, .. } = &m.kind {
                            let d = *end - *start;
                            let icon = if d.x.abs() >= d.y.abs() {
                                CursorIcon::ResizeVertical
                            } else {
                                CursorIcon::ResizeHorizontal
                            };
                            ui.ctx().set_cursor_icon(icon);
                            set_cursor = true;
                            break;
                        }
                    } else {
                        let icon = match h {
                            HandleKind::CloudCorner(_) if alt_down => CursorIcon::Move,
                            HandleKind::RectCorner(0 | 2) | HandleKind::CalloutBoxCorner(0 | 2)
                            | HandleKind::CloudCorner(0 | 2) => {
                                CursorIcon::ResizeNwSe
                            }
                            HandleKind::RectCorner(_) | HandleKind::CalloutBoxCorner(_)
                            | HandleKind::CloudCorner(_) => {
                                CursorIcon::ResizeNeSw
                            }
                            _ => CursorIcon::Move,
                        };
                        ui.ctx().set_cursor_icon(icon);
                        set_cursor = true;
                        break;
                    }
                }
            }
            if !set_cursor
                && !alt_down
                && let Some(m) = tab.markups.iter().rev().find(|m| {
                    m.page_index == cam_page_idx
                        && tab.is_layer_visible(&m.layer)
                        && !tab.is_markup_locked(m)
                        && is_measure_length_offset_drag_hit(m, raw_pt, tol_pt)
                })
                && let MarkupKind::MeasureLength { start, end, .. } = &m.kind
            {
                let d = *end - *start;
                let icon = if d.x.abs() >= d.y.abs() {
                    CursorIcon::ResizeVertical
                } else {
                    CursorIcon::ResizeHorizontal
                };
                ui.ctx().set_cursor_icon(icon);
            }
        }

        if double_clicked
            && in_pane
            && let Some(m) = tab.markups.iter().rev().find(|m| {
                m.page_index == cam_page_idx
                    && tab.is_layer_visible(&m.layer)
                    && (hit_test_markup(m, raw_pt, tol_pt)
                        || matches!(m.kind, MarkupKind::Hyperlink { .. })
                            && hyperlink_badge(m, xf).is_some_and(|(r, _)| pointer_pos.is_some_and(|pos| r.contains(pos))))
            })
        {
            if let MarkupKind::Hyperlink { target, .. } = &m.kind
                && !hyperlink_badge(m, xf).is_some_and(|(r, _)| pointer_pos.is_some_and(|pos| r.contains(pos)))
            {
                actions.push(UiAction::FollowHyperlink(target.clone()));
                return;
            }
            if !tab.is_markup_locked(m) {
                let mut edit = InlineTextEdit::new(m.id, m.text_content().unwrap_or(&m.comment).to_owned());
                if let MarkupKind::Stamp { rect, subtitle, .. } = &m.kind
                    && raw_pt.y > rect.min_y + rect.height() * 0.55
                {
                    edit.subtitle = true;
                    edit.text.clone_from(subtitle);
                }
                tab.selected_ids = vec![m.id];
                canvas.inline_edit = Some(edit);
                return;
            }
        }

        let select_press = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. }
                    if pane_rect.contains(*pos) => Some(xf.screen_to_page(pos.x as f64, pos.y as f64)),
                _ => None,
            })
        });
        let callout_moved = ui.input(|i| i.pointer.press_origin())
            .map(|pos| xf.screen_to_page(pos.x as f64, pos.y as f64))
            .or(select_press)
            .is_some_and(|origin| raw_pt.dist(origin) >= 3.0 / xf.scale.max(0.05));
        if let Some(press_pt) = select_press
            && (resp.hovered() || resp.dragged() || resp.drag_stopped())
        {
            // Ctrl+Click on a Hyperlink follows it immediately (Bluebeam-style)
            if ctrl_down
                && let Some(link_target) = tab.markups.iter().rev().find_map(|m| {
                    if m.page_index == cam_page_idx
                        && tab.is_layer_visible(&m.layer)
                        && hit_test_markup(m, press_pt, tol_pt)
                        && let MarkupKind::Hyperlink { target, .. } = &m.kind
                    {
                        Some(target.clone())
                    } else {
                        None
                    }
                })
            {
                actions.push(UiAction::FollowHyperlink(link_target));
                return;
            }
            // Check selected markup handles first
            let selected_handle = tab.selected_ids.iter().find_map(|&id| {
                let m = tab.markups.iter().find(|m| {
                    m.id == id && m.page_index == cam_page_idx && !tab.is_markup_locked(m)
                })?;
                Some((m, hit_test_handle(m, press_pt, tol_pt * 1.1)?))
            });
            if let Some((m, handle)) = selected_handle {
                let callout_grab_offset = if matches!(m.kind, MarkupKind::Callout { .. }) {
                    list_handles(m).into_iter().find_map(|(kind, pos)| {
                        (kind == handle).then_some(pos - press_pt)
                    })
                } else {
                    None
                };
                canvas.drag = Some(DragMode::MoveHandle {
                    markup_id: m.id,
                    handle,
                    callout_grab_offset,
                    pushed_undo: false,
                });
                if callout_grab_offset.is_none() {
                    return;
                }
            } else if let Some(hit_markup) = tab
                .markups
                .iter()
                .rev()
                .find(|m| m.page_index == cam_page_idx && tab.is_layer_visible(&m.layer) && hit_test_markup(m, press_pt, tol_pt))
            {
                let hit_id = hit_markup.id;
                let hit_locked = tab.is_markup_locked(hit_markup);
                let drag_measure_offset = !alt_down
                    && !shift_down
                    && !ctrl_down
                    && !hit_locked
                    && tab.selected_ids.len() <= 1
                    && is_measure_length_offset_drag_hit(hit_markup, press_pt, tol_pt);
                if shift_down || ctrl_down {
                    tab.toggle_select(hit_id);
                } else if !tab.selected_ids.contains(&hit_id) {
                    tab.select_only(hit_id);
                }
                if drag_measure_offset {
                    canvas.drag = Some(DragMode::MoveHandle {
                        markup_id: hit_id,
                        handle: HandleKind::MeasureOffset,
                        callout_grab_offset: None,
                        pushed_undo: false,
                    });
                } else {
                    canvas.drag = Some(DragMode::MoveSelected {
                        last_pt: press_pt,
                        pushed_undo: false,
                    });
                }
                return;
            } else {
                if !shift_down && !ctrl_down {
                    tab.selected_ids.clear();
                }
                canvas.drag = Some(DragMode::Marquee {
                    start_pt: press_pt,
                    current_pt: raw_pt,
                });
            }
        }

        match &mut canvas.drag {
            Some(DragMode::MoveHandle {
                markup_id,
                handle,
                callout_grab_offset,
                pushed_undo,
            }) => {
                let finishing_callout = callout_grab_offset.is_some()
                    && primary_released && (*pushed_undo || callout_moved);
                if primary_down || finishing_callout {
                    if !*pushed_undo
                        && ((callout_grab_offset.is_some() && !callout_moved)
                            || (matches!(handle, HandleKind::CloudCorner(_)) && !resp.dragged()))
                    {
                        return;
                    }
                    let mid = *markup_id;
                    let h = *handle;
                    if !*pushed_undo {
                        tab.push_undo();
                        *pushed_undo = true;
                    }
                    if let Some(m) = tab.markups.iter_mut().find(|x| x.id == mid) {
                        if h == HandleKind::MeasureOffset {
                            if let MarkupKind::MeasureLength { start, end, .. } = &m.kind {
                                let d = *end - *start;
                                let icon = if d.x.abs() >= d.y.abs() {
                                    CursorIcon::ResizeVertical
                                } else {
                                    CursorIcon::ResizeHorizontal
                                };
                                ui.ctx().set_cursor_icon(icon);
                            }
                            apply_handle_drag(m, h, raw_pt, false);
                            if let MarkupKind::MeasureLength { offset_pt, .. } = &mut m.kind
                                && offset_pt.abs() < 3.0 / xf.scale.max(0.05)
                            {
                                *offset_pt = 0.0;
                            }
                        } else {
                            let mut hv_guide: Option<(Point2, Point2)> = None;
                            let free_pt = raw_pt + callout_grab_offset.unwrap_or_default();
                            let callout_box = match (&m.kind, h) {
                                (
                                    MarkupKind::CloudPlus { box_rect, .. },
                                    HandleKind::CalloutKnee,
                                ) => Some(*box_rect),
                                _ => None,
                            };
                            let target_pt = if callout_grab_offset.is_some() {
                                match (&m.kind, h) {
                                    (MarkupKind::Callout { knee, box_rect, .. }, HandleKind::CalloutKnee)
                                        if shift_down && !alt_down =>
                                            Point2::new(free_pt.x, callout_landing(*box_rect, *knee).0.y),
                                    (_, HandleKind::CalloutTip) if snap_res.is_some() => pt,
                                    (MarkupKind::Callout { knee, box_rect, .. }, HandleKind::CalloutTip)
                                        if !alt_down && (shift_down || settings.snap.ortho_45) => {
                                            let (knee, _) = callout_landing(*box_rect, *knee);
                                            let target = apply_ortho_constraint(knee, free_pt, true);
                                            hv_guide = Some((knee, target));
                                            target
                                        }
                                    _ => free_pt,
                                }
                            } else if let Some(box_rect) = callout_box {
                                if alt_down {
                                    raw_pt
                                } else {
                                    let candidate = if shift_down { raw_pt } else { pt };
                                    let anchor = box_rect_anchor(box_rect, candidate);
                                    let target = snap_callout_landing(anchor, candidate, shift_down);
                                    if target.y == anchor.y {
                                        hv_guide = Some((anchor, target));
                                    }
                                    target
                                }
                            } else if snap_res.is_none() && !alt_down {
                                let anchor = match (&m.kind, h) {
                                    (
                                        MarkupKind::Line { start: _, end }
                                        | MarkupKind::Arrow { start: _, end }
                                        | MarkupKind::MeasureLength { end, .. }
                                        | MarkupKind::MeasureDiameter { p2: end, .. },
                                        HandleKind::LineStart,
                                    ) => Some(*end),
                                    (
                                        MarkupKind::Line { start, end: _ }
                                        | MarkupKind::Arrow { start, end: _ }
                                        | MarkupKind::MeasureLength { start, .. }
                                        | MarkupKind::MeasureDiameter { p1: start, .. },
                                        HandleKind::LineEnd,
                                    ) => Some(*start),
                                    _ => None,
                                };
                                if let Some(orig) = anchor {
                                    if shift_down || settings.snap.ortho_45 {
                                        let p = apply_ortho_constraint(orig, pt, true);
                                        hv_guide = Some((orig, p));
                                        p
                                    } else {
                                        let (p, locked_hv) = apply_soft_hv_snap(orig, pt, 3.5);
                                        if locked_hv {
                                            hv_guide = Some((orig, p));
                                        }
                                        p
                                    }
                                } else {
                                    pt
                                }
                            } else {
                                pt
                            };
                            let free_corner = alt_down && matches!(h, HandleKind::CloudCorner(_));
                            apply_handle_drag(m, h, if free_corner { raw_pt } else { target_pt }, free_corner);
                            if let HandleKind::CalloutBoxCorner(corner) = h {
                                fit_callout_text(ui.ctx(), m, corner < 2);
                            }
                            if let Some((ga, gb)) = hv_guide {
                                let p = ui.painter_at(pane_rect);
                                let (ax, ay) = xf.page_to_screen(ga);
                                let (bx, by) = xf.page_to_screen(gb);
                                draw_dashed_segment(
                                    &p,
                                    Pos2::new(ax, ay),
                                    Pos2::new(bx, by),
                                    Stroke::new(1.0, theme.accent_cyan.gamma_multiply(0.8)),
                                    4.0,
                                    4.0,
                                );
                            }
                        }
                    }
                }
                if !primary_down {
                    canvas.drag = None;
                }
            }
            Some(DragMode::MoveSelected {
                last_pt,
                pushed_undo,
            }) => {
                if primary_down {
                    let raw_delta = raw_pt - *last_pt;
                    let fine_scale = if alt_down { 0.2 } else { 1.0 };
                    let delta = Point2::new(raw_delta.x * fine_scale, raw_delta.y * fine_scale);
                    if delta.len() > 0.02 {
                        let sel = tab.selected_ids.clone();
                        let locked_layers: std::collections::HashSet<String> = tab
                            .layers
                            .iter()
                            .filter(|l| l.locked)
                            .map(|l| l.name.clone())
                            .collect();
                        let any_unlocked = tab.markups.iter().any(|m| {
                            sel.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer)
                        });
                        if any_unlocked {
                            if !*pushed_undo {
                                tab.push_undo();
                                *pushed_undo = true;
                            }
                            *last_pt = raw_pt;
                            for m in &mut tab.markups {
                                if sel.contains(&m.id)
                                    && !m.locked
                                    && !locked_layers.contains(&m.layer)
                                {
                                    translate_markup(m, delta);
                                }
                            }
                            // Magnetic endpoint snap when dragging a single unlocked line/cota (without Alt)
                            if !alt_down
                                && sel.len() == 1
                                && settings.snap.snap_to_content
                                && let Some(idx) = snap_index
                                && let Some(m) = tab.markups.iter_mut().find(|m| {
                                    m.id == sel[0]
                                        && !m.locked
                                        && !locked_layers.contains(&m.layer)
                                })
                            {
                                let snap_rad = tol_pt * 0.75;
                                let anchor_snap = match &m.kind {
                                    MarkupKind::MeasureLength { start, end, .. }
                                    | MarkupKind::Line { start, end }
                                    | MarkupKind::Arrow { start, end } => {
                                        let s_hit = idx.query_nearest_corner(*start, snap_rad).map(|h| h.point - *start);
                                        let e_hit = idx.query_nearest_corner(*end, snap_rad).map(|h| h.point - *end);
                                        match (s_hit, e_hit) {
                                            (Some(ds), Some(de)) => {
                                                if ds.len() <= de.len() { Some(ds) } else { Some(de) }
                                            }
                                            (Some(ds), None) => Some(ds),
                                            (None, Some(de)) => Some(de),
                                            (None, None) => None,
                                        }
                                    }
                                    _ => None,
                                };
                                if let Some(adj) = anchor_snap
                                    && adj.len() > 1e-4
                                {
                                    translate_markup(m, adj);
                                    *last_pt = *last_pt + adj;
                                }
                            }
                        }
                    }
                } else {
                    canvas.drag = None;
                }
            }
            Some(DragMode::Marquee {
                start_pt,
                current_pt,
            }) => {
                if primary_down {
                    *current_pt = raw_pt;
                    let r = Rect2::from_min_max(*start_pt, *current_pt);
                    let (sx0, sy0) = xf.page_to_screen(r.min());
                    let (sx1, sy1) = xf.page_to_screen(r.max());
                    let sr = Rect::from_min_max(Pos2::new(sx0, sy0), Pos2::new(sx1, sy1));
                    let p = ui.painter_at(pane_rect);
                    p.rect_filled(sr, CornerRadius::ZERO, theme.accent_red.gamma_multiply(0.18));
                    p.rect_stroke(sr, CornerRadius::ZERO, Stroke::new(1.0, theme.accent_red), egui::StrokeKind::Inside);
                } else {
                    let r = Rect2::from_min_max(*start_pt, *current_pt);
                    if r.width() > 3.0 && r.height() > 3.0 {
                        let hits: Vec<Uuid> = tab
                            .markups
                            .iter()
                            .filter(|m| {
                                m.page_index == cam_page_idx
                                    && tab.is_layer_visible(&m.layer)
                                    && markup_bounds(m).intersects(r)
                            })
                            .map(|m| m.id)
                            .collect();
                        // Expand group members for any hit markup
                        let hit_groups: std::collections::HashSet<Uuid> = tab
                            .markups
                            .iter()
                            .filter(|m| hits.contains(&m.id))
                            .filter_map(|m| m.group_id)
                            .collect();
                        let mut expanded: Vec<Uuid> = hits;
                        if !hit_groups.is_empty() {
                            for m in &tab.markups {
                                if let Some(gid) = m.group_id
                                    && hit_groups.contains(&gid)
                                    && !expanded.contains(&m.id)
                                {
                                    expanded.push(m.id);
                                }
                            }
                        }
                        if shift_down || ctrl_down {
                            for id in expanded {
                                if !tab.selected_ids.contains(&id) {
                                    tab.selected_ids.push(id);
                                }
                            }
                        } else {
                            tab.selected_ids = expanded;
                        }
                    }
                    canvas.drag = None;
                }
            }
            _ => {}
        }

        // Subtle arrow-key nudging for selected markups (1px, Shift=10px, Ctrl=0.25px)
        if canvas.inline_edit.is_none()
            && !tab.selected_ids.is_empty()
            && !ui.ctx().egui_wants_keyboard_input()
        {
            let step_px = if ctrl_down {
                0.25
            } else if shift_down {
                10.0
            } else {
                1.0
            };
            let step_pt = step_px / xf.scale.max(0.05);
            let (dx, dy) = ui.input(|i| {
                let mut x = 0.0;
                let mut y = 0.0;
                if i.key_pressed(egui::Key::ArrowLeft) {
                    x -= step_pt;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    x += step_pt;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    y -= step_pt;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    y += step_pt;
                }
                (x, y)
            });
            if dx != 0.0 || dy != 0.0 {
                let sel = tab.selected_ids.clone();
                let locked_layers: std::collections::HashSet<String> = tab
                    .layers
                    .iter()
                    .filter(|l| l.locked)
                    .map(|l| l.name.clone())
                    .collect();
                if tab
                    .markups
                    .iter()
                    .any(|m| sel.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer))
                {
                    tab.push_undo();
                    let delta = Point2::new(dx, dy);
                    for m in &mut tab.markups {
                        if sel.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer) {
                            translate_markup(m, delta);
                        }
                    }
                }
            }
        }

        // Right-click context menu
        if secondary_clicked
            && in_pane
            && let Some(hit_id) = tab
                .markups
                .iter()
                .rev()
                .find(|m| m.page_index == cam_page_idx && tab.is_layer_visible(&m.layer) && hit_test_markup(m, raw_pt, tol_pt))
                .map(|m| m.id)
            && !tab.selected_ids.contains(&hit_id)
        {
            tab.select_only(hit_id);
        }
        let sel_snapshot = tab.selected_ids.clone();
        let can_cutout = sel_snapshot.len() == 1
            && tab
                .markups
                .iter()
                .find(|m| m.id == sel_snapshot[0])
                .is_some_and(|m| matches!(m.kind, MarkupKind::MeasureArea { .. } | MarkupKind::MeasureVolume { .. }));
        let selected_hyperlink = if sel_snapshot.len() == 1 {
            tab.markups.iter().find_map(|m| {
                if m.id == sel_snapshot[0]
                    && let MarkupKind::Hyperlink { target, .. } = &m.kind
                {
                    Some(target.clone())
                } else {
                    None
                }
            })
        } else {
            None
        };
        resp.context_menu(|ui| {
            if !sel_snapshot.is_empty() {
                if let Some(ref target) = selected_hyperlink
                    && ui.button("🔗 Abrir hipervínculo").clicked()
                {
                    actions.push(UiAction::FollowHyperlink(target.clone()));
                    ui.close();
                }
                if ui.button("Editar texto / comentario").clicked() {
                    actions.push(UiAction::StartInlineEdit(sel_snapshot[0]));
                    ui.close();
                }
                if can_cutout && ui.button("Agregar Recorte (descontar área)").clicked() {
                    actions.push(UiAction::CreateCutoutForMarkup(sel_snapshot[0]));
                    ui.close();
                }
                if ui
                    .button("Ajustar puntos a esquinas (Auto-Snap)")
                    .clicked()
                {
                    actions.push(UiAction::SnapSelectedToContent);
                    ui.close();
                }
                let any_locked = tab
                    .markups
                    .iter()
                    .any(|m| sel_snapshot.contains(&m.id) && m.locked);
                let lock_lbl = if any_locked {
                    "Desbloquear puntos"
                } else {
                    "Fijar / Bloquear"
                };
                if ui.button(lock_lbl).clicked() {
                    actions.push(UiAction::ToggleLockSelected);
                    ui.close();
                }
                if sel_snapshot.len() >= 2 {
                    ui.menu_button("Alinear / Distribuir", |ui| {
                        for (kind, label) in [
                            (AlignKind::Left, "Alinear a la izquierda"),
                            (AlignKind::CenterX, "Centrar horizontalmente"),
                            (AlignKind::Right, "Alinear a la derecha"),
                            (AlignKind::Top, "Alinear arriba"),
                            (AlignKind::CenterY, "Centrar verticalmente"),
                            (AlignKind::Bottom, "Alinear abajo"),
                            (AlignKind::DistributeH, "Distribuir horizontalmente"),
                            (AlignKind::DistributeV, "Distribuir verticalmente"),
                        ] {
                            if ui.button(label).clicked() {
                                actions.push(UiAction::AlignSelected(kind));
                                ui.close();
                            }
                        }
                    });
                }
                if ui.button("+ Agregar a Caja de herramientas").clicked() {
                    actions.push(UiAction::AddSelectedToToolChest);
                    ui.close();
                }
                if ui.button("Usar estilo como predeterminado").clicked() {
                    actions.push(UiAction::SetCurrentStyleAsDefault);
                    ui.close();
                }
                ui.menu_button("Cambiar estado", |ui| {
                    for &st in MarkupStatus::ALL {
                        if ui.button(st.label()).clicked() {
                            actions.push(UiAction::ChangeSelectedStatus(st));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Duplicar  (Ctrl+D)").clicked() {
                    actions.push(UiAction::DuplicateSelected);
                    ui.close();
                }
                if ui.button("Traer al frente").clicked() {
                    actions.push(UiAction::BringToFront);
                    ui.close();
                }
                if ui.button("Enviar atrás").clicked() {
                    actions.push(UiAction::SendToBack);
                    ui.close();
                }
                if ui.button("Eliminar  (Supr)").clicked() {
                    actions.push(UiAction::DeleteSelected);
                    ui.close();
                }
            } else {
                if ui.button("Insertar Leyenda aquí").clicked() {
                    actions.push(UiAction::InsertLegendOnCurrentPage(raw_pt));
                    ui.close();
                }
                if ui.button("Pegar  (Ctrl+V)").clicked() {
                    actions.push(UiAction::PasteClipboard);
                    ui.close();
                }
                if ui.button("Ajustar página  (Ctrl+0)").clicked() {
                    actions.push(UiAction::FitPage);
                    ui.close();
                }
            }
        });
        return;
    }

    // --- Interactive Creation Tools ---
    let mut active_hud_text: Option<String> = None;

    // Apply orthogonal constraint when Shift or Ortho setting is active relative to last anchor,
    // or soft magnetic horizontal/vertical alignment for cotas/lines when not holding Alt.
    if let Some(CreationState::Callout { knee: Some(knee), .. }) = &canvas.creation {
        pt = Point2::new(raw_pt.x, knee.y);
    } else if let Some(origin) = creation_anchor(&canvas.creation)
        && snap_res.is_none()
        && !alt_down
    {
        if shift_down || settings.snap.ortho_45 {
            pt = apply_ortho_constraint(origin, pt, true);
        } else if matches!(
            *active_tool,
            ActiveTool::MeasureLength
                | ActiveTool::MeasurePolylength
                | ActiveTool::MeasurePerimeter
                | ActiveTool::Calibrate
                | ActiveTool::Line
                | ActiveTool::Arrow
        ) {
            let (snapped_pt, locked_hv) = apply_soft_hv_snap(origin, pt, 3.5);
            if locked_hv {
                pt = snapped_pt;
                let p = ui.painter_at(pane_rect);
                let (ax, ay) = xf.page_to_screen(origin);
                let (bx, by) = xf.page_to_screen(pt);
                draw_dashed_segment(
                    &p,
                    Pos2::new(ax, ay),
                    Pos2::new(bx, by),
                    Stroke::new(1.0, theme.accent_cyan.gamma_multiply(0.75)),
                    4.0,
                    4.0,
                );
            }
        }
    }

    let finish_requested = double_clicked
        || secondary_clicked
        || ui.input(|i| i.key_pressed(egui::Key::Enter));

    if primary_pressed && in_pane {
        if canvas.creation.is_none() {
            match *active_tool {
                ActiveTool::Note => {
                    tab.push_undo();
                    let m = Markup::new(
                        cam_page_idx,
                        "Nota de revisión",
                        &settings.author_name,
                        default_style.clone(),
                        MarkupKind::Note { pos: pt },
                    );
                    let id = m.id;
                    tab.markups.push(m);
                    tab.select_only(id);
                    let seq_tpl = canvas
                        .active_chest_template
                        .as_ref()
                        .filter(|t| t.tool == ActiveTool::Note && t.sequence.is_some());
                    if let Some(tpl) = seq_tpl {
                        actions.push(UiAction::ConsumeToolSequence {
                            item_id: tpl.id,
                            markup_id: id,
                        });
                    } else {
                        canvas.inline_edit = Some(InlineTextEdit::new(id, String::new()));
                    }
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Typewriter => {
                    tab.push_undo();
                    let m = Markup::new(
                        cam_page_idx,
                        "Texto",
                        &settings.author_name,
                        default_style.clone(),
                        MarkupKind::Typewriter {
                            pos: pt,
                            text: "Comentario".to_string(),
                        },
                    );
                    let id = m.id;
                    tab.markups.push(m);
                    tab.select_only(id);
                    canvas.inline_edit = Some(InlineTextEdit::new(id, "Comentario".to_string()));
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Stamp => {
                    tab.push_undo();
                    let rect = Rect2::from_xywh(pt.x - 105.0, pt.y - 26.0, 210.0, 52.0);
                    let tpl = canvas
                        .active_chest_template
                        .as_ref()
                        .filter(|t| t.tool == ActiveTool::Stamp);
                    let title = tpl
                        .filter(|t| !t.default_text.is_empty())
                        .map(|t| t.default_text.clone())
                        .unwrap_or_else(|| "REVISADO".to_string());
                    let subtitle = tpl
                        .filter(|t| !t.stamp_subtitle.is_empty())
                        .map(|t| t.stamp_subtitle.clone())
                        .unwrap_or_else(|| "RedBeam Chile".to_string());
                    let subject = tpl
                        .filter(|t| !t.subject.is_empty())
                        .map(|t| t.subject.clone())
                        .unwrap_or_else(|| "Sello".to_string());
                    let mut m = Markup::new(
                        cam_page_idx,
                        subject,
                        &settings.author_name,
                        MarkupStyle {
                            bold: true,
                            font_size: 15.0,
                            line_width: 2.2,
                            fill_color: Some(default_style.stroke_color),
                            fill_opacity: 0.12,
                            ..default_style.clone()
                        },
                        MarkupKind::Stamp {
                            rect,
                            title,
                            subtitle,
                            show_date: true,
                            show_author: true,
                        },
                    );
                    if let Some(layer) = tpl.and_then(|t| t.layer.as_ref()) {
                        tab.ensure_layer(layer);
                        m.layer = layer.clone();
                    }
                    let id = m.id;
                    tab.markups.push(m);
                    tab.select_only(id);
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Legend => {
                    actions.push(UiAction::InsertLegendOnCurrentPage(pt));
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Image => {
                    if let Some(img_path) = rfd::FileDialog::new()
                        .add_filter("Imágenes", &["png", "jpg", "jpeg"])
                        .pick_file()
                    {
                        match std::fs::read(&img_path) {
                            Ok(bytes) => {
                                let (w, h) = image::load_from_memory(&bytes)
                                    .map(|im| {
                                        let iw = im.width().max(1) as f64;
                                        let ih = im.height().max(1) as f64;
                                        let max_w = 220.0;
                                        (max_w, (max_w * ih / iw).clamp(32.0, 360.0))
                                    })
                                    .unwrap_or((180.0, 120.0));
                                tab.push_undo();
                                let b64 = crate::pdf::base64_encode(&bytes);
                                let m = Markup::new(
                                    cam_page_idx,
                                    "Imagen",
                                    &settings.author_name,
                                    default_style.clone(),
                                    MarkupKind::Image {
                                        rect: Rect2::from_xywh(pt.x, pt.y, w, h),
                                        png_b64: b64.into(),
                                        caption: img_path
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("Imagen")
                                            .to_string(),
                                    },
                                );
                                let id = m.id;
                                tab.markups.push(m);
                                tab.select_only(id);
                            }
                            Err(e) => {
                                actions.push(UiAction::ShowToast(format!(
                                    "No se pudo leer la imagen: {e}"
                                )));
                            }
                        }
                    }
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Snapshot | ActiveTool::VisualSearch => {
                    canvas.creation = Some(CreationState::TwoPoint {
                        tool: *active_tool,
                        start: pt,
                        current: pt,
                        from_drag: false,
                    });
                }
                ActiveTool::MeasureCount => {
                    tab.push_undo();
                    let m = Markup::new(
                        cam_page_idx,
                        "Conteo",
                        &settings.author_name,
                        default_style.clone(),
                        MarkupKind::MeasureCount {
                            points: vec![pt],
                            opts: default_meas_opts.clone(),
                        },
                    );
                    let id = m.id;
                    tab.markups.push(m);
                    tab.select_only(id);
                    canvas.creation = Some(CreationState::Count { markup_id: id });
                    return;
                }
                ActiveTool::Ink | ActiveTool::Highlight => {
                    canvas.creation = Some(CreationState::Freehand {
                        tool: *active_tool,
                        points: vec![pt],
                    });
                }
                ActiveTool::Callout => {
                    canvas.creation = Some(CreationState::Callout {
                        arrow_tip: pt,
                        knee: None,
                        current: pt,
                    });
                }
                ActiveTool::Arc | ActiveTool::MeasureAngle => {
                    canvas.creation = Some(CreationState::ThreePoint {
                        tool: *active_tool,
                        points: vec![pt],
                        current: pt,
                    });
                }
                ActiveTool::Polyline
                | ActiveTool::Polygon
                | ActiveTool::MeasurePolylength
                | ActiveTool::MeasurePerimeter
                | ActiveTool::MeasureArea
                | ActiveTool::MeasureVolume
                | ActiveTool::CutoutPoly
                | ActiveTool::Space => {
                    let target_id = if *active_tool == ActiveTool::CutoutPoly {
                        canvas.active_cutout_target.or_else(|| {
                            tab.markups
                                .iter()
                                .rev()
                                .find(|m| {
                                    m.page_index == cam_page_idx
                                        && matches!(
                                            m.kind,
                                            MarkupKind::MeasureArea { .. }
                                                | MarkupKind::MeasureVolume { .. }
                                        )
                                })
                                .map(|m| m.id)
                        })
                    } else {
                        None
                    };
                    canvas.creation = Some(CreationState::Poly {
                        tool: *active_tool,
                        points: vec![pt],
                        current: pt,
                        target_cutout_id: target_id,
                    });
                }
                _ => {
                    canvas.creation = Some(CreationState::TwoPoint {
                        tool: *active_tool,
                        start: pt,
                        current: pt,
                        from_drag: false,
                    });
                }
            }
        } else {
            // Advance existing multi-step creation
            match &mut canvas.creation {
                Some(CreationState::TwoPoint {
                    tool,
                    start,
                    current,
                    ..
                }) => {
                    *current = pt;
                    let t = *tool;
                    let s = *start;
                    let e = *current;
                    canvas.creation = None;
                    finalize_two_point(
                        t,
                        s,
                        e,
                        cam_page_idx,
                        is_secondary,
                        tab,
                        canvas,
                        active_tool,
                        default_style,
                        default_meas_opts,
                        settings,
                        actions,
                    );
                    return;
                }
                Some(CreationState::ThreePoint { tool, points, .. }) => {
                    points.push(pt);
                    if points.len() >= 3 {
                        let pts = points.clone();
                        let t = *tool;
                        canvas.creation = None;
                        finalize_three_point(
                            t,
                            pts,
                            cam_page_idx,
                            tab,
                            active_tool,
                            default_style,
                            default_meas_opts,
                            settings,
                        );
                        return;
                    }
                }
                Some(CreationState::Callout {
                    arrow_tip,
                    knee,
                    current,
                }) => {
                    if knee.is_none() {
                        *knee = Some(pt);
                        *current = pt;
                    } else {
                        let tip = *arrow_tip;
                        let k = knee.unwrap();
                        let box_rect = callout_box_at(k, pt, 165.0, 36.0);
                        canvas.creation = None;
                        tab.push_undo();
                        let mut st = default_style.clone();
                        st.end_end = LineEnd::ClosedArrow;
                        if st.fill_color.is_none() {
                            st.fill_color = Some([255, 255, 255, 240]);
                            st.fill_opacity = 0.92;
                        }
                        let mut m = Markup::new(
                            cam_page_idx,
                            "Observación",
                            &settings.author_name,
                            st,
                            MarkupKind::Callout {
                                arrow_tip: tip,
                                knee: k,
                                box_rect,
                                text: "Revisar detalle".to_string(),
                            },
                        );
                        fit_callout_text(ui.ctx(), &mut m, false);
                        let id = m.id;
                        tab.markups.push(m);
                        tab.select_only(id);
                        let seq_tpl = canvas
                            .active_chest_template
                            .as_ref()
                            .filter(|t| t.tool == ActiveTool::Callout && t.sequence.is_some());
                        if let Some(tpl) = seq_tpl {
                            actions.push(UiAction::ConsumeToolSequence {
                                item_id: tpl.id,
                                markup_id: id,
                            });
                        } else {
                            canvas.inline_edit = Some(InlineTextEdit::new(id, "Revisar detalle".to_string()));
                        }
                        if !settings.reuse_tool {
                            *active_tool = ActiveTool::Select;
                        }
                        return;
                    }
                }
                Some(CreationState::CloudPlus { cloud_rect, .. }) => {
                    let cr = *cloud_rect;
                    let (knee, box_rect) = cloud_callout_layout(cr, pt);
                    canvas.creation = None;
                    tab.push_undo();
                    let mut st = default_style.clone();
                    st.line_style = LineStyle::Cloud;
                    st.end_end = LineEnd::ClosedArrow;
                    if st.fill_color.is_none() {
                        st.fill_color = Some([255, 255, 255, 240]);
                        st.fill_opacity = 0.92;
                    }
                    let m = Markup::new(
                        cam_page_idx,
                        "Nube+ observación",
                        &settings.author_name,
                        st,
                        MarkupKind::CloudPlus {
                            cloud_points: cr.corners().to_vec(),
                            is_rect: true,
                            knee,
                            box_rect,
                            text: "Modificar según plano".to_string(),
                        },
                    );
                    let id = m.id;
                    tab.markups.push(m);
                    tab.select_only(id);
                    canvas.inline_edit = Some(InlineTextEdit::new(id, "Modificar según plano".to_string()));
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                Some(CreationState::Poly { points, .. }) => {
                    if !finish_requested && points.last().is_none_or(|last| last.dist(pt) > 0.5) {
                        points.push(pt);
                    }
                }
                Some(CreationState::Count { markup_id }) => {
                    let mid = *markup_id;
                    if !finish_requested
                        && tab.markups.iter().any(|x| x.id == mid)
                    {
                        tab.push_undo();
                        if let Some(m) = tab.markups.iter_mut().find(|x| x.id == mid)
                            && let MarkupKind::MeasureCount { points, .. } = &mut m.kind
                        {
                            points.push(pt);
                            m.modified_at = chrono::Utc::now();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Update hover / drag state & finish drag-created TwoPoint or Freehand
    if let Some(ref mut creation) = canvas.creation {
        match creation {
            CreationState::TwoPoint {
                tool,
                start,
                current,
                from_drag,
            } => {
                *current = pt;
                if primary_down && start.dist(pt) > 10.0 / xf.scale.max(0.1) {
                    *from_drag = true;
                }
                let scale = resolve_scale_at(&page.scale, &page.viewports, *start);
                let dist_m = scale.distance_meters(*start, pt);
                let dx = pt.x - start.x;
                let dy = pt.y - start.y;
                let ang = (-dy).atan2(dx).to_degrees();
                active_hud_text = Some(format!(
                    "L: {}  ·  {ang:.1}°",
                    scale.format_length_val(dist_m)
                ));

                if primary_released && *from_drag {
                    let t = *tool;
                    let s = *start;
                    let e = *current;
                    canvas.creation = None;
                    if t == ActiveTool::CloudPlus {
                        let cr = Rect2::from_min_max(s, e);
                        canvas.creation = Some(CreationState::CloudPlus {
                            cloud_rect: cr,
                            current: Point2::new(cr.max_x + 40.0, cr.min_y),
                        });
                    } else {
                        finalize_two_point(
                            t,
                            s,
                            e,
                            cam_page_idx,
                            is_secondary,
                            tab,
                            canvas,
                            active_tool,
                            default_style,
                            default_meas_opts,
                            settings,
                            actions,
                        );
                    }
                    return;
                } else if primary_released && !*from_drag && *tool == ActiveTool::Cloud {
                    let s = *start;
                    canvas.creation = Some(CreationState::Poly {
                        tool: ActiveTool::Cloud,
                        points: vec![s],
                        current: pt,
                        target_cutout_id: None,
                    });
                    return;
                }
            }
            CreationState::ThreePoint { points, current, .. } => {
                *current = pt;
                if points.len() == 2 {
                    let deg = angle_three_points_deg(points[0], points[1], pt);
                    active_hud_text = Some(format!("Ángulo: {}", format_angle(deg)));
                }
            }
            CreationState::Callout { current, .. } | CreationState::CloudPlus { current, .. } => {
                *current = pt;
            }
            CreationState::Poly {
                tool,
                points,
                current,
                ..
            } => {
                *current = pt;
                let mut preview_pts = points.clone();
                preview_pts.push(pt);
                let scale = resolve_scale_at(&page.scale, &page.viewports, points[0]);
                if matches!(tool, ActiveTool::MeasureArea | ActiveTool::MeasureVolume) {
                    let a = scale.area_sq_meters(&preview_pts);
                    active_hud_text = Some(format!("Área: {}", scale.format_area_val(a)));
                } else {
                    let l = scale.polyline_length_meters(&preview_pts, false);
                    active_hud_text = Some(format!("Total: {}", scale.format_length_val(l)));
                }
            }
            CreationState::Freehand { tool, points } => {
                if primary_down {
                    if points.last().is_none_or(|p| p.dist(pt) > 2.0 / xf.scale.max(0.2)) {
                        points.push(pt);
                    }
                } else {
                    let pts = points.clone();
                    let t = *tool;
                    canvas.creation = None;
                    if pts.len() >= 2 {
                        tab.push_undo();
                        let mut st = default_style.clone();
                        let kind = if t == ActiveTool::Highlight {
                            if st.opacity > 0.7 {
                                st.opacity = 0.42;
                            }
                            if st.line_width < 6.0 {
                                st.line_width = 14.0;
                            }
                            MarkupKind::Highlight { strokes: vec![pts] }
                        } else {
                            MarkupKind::Ink { strokes: vec![pts] }
                        };
                        let m = Markup::new(
                            cam_page_idx,
                            t.default_subject(),
                            &settings.author_name,
                            st,
                            kind,
                        );
                        let id = m.id;
                        tab.markups.push(m);
                        tab.select_only(id);
                    }
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
            }
            CreationState::Count { .. } => {
                active_hud_text = Some("Clic para agregar punto · Esc / Clic der. para terminar".to_string());
            }
        }
    }

    // Finish Poly or Count on double-click / right-click / Enter
    if finish_requested {
        if let Some(CreationState::Poly {
            tool,
            points,
            target_cutout_id,
            ..
        }) = canvas.creation.take()
        {
            finalize_poly(
                tool,
                points,
                target_cutout_id,
                cam_page_idx,
                tab,
                active_tool,
                default_style,
                default_meas_opts,
                settings,
            );
            return;
        }
        if matches!(canvas.creation, Some(CreationState::Count { .. })) {
            canvas.creation = None;
            if !settings.reuse_tool {
                *active_tool = ActiveTool::Select;
            }
            return;
        }
    }

    // Paint live creation preview + cursor HUD
    if let Some(creation) = &canvas.creation {
        let p = ui.painter_at(pane_rect);
        draw_creation_preview(&p, xf, creation, default_style, theme);
        if let Some(hud) = active_hud_text {
            let hud_pos = screen_pos + Vec2::new(16.0, 18.0);
            let bg_rect = Rect::from_min_size(hud_pos, Vec2::new(165.0, 22.0));
            p.rect_filled(bg_rect, CornerRadius::same(4), Color32::from_rgba_unmultiplied(15, 23, 42, 225));
            p.rect_stroke(bg_rect, CornerRadius::same(4), Stroke::new(1.0, theme.accent_cyan), egui::StrokeKind::Inside);
            p.text(
                hud_pos + Vec2::new(6.0, 11.0),
                egui::Align2::LEFT_CENTER,
                hud,
                FontId::proportional(11.5),
                Color32::WHITE,
            );
        }
    }
}

fn snap_callout_landing(anchor: Point2, target: Point2, lock_horizontal: bool) -> Point2 {
    let delta = target - anchor;
    if lock_horizontal || delta.y.abs().atan2(delta.x.abs()) <= 8.0_f64.to_radians() {
        Point2::new(target.x, anchor.y)
    } else {
        target
    }
}

fn callout_box_at(knee: Point2, anchor: Point2, width: f64, height: f64) -> Rect2 {
    let x = if anchor.x < knee.x {
        anchor.x.min(knee.x - 12.0) - width
    } else {
        anchor.x.max(knee.x + 12.0)
    };
    Rect2::from_xywh(x, anchor.y - height * 0.5, width, height)
}

fn cloud_callout_layout(cloud_rect: Rect2, anchor: Point2) -> (Point2, Rect2) {
    let knee = Point2::new(cloud_rect.center().midpoint(anchor).x, anchor.y);
    (knee, callout_box_at(knee, anchor, 175.0, 38.0))
}

#[cfg(test)]
mod callout_tests {
    use super::*;

    #[test]
    fn callout_landing_snaps_gently_and_can_lock_to_horizontal() {
        let knee = Point2::new(100.0, 100.0);
        for x in [60.0, 140.0] {
            let almost_level = Point2::new(x, 104.0);
            let angled = Point2::new(x, 120.0);
            let horizontal = Point2::new(x, knee.y);
            assert_eq!(snap_callout_landing(knee, almost_level, false), horizontal);
            assert_eq!(snap_callout_landing(knee, angled, false), angled);
            assert_eq!(snap_callout_landing(knee, angled, true), horizontal);
            assert_eq!(
                box_rect_anchor(callout_box_at(knee, angled, 165.0, 36.0), knee),
                angled
            );
        }
    }

    #[test]
    fn cloud_callout_lands_horizontally_on_either_side() {
        let cloud = Rect2::from_xywh(80.0, 80.0, 40.0, 40.0);
        for x in [40.0, 160.0] {
            let anchor = Point2::new(x, 55.0);
            let (knee, box_rect) = cloud_callout_layout(cloud, anchor);
            assert_eq!(knee.y, anchor.y);
            assert_eq!(box_rect_anchor(box_rect, knee), anchor);
        }
    }

    #[test]
    fn cloud_callout_text_edits_in_label_and_can_commit_or_cancel() {
        use crate::model::document::PageModel;
        use crate::model::settings::ThemeMode;

        let box_rect = Rect2::from_xywh(250.0, 60.0, 180.0, 45.0);
        let markup = Markup::new(
            0,
            "Nube+",
            "Autor",
            MarkupStyle::default(),
            MarkupKind::CloudPlus {
                cloud_points: Rect2::from_xywh(40.0, 160.0, 120.0, 90.0)
                    .corners()
                    .to_vec(),
                is_rect: true,
                knee: Point2::new(200.0, 90.0),
                box_rect,
                text: "Revisar".into(),
            },
        );
        let id = markup.id;
        let mut tab = DocumentTab::new_with_pages(
            "Prueba",
            None,
            Vec::new(),
            vec![PageModel::new(0, 500.0, 350.0)],
        );
        tab.markups.push(markup);
        let xf = PageTransform {
            page_width_pt: 500.0,
            page_height_pt: 350.0,
            viewport_origin_x: 0.0,
            viewport_origin_y: 0.0,
            scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 350.0));
        let theme = RedBimTheme::from_mode(ThemeMode::default());
        let ctx = egui::Context::default();
        let mut canvas = CanvasState::default();
        let run = |tab: &mut DocumentTab, canvas: &mut CanvasState, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(pane),
                    events,
                    ..Default::default()
                },
                |ui| {
                    draw_inline_text_editor(ui, pane, xf, 0, tab, canvas, &theme);
                },
            );
            output.textures_delta.clear();
        };
        for (key, modifiers) in [
            (egui::Key::Escape, egui::Modifiers::NONE),
            (egui::Key::Enter, egui::Modifiers::CTRL),
        ] {
            let editor = InlineTextEdit::new(id, "Revisar".into());
            assert_eq!(
                inline_text_geometry(&tab.markups[0], &editor, xf, pane, &theme).0,
                screen_rect(xf, box_rect).shrink2(Vec2::new(5.0, 3.0))
            );
            canvas.inline_edit = Some(editor);
            run(&mut tab, &mut canvas, vec![]);
            run(
                &mut tab,
                &mut canvas,
                vec![egui::Event::Text(" detalle".into())],
            );
            assert!(canvas.inline_edit.as_ref().unwrap().changed);
            run(
                &mut tab,
                &mut canvas,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
            );
            assert!(canvas.inline_edit.is_none());
            assert_eq!(
                tab.markups[0].text_content(),
                Some(if key == egui::Key::Escape {
                    "Revisar"
                } else {
                    "Revisar detalle"
                })
            );
        }
        assert!(tab.undo());
        assert_eq!(tab.markups[0].text_content(), Some("Revisar"));
    }
}

fn creation_anchor(c: &Option<CreationState>) -> Option<Point2> {
    match c {
        Some(CreationState::TwoPoint { start, .. }) => Some(*start),
        Some(CreationState::ThreePoint { points, .. }) => points.last().copied(),
        Some(CreationState::Poly { points, .. }) => points.last().copied(),
        Some(CreationState::Callout {
            arrow_tip, knee, ..
        }) => Some(knee.unwrap_or(*arrow_tip)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_two_point(
    tool: ActiveTool,
    start: Point2,
    end: Point2,
    page_idx: usize,
    is_secondary: bool,
    tab: &mut DocumentTab,
    canvas: &mut CanvasState,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
    actions: &mut Vec<UiAction>,
) {
    if start.dist(end) < 2.0 {
        return;
    }

    match tool {
        ActiveTool::ZoomWindow => {
            let r = Rect2::from_min_max(start, end);
            let cam = if is_secondary {
                &mut tab.secondary_camera
            } else {
                &mut tab.camera
            };
            let (vw, vh) = canvas.last_viewport_size;
            let zx = vw / r.width().max(10.0);
            let zy = vh / r.height().max(10.0);
            cam.zoom = zx.min(zy).clamp(0.05, 16.0);
            cam.pan = r.min();
            cam.fit_page = false;
            *active_tool = ActiveTool::Select;
            return;
        }
        ActiveTool::Calibrate => {
            let dist_pts = start.dist(end);
            actions.push(UiAction::OpenDialog(DialogRequest::Calibrate {
                measured_pts: dist_pts,
                initial_value: "10.00".to_string(),
                unit: tab
                    .pages
                    .get(page_idx)
                    .map(|p| p.scale.display_unit)
                    .unwrap_or(settings.default_length_unit),
                apply_all_pages: false,
            }));
            *active_tool = ActiveTool::Select;
            return;
        }
        ActiveTool::ViewportRegion => {
            let rect = Rect2::from_min_max(start, end);
            tab.push_undo();
            if let Some(p) = tab.pages.get_mut(page_idx) {
                let n = p.viewports.len() + 1;
                p.viewports.push(ViewportScale::new(
                    format!("Viewport {n}"),
                    rect,
                    PageScale::from_metric_ratio(20.0),
                ));
            }
            *active_tool = ActiveTool::Select;
            return;
        }
        ActiveTool::Snapshot => {
            let r = Rect2::from_min_max(start, end);
            if r.width() > 4.0 && r.height() > 4.0 {
                actions.push(UiAction::CaptureSnapshot(r));
            }
            *active_tool = ActiveTool::Select;
            return;
        }
        ActiveTool::VisualSearch => {
            let r = Rect2::from_min_max(start, end);
            if r.width() > 4.0 && r.height() > 4.0 {
                actions.push(UiAction::RunVisualSearch(r));
            }
            *active_tool = ActiveTool::Select;
            return;
        }
        _ => {}
    }

    let rect = Rect2::from_min_max(start, end);
    let mut style = default_style.clone();
    let mut open_inline = false;
    let kind = match tool {
        ActiveTool::Line => MarkupKind::Line { start, end },
        ActiveTool::Arrow => {
            style.end_end = LineEnd::ClosedArrow;
            MarkupKind::Arrow { start, end }
        }
        ActiveTool::Rectangle => MarkupKind::Rectangle { rect },
        ActiveTool::Ellipse => MarkupKind::Ellipse { rect },
        ActiveTool::Cloud => {
            style.line_style = LineStyle::Cloud;
            MarkupKind::Cloud {
                points: rect.corners().to_vec(),
                is_rect: true,
            }
        }
        ActiveTool::CloudPlus => {
            style.line_style = LineStyle::Cloud;
            style.end_end = LineEnd::ClosedArrow;
            if style.fill_color.is_none() {
                style.fill_color = Some([255, 255, 255, 240]);
                style.fill_opacity = 0.92;
            }
            open_inline = true;
            MarkupKind::CloudPlus {
                cloud_points: rect.corners().to_vec(),
                is_rect: true,
                knee: Point2::new(rect.max_x + 25.0, rect.min_y),
                box_rect: Rect2::from_xywh(rect.max_x + 40.0, rect.min_y - 18.0, 175.0, 36.0),
                text: "Revisar área".to_string(),
            }
        }
        ActiveTool::TextBox => {
            if style.fill_color.is_none() {
                style.fill_color = Some([255, 255, 255, 235]);
                style.fill_opacity = 0.92;
            }
            open_inline = true;
            MarkupKind::TextBox {
                rect,
                text: "Comentario".to_string(),
            }
        }
        ActiveTool::MeasureLength => {
            if style.start_end == LineEnd::None && style.end_end == LineEnd::None {
                style.start_end = LineEnd::Slash;
                style.end_end = LineEnd::Slash;
            }
            MarkupKind::MeasureLength {
                start,
                end,
                offset_pt: -16.0,
                opts: default_meas_opts.clone(),
            }
        }
        ActiveTool::MeasureRadius => MarkupKind::MeasureRadius {
            center: start,
            edge: end,
            three_point: None,
            opts: default_meas_opts.clone(),
        },
        ActiveTool::MeasureDiameter => MarkupKind::MeasureDiameter {
            p1: start,
            p2: end,
            opts: default_meas_opts.clone(),
        },
        ActiveTool::Hyperlink => {
            let target_page = if tab.pages.len() > 1 {
                (page_idx + 1) % tab.pages.len()
            } else {
                0
            };
            let target_lbl = tab.page_display_label(target_page);
            style.stroke_color = [37, 99, 235, 255];
            style.fill_color = Some([37, 99, 235, 255]);
            style.fill_opacity = 0.14;
            style.line_style = LineStyle::Dashed;
            style.line_width = 1.5;
            MarkupKind::Hyperlink {
                rect,
                target: HyperlinkTarget::Page {
                    page_index: target_page,
                },
                description: format!("Ir a {target_lbl}"),
            }
        }
        ActiveTool::Redaction => {
            style.stroke_color = [220, 38, 38, 255];
            style.fill_color = Some([0, 0, 0, 255]);
            style.fill_opacity = 0.45;
            style.line_width = 1.6;
            MarkupKind::Redaction {
                rect,
                overlay_text: "CENSURADO".to_string(),
            }
        }
        _ => return,
    };

    let tpl = canvas
        .active_chest_template
        .as_ref()
        .filter(|t| t.tool == tool);
    let subject = tpl
        .filter(|t| !t.subject.is_empty())
        .map(|t| t.subject.clone())
        .unwrap_or_else(|| tool.default_subject().to_string());

    tab.push_undo();
    let mut m = Markup::new(
        page_idx,
        subject,
        &settings.author_name,
        style,
        kind,
    );
    if let Some(t) = tpl {
        if let Some(ref layer) = t.layer {
            tab.ensure_layer(layer);
            m.layer = layer.clone();
        }
        if t.unit_cost.is_some() {
            m.unit_cost = t.unit_cost;
        }
    }
    let id = m.id;
    let init_txt = m.text_content().unwrap_or("").to_string();
    let seq_item_id = tpl.filter(|t| t.sequence.is_some()).map(|t| t.id);
    tab.markups.push(m);
    tab.select_only(id);
    if let Some(item_id) = seq_item_id {
        actions.push(UiAction::ConsumeToolSequence {
            item_id,
            markup_id: id,
        });
    } else if open_inline {
        canvas.inline_edit = Some(InlineTextEdit::new(id, init_txt));
    }
    if !settings.reuse_tool {
        *active_tool = ActiveTool::Select;
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_three_point(
    tool: ActiveTool,
    pts: Vec<Point2>,
    page_idx: usize,
    tab: &mut DocumentTab,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
) {
    if pts.len() < 3 {
        return;
    }
    let kind = match tool {
        ActiveTool::Arc => MarkupKind::Arc {
            start: pts[0],
            mid: pts[1],
            end: pts[2],
        },
        ActiveTool::MeasureAngle => MarkupKind::MeasureAngle {
            p1: pts[0],
            vertex: pts[1],
            p2: pts[2],
            opts: default_meas_opts.clone(),
        },
        _ => return,
    };
    tab.push_undo();
    let m = Markup::new(
        page_idx,
        tool.default_subject(),
        &settings.author_name,
        default_style.clone(),
        kind,
    );
    let id = m.id;
    tab.markups.push(m);
    tab.select_only(id);
    if !settings.reuse_tool {
        *active_tool = ActiveTool::Select;
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_poly(
    tool: ActiveTool,
    pts: Vec<Point2>,
    target_cutout_id: Option<Uuid>,
    page_idx: usize,
    tab: &mut DocumentTab,
    active_tool: &mut ActiveTool,
    default_style: &MarkupStyle,
    default_meas_opts: &MeasurementOptions,
    settings: &AppSettings,
) {
    if pts.len() < 2 {
        return;
    }
    if tool == ActiveTool::CutoutPoly {
        if pts.len() >= 3
            && let Some(tid) = target_cutout_id
        {
            tab.push_undo();
            if let Some(m) = tab.markups.iter_mut().find(|x| x.id == tid) {
                match &mut m.kind {
                    MarkupKind::MeasureArea { cutouts, .. }
                    | MarkupKind::MeasureVolume { cutouts, .. } => {
                        cutouts.push(pts);
                    }
                    _ => {}
                }
            }
        }
        *active_tool = ActiveTool::Select;
        return;
    }

    if tool == ActiveTool::Space {
        if pts.len() >= 3 {
            const SPACE_PALETTE: [[u8; 4]; 6] = [
                [37, 99, 235, 255],
                [22, 163, 74, 255],
                [245, 158, 11, 255],
                [147, 51, 234, 255],
                [6, 182, 212, 255],
                [229, 57, 53, 255],
            ];
            tab.push_undo();
            let idx = tab.spaces.len();
            let mut sp = SpaceRegion::new(page_idx, format!("Espacio {}", idx + 1), pts);
            sp.fill_color = SPACE_PALETTE[idx % SPACE_PALETTE.len()];
            tab.spaces.push(sp);
            tab.highlight_spaces = true;
        }
        if !settings.reuse_tool {
            *active_tool = ActiveTool::Select;
        }
        return;
    }

    let mut style = default_style.clone();
    let kind = match tool {
        ActiveTool::Polyline => MarkupKind::Polyline { points: pts },
        ActiveTool::Polygon if pts.len() >= 3 => MarkupKind::Polygon { points: pts },
        ActiveTool::Cloud if pts.len() >= 3 => {
            style.line_style = LineStyle::Cloud;
            MarkupKind::Cloud {
                points: pts,
                is_rect: false,
            }
        }
        ActiveTool::MeasurePolylength => MarkupKind::MeasurePolylength {
            points: pts,
            opts: default_meas_opts.clone(),
        },
        ActiveTool::MeasurePerimeter => MarkupKind::MeasurePerimeter {
            points: pts,
            closed: false,
            opts: default_meas_opts.clone(),
        },
        ActiveTool::MeasureArea if pts.len() >= 3 => {
            if style.fill_color.is_none() {
                style.fill_color = Some(style.stroke_color);
                style.fill_opacity = 0.22;
            }
            MarkupKind::MeasureArea {
                points: pts,
                cutouts: Vec::new(),
                opts: default_meas_opts.clone(),
            }
        }
        ActiveTool::MeasureVolume if pts.len() >= 3 => {
            if style.fill_color.is_none() {
                style.fill_color = Some(style.stroke_color);
                style.fill_opacity = 0.22;
            }
            MarkupKind::MeasureVolume {
                points: pts,
                cutouts: Vec::new(),
                opts: default_meas_opts.clone(),
            }
        }
        _ => return,
    };

    tab.push_undo();
    let m = Markup::new(
        page_idx,
        tool.default_subject(),
        &settings.author_name,
        style,
        kind,
    );
    let id = m.id;
    tab.markups.push(m);
    tab.select_only(id);
    if !settings.reuse_tool {
        *active_tool = ActiveTool::Select;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_markup_on_canvas(
    painter: &Painter,
    xf: PageTransform,
    m: &Markup,
    page_scale: &PageScale,
    viewports: &[ViewportScale],
    selected: bool,
    legend_rows: &[(String, usize, String)],
    _totals: &crate::model::document::DocumentTotals,
    image_textures: &HashMap<Uuid, TextureHandle>,
    theme: &RedBimTheme,
    inline_edit: Option<&InlineTextEdit>,
) {
    let show_text = inline_edit.is_none_or(|edit| edit.subtitle);
    let sc = m.style.stroke_color;
    let alpha = (m.style.opacity * 255.0).clamp(10.0, 255.0) as u8;
    let stroke_col = Color32::from_rgba_unmultiplied(sc[0], sc[1], sc[2], alpha);
    let sw = (m.style.line_width * (xf.scale as f32).sqrt().clamp(0.7, 2.2)).max(1.0);
    let stroke = Stroke::new(sw, stroke_col);

    let fill_col = m.style.fill_color.map(|fc| {
        let fa = (m.style.fill_opacity * m.style.opacity * 255.0).clamp(8.0, 255.0) as u8;
        Color32::from_rgba_unmultiplied(fc[0], fc[1], fc[2], fa)
    });
    let text_col = {
        let tc = m.style.text_color;
        Color32::from_rgba_unmultiplied(tc[0], tc[1], tc[2], alpha)
    };
    let font_px = (m.style.font_size * xf.scale as f32).clamp(6.0, 48.0);
    let to_pos = |pt: Point2| {
        let (x, y) = xf.page_to_screen(pt);
        Pos2::new(x, y)
    };

    let metrics = m.compute_metrics(page_scale, viewports);

    match &m.kind {
        MarkupKind::Note { pos } => {
            let r = note_pin_rect(xf, *pos);
            let bg = fill_col.unwrap_or(Color32::from_rgb(254, 240, 138));
            painter.rect(r, CornerRadius::same(4), bg, Stroke::new(1.5, stroke_col), egui::StrokeKind::Inside);
            let glyph = match m.style.note_icon {
                NoteIcon::Checkmark => "OK",
                NoteIcon::Cross => "X",
                NoteIcon::Help => "?",
                NoteIcon::Star => "*",
                _ => "N",
            };
            painter.text(r.center(), egui::Align2::CENTER_CENTER, glyph, FontId::proportional(11.5), Color32::from_rgb(30, 30, 35));
        }
        MarkupKind::TextBox { rect, text } => {
            draw_box_shape(painter, xf, *rect, m.style.box_shape, fill_col, stroke);
            if show_text {
                draw_wrapped_box_text(painter, screen_rect(xf, *rect), text, font_px, text_col);
            }
        }
        MarkupKind::Typewriter { pos, text } if show_text => {
            painter.text(
                to_pos(*pos),
                egui::Align2::LEFT_TOP,
                text,
                FontId::proportional(font_px),
                text_col,
            );
        }
        MarkupKind::Typewriter { .. } => {}
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            text,
        } => {
            let (knee, anchor) = callout_landing(*box_rect, *knee);
            painter.line_segment([to_pos(*arrow_tip), to_pos(knee)], stroke);
            painter.line_segment([to_pos(knee), to_pos(anchor)], stroke);
            draw_line_end(painter, to_pos(knee), to_pos(*arrow_tip), m.style.end_end, stroke, stroke_col);
            draw_box_shape(painter, xf, *box_rect, m.style.box_shape, fill_col, stroke);
            if show_text {
                let rect = screen_rect(xf, *box_rect);
                let inner = callout_text_rect(rect, xf.scale as f32);
                let galley = painter.layout(
                    text.clone(),
                    FontId::proportional(m.style.font_size * xf.scale as f32),
                    text_col,
                    inner.width().max(1.0),
                );
                if galley.size().y <= inner.height() + xf.scale as f32 {
                    painter.with_clip_rect(inner).galley(inner.left_top(), galley, text_col);
                } else {
                    draw_wrapped_box_text(painter, rect, text, font_px, text_col);
                }
            }
        }
        MarkupKind::Cloud { points, .. } => {
            let cloud = generate_cloud_polyline(points, m.style.cloud_arc_size as f64);
            let spts: Vec<Pos2> = cloud.into_iter().map(to_pos).collect();
            if let Some(fc) = fill_col
                && points.len() >= 3
            {
                paint_triangulated_fill(painter, xf, points, &[], fc);
            }
            if spts.len() >= 2 {
                painter.add(egui::Shape::closed_line(spts, stroke));
            }
        }
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            text,
            ..
        } => {
            let cloud = generate_cloud_polyline(cloud_points, m.style.cloud_arc_size as f64);
            let spts: Vec<Pos2> = cloud.into_iter().map(to_pos).collect();
            if spts.len() >= 2 {
                painter.add(egui::Shape::closed_line(spts, stroke));
            }
            let c_pt = polygon_centroid(cloud_points);
            let anchor = box_rect_anchor(*box_rect, *knee);
            painter.line_segment([to_pos(c_pt), to_pos(*knee)], stroke);
            painter.line_segment([to_pos(*knee), to_pos(anchor)], stroke);
            draw_box_shape(painter, xf, *box_rect, m.style.box_shape, fill_col, stroke);
            if show_text {
                draw_wrapped_box_text(painter, screen_rect(xf, *box_rect), text, font_px, text_col);
            }
        }
        MarkupKind::Line { start, end } => {
            draw_styled_polyline(painter, &[to_pos(*start), to_pos(*end)], false, m.style.line_style, stroke);
            draw_line_end(painter, to_pos(*end), to_pos(*start), m.style.start_end, stroke, stroke_col);
            draw_line_end(painter, to_pos(*start), to_pos(*end), m.style.end_end, stroke, stroke_col);
        }
        MarkupKind::Arrow { start, end } => {
            draw_styled_polyline(painter, &[to_pos(*start), to_pos(*end)], false, m.style.line_style, stroke);
            let end_style = if m.style.end_end == LineEnd::None {
                LineEnd::ClosedArrow
            } else {
                m.style.end_end
            };
            draw_line_end(painter, to_pos(*end), to_pos(*start), m.style.start_end, stroke, stroke_col);
            draw_line_end(painter, to_pos(*start), to_pos(*end), end_style, stroke, stroke_col);
        }
        MarkupKind::Polyline { points } | MarkupKind::MeasurePolylength { points, .. } => {
            let spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            draw_styled_polyline(painter, &spts, false, m.style.line_style, stroke);
            if spts.len() >= 2 {
                draw_line_end(painter, spts[1], spts[0], m.style.start_end, stroke, stroke_col);
                draw_line_end(
                    painter,
                    spts[spts.len() - 2],
                    spts[spts.len() - 1],
                    m.style.end_end,
                    stroke,
                    stroke_col,
                );
            }
        }
        MarkupKind::Arc { start, mid, end } => {
            let arc = sample_three_point_arc(*start, *mid, *end, 28);
            let spts: Vec<Pos2> = arc.into_iter().map(to_pos).collect();
            draw_styled_polyline(painter, &spts, false, m.style.line_style, stroke);
        }
        MarkupKind::Rectangle { rect } | MarkupKind::SketchRect { rect, .. } => {
            let sr = screen_rect(xf, *rect);
            if let Some(fc) = fill_col {
                painter.rect_filled(sr, CornerRadius::ZERO, fc);
            }
            draw_hatch_overlay(painter, xf, &rect.corners(), &[], m.style.hatch, stroke_col);
            let corners: Vec<Pos2> = rect.corners().into_iter().map(to_pos).collect();
            draw_styled_polyline(painter, &corners, true, m.style.line_style, stroke);
        }
        MarkupKind::Ellipse { rect } => {
            let pts = sample_ellipse(*rect, 40);
            let spts: Vec<Pos2> = pts.iter().copied().map(to_pos).collect();
            if let Some(fc) = fill_col {
                painter.add(egui::Shape::convex_polygon(spts.clone(), fc, Stroke::NONE));
            }
            draw_hatch_overlay(painter, xf, &pts, &[], m.style.hatch, stroke_col);
            draw_styled_polyline(painter, &spts, true, m.style.line_style, stroke);
        }
        MarkupKind::Polygon { points } | MarkupKind::SketchPoly { points, closed: true, .. } => {
            draw_polygon_with_fill_and_hatch(painter, xf, points, &[], fill_col, m.style.hatch, m.style.line_style, stroke, stroke_col);
        }
        MarkupKind::SketchPoly { points, closed: false, .. } => {
            let spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            draw_styled_polyline(painter, &spts, false, m.style.line_style, stroke);
        }
        MarkupKind::MeasurePerimeter { points, closed, .. } => {
            let spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            draw_styled_polyline(painter, &spts, *closed, m.style.line_style, stroke);
        }
        MarkupKind::MeasureArea {
            points, cutouts, ..
        }
        | MarkupKind::MeasureVolume {
            points, cutouts, ..
        } => {
            draw_polygon_with_fill_and_hatch(painter, xf, points, cutouts, fill_col, m.style.hatch, m.style.line_style, stroke, stroke_col);
            for hole in cutouts {
                let hpts: Vec<Pos2> = hole.iter().copied().map(to_pos).collect();
                draw_styled_polyline(painter, &hpts, true, LineStyle::Dashed, Stroke::new(sw, Color32::from_rgb(239, 68, 68)));
            }
        }
        MarkupKind::Ink { strokes } => {
            for s in strokes {
                let spts: Vec<Pos2> = s.iter().copied().map(to_pos).collect();
                painter.add(egui::Shape::line(spts, stroke));
            }
        }
        MarkupKind::Highlight { strokes } => {
            let hi_col = Color32::from_rgba_unmultiplied(sc[0], sc[1], sc[2], (m.style.opacity * 170.0) as u8);
            let hi_stroke = Stroke::new((m.style.line_width * xf.scale as f32).max(6.0), hi_col);
            for s in strokes {
                let spts: Vec<Pos2> = s.iter().copied().map(to_pos).collect();
                painter.add(egui::Shape::line(spts, hi_stroke));
            }
        }
        MarkupKind::Stamp {
            rect,
            title,
            subtitle,
            show_date,
            show_author,
        } => {
            let sr = screen_rect(xf, *rect);
            if let Some(fc) = fill_col {
                painter.rect_filled(sr, CornerRadius::same(4), fc);
            }
            painter.rect_stroke(sr, CornerRadius::same(4), Stroke::new(sw.max(1.8), stroke_col), egui::StrokeKind::Inside);
            painter.rect_stroke(sr.shrink(3.5), CornerRadius::same(2), Stroke::new(1.0, stroke_col), egui::StrokeKind::Inside);
            let title_px = (sr.height() * 0.34).clamp(7.0, 28.0);
            let sub_px = (sr.height() * 0.20).clamp(5.5, 14.0);
            let clipped = painter.with_clip_rect(sr.shrink(3.0));
            if show_text {
                clipped.text(
                    Pos2::new(sr.center().x, sr.top() + sr.height() * 0.35),
                    egui::Align2::CENTER_CENTER,
                    title,
                    FontId::proportional(title_px),
                    text_col,
                );
            }
            let mut sub_parts = Vec::new();
            if !subtitle.is_empty() {
                sub_parts.push(subtitle.clone());
            }
            if *show_author {
                sub_parts.push(m.author.clone());
            }
            if *show_date {
                sub_parts.push(m.created_at.format("%d/%m/%Y").to_string());
            }
            if !sub_parts.is_empty() && inline_edit.is_none_or(|edit| !edit.subtitle) {
                let sub_str = sub_parts.join(" · ");
                let max_sub_w = (sr.width() - 12.0).max(20.0);
                let fit_sub = max_sub_w / (sub_str.chars().count().max(1) as f32 * 0.52);
                let final_sub_px = sub_px.min(fit_sub).max(5.0);
                clipped.text(
                    Pos2::new(sr.center().x, sr.top() + sr.height() * 0.72),
                    egui::Align2::CENTER_CENTER,
                    sub_str,
                    FontId::proportional(final_sub_px),
                    text_col,
                );
            }
        }
        MarkupKind::Image { rect, caption, .. } => {
            let sr = screen_rect(xf, *rect);
            if let Some(tex) = image_textures.get(&m.id) {
                painter.image(
                    tex.id(),
                    sr,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.rect_stroke(sr, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            } else {
                painter.rect_filled(sr, CornerRadius::same(2), Color32::from_rgba_unmultiplied(240, 244, 250, 220));
                painter.rect_stroke(sr, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
                painter.text(sr.center(), egui::Align2::CENTER_CENTER, format!("[Imagen: {caption}]"), FontId::proportional(11.0), stroke_col);
            }
        }
        MarkupKind::Legend { pos, title, width_pt } => {
            let row_h = 18.0;
            let h = 34.0 + (legend_rows.len().max(1) as f32) * row_h;
            let tl = to_pos(*pos);
            let w_px = (*width_pt * xf.scale).max(200.0) as f32;
            let lr = Rect::from_min_size(tl, Vec2::new(w_px, h));
            painter.rect_filled(lr, CornerRadius::same(4), Color32::from_rgba_unmultiplied(255, 255, 255, 242));
            painter.rect_stroke(lr, CornerRadius::same(4), Stroke::new(1.5, stroke_col), egui::StrokeKind::Inside);
            painter.rect_filled(
                Rect::from_min_size(tl, Vec2::new(w_px, 24.0)),
                CornerRadius::ZERO,
                stroke_col,
            );
            if show_text {
                painter.text(
                    tl + Vec2::new(8.0, 12.0),
                    egui::Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
            }
            for (i, (subj, cnt, qty)) in legend_rows.iter().enumerate() {
                let ry = tl.y + 30.0 + i as f32 * row_h;
                painter.text(
                    Pos2::new(tl.x + 8.0, ry),
                    egui::Align2::LEFT_TOP,
                    format!("{subj} ({cnt})"),
                    FontId::proportional(11.0),
                    Color32::from_rgb(24, 24, 27),
                );
                painter.text(
                    Pos2::new(lr.right() - 8.0, ry),
                    egui::Align2::RIGHT_TOP,
                    qty,
                    FontId::proportional(11.0),
                    stroke_col,
                );
            }
        }
        MarkupKind::Hyperlink { rect, .. } => {
            let sr = screen_rect(xf, *rect);
            let link_fill = fill_col.unwrap_or(Color32::from_rgba_unmultiplied(37, 99, 235, 36));
            painter.rect_filled(sr, CornerRadius::same(3), link_fill);
            draw_dashed_rect(painter, sr, Stroke::new(sw.max(1.4), stroke_col), 6.0, 4.0);
            let (badge_r, badge_txt) = hyperlink_badge(m, xf).unwrap();
            painter.rect_filled(
                badge_r,
                CornerRadius::same(3),
                Color32::from_rgba_unmultiplied(30, 64, 175, 220),
            );
            if show_text {
                painter.text(
                    badge_r.center(),
                    egui::Align2::CENTER_CENTER,
                    badge_txt,
                    FontId::proportional(10.0),
                    Color32::WHITE,
                );
            }
        }
        MarkupKind::Redaction {
            rect,
            overlay_text,
        } => {
            let sr = screen_rect(xf, *rect);
            painter.rect_filled(sr, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(15, 15, 20, 155));
            // Diagonal warning hatch lines indicating pending (not yet destructively applied) redaction
            let step = 12.0_f32;
            let clipped = painter.with_clip_rect(sr);
            let mut x = sr.left() - sr.height();
            while x < sr.right() {
                clipped.line_segment(
                    [Pos2::new(x, sr.bottom()), Pos2::new(x + sr.height(), sr.top())],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(239, 68, 68, 125)),
                );
                x += step;
            }
            painter.rect_stroke(
                sr,
                CornerRadius::ZERO,
                Stroke::new(1.8, Color32::from_rgb(220, 38, 38)),
                egui::StrokeKind::Inside,
            );
            let lbl = if overlay_text.trim().is_empty() {
                "REDACCIÓN PENDIENTE"
            } else {
                overlay_text.trim()
            };
            if show_text {
                clipped.text(
                    sr.center(),
                    egui::Align2::CENTER_CENTER,
                    lbl,
                    FontId::proportional((sr.height() * 0.34).clamp(8.5, 16.0)),
                    Color32::WHITE,
                );
            }
        }
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            let ext_vec = da - *start;
            let ext_len = ext_vec.len();
            let ext_stroke = Stroke::new(1.0, stroke_col.gamma_multiply(0.72));
            if ext_len > 0.5 {
                let dir = Point2::new(ext_vec.x / ext_len, ext_vec.y / ext_len);
                let gap = (3.5 / xf.scale.max(0.05)).min(ext_len * 0.35);
                let overshoot = 5.5 / xf.scale.max(0.05);
                let ea0 = *start + dir * gap;
                let ea1 = da + dir * overshoot;
                let eb0 = *end + dir * gap;
                let eb1 = db + dir * overshoot;
                painter.line_segment([to_pos(ea0), to_pos(ea1)], ext_stroke);
                painter.line_segment([to_pos(eb0), to_pos(eb1)], ext_stroke);
            } else {
                // Short tick marks even when offset_pt == 0
                let d = *end - *start;
                let l = d.len().max(1e-6);
                let n = Point2::new(-d.y / l, d.x / l);
                let tick = 5.0 / xf.scale.max(0.05);
                painter.line_segment([to_pos(*start - n * tick), to_pos(*start + n * tick)], ext_stroke);
                painter.line_segment([to_pos(*end - n * tick), to_pos(*end + n * tick)], ext_stroke);
            }
            painter.line_segment([to_pos(da), to_pos(db)], stroke);
            draw_line_end(painter, to_pos(db), to_pos(da), m.style.start_end, stroke, stroke_col);
            draw_line_end(painter, to_pos(da), to_pos(db), m.style.end_end, stroke, stroke_col);
            // Subtle vertex anchor markers at the exact measured points
            if selected {
                let anchor_col = stroke_col.gamma_multiply(0.85);
                painter.circle_filled(to_pos(*start), 2.5, anchor_col);
                painter.circle_filled(to_pos(*end), 2.5, anchor_col);
            }
            if m.measurement_opts().is_some_and(|o| o.show_caption)
                && let Some(disp) = &metrics.primary_display
            {
                draw_caption_pill(painter, to_pos(da.midpoint(db)), disp, stroke_col);
            }
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            painter.line_segment([to_pos(*p1), to_pos(*vertex)], stroke);
            painter.line_segment([to_pos(*vertex), to_pos(*p2)], stroke);
            if m.measurement_opts().is_some_and(|o| o.show_caption)
                && let Some(disp) = &metrics.primary_display
            {
                draw_caption_pill(painter, to_pos(*vertex) + Vec2::new(16.0, -14.0), disp, stroke_col);
            }
        }
        MarkupKind::MeasureRadius {
            center,
            edge,
            three_point,
            ..
        } => {
            let (c, r) = if let Some([a, b, cpt]) = three_point {
                crate::geometry::math::circle_from_three_points(*a, *b, *cpt)
                    .unwrap_or((*center, center.dist(*edge)))
            } else {
                (*center, center.dist(*edge))
            };
            let sc_pt = to_pos(c);
            painter.circle_stroke(sc_pt, (r * xf.scale) as f32, stroke);
            painter.line_segment([sc_pt, to_pos(*edge)], stroke);
            draw_line_end(painter, sc_pt, to_pos(*edge), LineEnd::ClosedArrow, stroke, stroke_col);
            if m.measurement_opts().is_some_and(|o| o.show_caption)
                && let Some(disp) = &metrics.primary_display
            {
                draw_caption_pill(painter, to_pos(c.midpoint(*edge)), disp, stroke_col);
            }
        }
        MarkupKind::MeasureDiameter { p1, p2, .. } => {
            let c = p1.midpoint(*p2);
            let r = p1.dist(*p2) * 0.5;
            painter.circle_stroke(to_pos(c), (r * xf.scale) as f32, stroke);
            painter.line_segment([to_pos(*p1), to_pos(*p2)], stroke);
            draw_line_end(painter, to_pos(*p2), to_pos(*p1), LineEnd::ClosedArrow, stroke, stroke_col);
            draw_line_end(painter, to_pos(*p1), to_pos(*p2), LineEnd::ClosedArrow, stroke, stroke_col);
            if m.measurement_opts().is_some_and(|o| o.show_caption)
                && let Some(disp) = &metrics.primary_display
            {
                draw_caption_pill(painter, to_pos(c), disp, stroke_col);
            }
        }
        MarkupKind::MeasureCount { points, .. } => {
            for (idx, pt) in points.iter().enumerate() {
                let sp = to_pos(*pt);
                draw_count_marker(painter, sp, m.style.count_style, stroke_col);
                painter.text(
                    sp + Vec2::new(9.0, -7.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", idx + 1),
                    FontId::proportional(10.5),
                    stroke_col,
                );
            }
        }
    }

    // Measurement caption pill & segment callouts (except Length/Angle/Radius/Diameter which placed it specifically)
    if m.is_measurement()
        && !matches!(
            m.kind,
            MarkupKind::MeasureLength { .. }
                | MarkupKind::MeasureAngle { .. }
                | MarkupKind::MeasureRadius { .. }
                | MarkupKind::MeasureDiameter { .. }
                | MarkupKind::MeasureCount { .. }
        )
        && m.measurement_opts().is_some_and(|o| o.show_caption)
        && let Some(disp) = &metrics.primary_display
    {
        draw_caption_pill(painter, to_pos(m.anchor_point()), disp, stroke_col);
    }

    for (a, b, seg_txt) in &metrics.segment_displays {
        let mid = to_pos(a.midpoint(*b));
        draw_caption_pill(painter, mid, seg_txt, stroke_col);
    }

    // Site photo capture indicator badge on canvas
    if !m.attachments.is_empty() {
        let b = markup_bounds(m);
        let sr = screen_rect(xf, b);
        let badge_txt = format!("📷 {}", m.attachments.len());
        let br = Rect::from_min_size(
            Pos2::new(sr.right() + 2.0, (sr.top() - 10.0).max(0.0)),
            Vec2::new(34.0, 16.0),
        );
        painter.rect_filled(
            br,
            CornerRadius::same(4),
            Color32::from_rgba_unmultiplied(15, 23, 42, 215),
        );
        painter.text(
            br.center(),
            egui::Align2::CENTER_CENTER,
            badge_txt,
            FontId::proportional(9.5),
            Color32::WHITE,
        );
    }

    // Selection bounding box & control handles
    if selected && !(inline_edit.is_some() && matches!(m.kind, MarkupKind::Note { .. })) {
        let b = markup_bounds(m);
        let sr = screen_rect(xf, b).expand(4.0);
        draw_dashed_rect(painter, sr, Stroke::new(1.2, theme.accent_cyan), 5.0, 4.0);
        for (hkind, hpos) in list_handles(m) {
            let hp = to_pos(hpos);
            if hkind == HandleKind::MeasureOffset {
                let r = 5.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(hp.x, hp.y - r),
                        Pos2::new(hp.x + r, hp.y),
                        Pos2::new(hp.x, hp.y + r),
                        Pos2::new(hp.x - r, hp.y),
                    ],
                    Color32::WHITE,
                    Stroke::new(1.5, theme.accent_cyan),
                ));
            } else {
                painter.rect(
                    Rect::from_center_size(hp, Vec2::splat(7.5)),
                    CornerRadius::same(1),
                    Color32::WHITE,
                    Stroke::new(1.5, theme.accent_red),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

fn build_legend_summary(tab: &DocumentTab, page_idx: usize) -> Vec<(String, usize, String)> {
    let Some(page) = tab.pages.get(page_idx) else {
        return Vec::new();
    };
    let mut map: BTreeMap<String, (usize, f64, f64, f64, usize)> = BTreeMap::new();
    for m in &tab.markups {
        if m.page_index != page_idx || !m.is_measurement() || !tab.is_layer_visible(&m.layer) {
            continue;
        }
        let metrics = m.compute_metrics(&page.scale, &page.viewports);
        let key = if m.label.is_empty() {
            m.subject.clone()
        } else {
            format!("{} - {}", m.subject, m.label)
        };
        let entry = map.entry(key).or_insert((0, 0.0, 0.0, 0.0, 0));
        entry.0 += 1;
        entry.1 += metrics.length_m.or(metrics.perimeter_m).unwrap_or(0.0);
        entry.2 += metrics.area_m2.unwrap_or(0.0);
        entry.3 += metrics.volume_m3.unwrap_or(0.0);
        entry.4 += metrics.count.unwrap_or(0);
    }
    map.into_iter()
        .map(|(k, (n, len, area, vol, cnt))| {
            let qty = if vol > 0.0 {
                format_volume(vol, page.scale.volume_unit, page.scale.precision)
            } else if area > 0.0 {
                format_area(area, page.scale.area_unit, page.scale.precision)
            } else if len > 0.0 {
                format_length(len, page.scale.display_unit, page.scale.precision)
            } else {
                format!("{cnt} und")
            };
            (k, n, qty)
        })
        .collect()
}

fn screen_rect(xf: PageTransform, r: Rect2) -> Rect {
    let (x0, y0) = xf.page_to_screen(r.min());
    let (x1, y1) = xf.page_to_screen(r.max());
    Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
}

fn callout_text_rect(rect: Rect, scale: f32) -> Rect {
    rect.shrink2(Vec2::new(5.0, 3.0) * scale)
}

pub(crate) fn fit_callout_text(ctx: &egui::Context, markup: &mut Markup, keep_bottom: bool) {
    let MarkupKind::Callout { knee, box_rect, text, .. } = &mut markup.kind else {
        return;
    };
    let inner = callout_text_rect(
        Rect::from_min_size(Pos2::ZERO, Vec2::new(box_rect.width() as f32, box_rect.height() as f32)),
        1.0,
    );
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout(
            text.clone(),
            FontId::proportional(markup.style.font_size),
            Color32::WHITE,
            inner.width().max(1.0),
        )
    });
    let extra = (galley.size().y + 2.0 - inner.height()).max(0.0) as f64;
    if extra > 0.0 {
        *knee = callout_landing(*box_rect, *knee).0;
    }
    if keep_bottom {
        box_rect.min_y -= extra;
    } else {
        box_rect.max_y += extra;
    }
}

fn draw_wrapped_box_text(
    painter: &Painter,
    rect: Rect,
    text: &str,
    font_px: f32,
    color: Color32,
) {
    let inner = rect.shrink2(Vec2::new(5.0, 3.0));
    if inner.width() <= 6.0 || inner.height() <= 6.0 {
        return;
    }
    let mut fitted_px = font_px.clamp(6.5, (inner.height() * 0.55).max(7.0));
    let mut galley = painter.ctx().fonts_mut(|f| {
        f.layout(
            text.to_string(),
            FontId::proportional(fitted_px),
            color,
            inner.width(),
        )
    });
    if galley.size().y > inner.height() && fitted_px > 6.5 {
        let ratio = (inner.height() / galley.size().y.max(1.0)).sqrt().clamp(0.55, 0.95);
        fitted_px = (fitted_px * ratio).max(6.2);
        galley = painter.ctx().fonts_mut(|f| {
            f.layout(
                text.to_string(),
                FontId::proportional(fitted_px),
                color,
                inner.width(),
            )
        });
    }
    painter.with_clip_rect(inner).galley(inner.left_top(), galley, color);
}

fn draw_box_shape(
    painter: &Painter,
    xf: PageTransform,
    rect: Rect2,
    shape: BoxShape,
    fill: Option<Color32>,
    stroke: Stroke,
) {
    let sr = screen_rect(xf, rect);
    match shape {
        BoxShape::Rectangle => {
            if let Some(f) = fill {
                painter.rect_filled(sr, CornerRadius::same(2), f);
            }
            painter.rect_stroke(sr, CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
        }
        BoxShape::Circle => {
            let c = sr.center();
            let rad = sr.width().max(sr.height()) * 0.5;
            if let Some(f) = fill {
                painter.circle_filled(c, rad, f);
            }
            painter.circle_stroke(c, rad, stroke);
        }
        BoxShape::Triangle => {
            let pts = vec![
                Pos2::new(sr.center().x, sr.top()),
                sr.right_bottom(),
                sr.left_bottom(),
            ];
            painter.add(egui::Shape::convex_polygon(pts, fill.unwrap_or(Color32::TRANSPARENT), stroke));
        }
        BoxShape::Hexagon => {
            let dx = sr.width() * 0.2;
            let pts = vec![
                Pos2::new(sr.left() + dx, sr.top()),
                Pos2::new(sr.right() - dx, sr.top()),
                Pos2::new(sr.right(), sr.center().y),
                Pos2::new(sr.right() - dx, sr.bottom()),
                Pos2::new(sr.left() + dx, sr.bottom()),
                Pos2::new(sr.left(), sr.center().y),
            ];
            painter.add(egui::Shape::convex_polygon(pts, fill.unwrap_or(Color32::TRANSPARENT), stroke));
        }
    }
}

fn paint_triangulated_fill(
    painter: &Painter,
    xf: PageTransform,
    outer: &[Point2],
    cutouts: &[Vec<Point2>],
    fill_col: Color32,
) {
    let tris = triangulate_polygon_with_holes(outer, cutouts);
    if tris.is_empty() {
        return;
    }
    let mut mesh = egui::epaint::Mesh::default();
    mesh.vertices.reserve(tris.len() * 3);
    mesh.indices.reserve(tris.len() * 3);
    for [a, b, c] in tris {
        let base = mesh.vertices.len() as u32;
        for pt in [a, b, c] {
            let (sx, sy) = xf.page_to_screen(pt);
            mesh.colored_vertex(Pos2::new(sx, sy), fill_col);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

#[allow(clippy::too_many_arguments)]
fn draw_polygon_with_fill_and_hatch(
    painter: &Painter,
    xf: PageTransform,
    points: &[Point2],
    cutouts: &[Vec<Point2>],
    fill_col: Option<Color32>,
    hatch: HatchPattern,
    line_style: LineStyle,
    stroke: Stroke,
    stroke_col: Color32,
) {
    if points.len() < 3 {
        return;
    }
    let spts: Vec<Pos2> = points
        .iter()
        .map(|&pt| {
            let (x, y) = xf.page_to_screen(pt);
            Pos2::new(x, y)
        })
        .collect();
    if let Some(fc) = fill_col {
        paint_triangulated_fill(painter, xf, points, cutouts, fc);
    }
    draw_hatch_overlay(painter, xf, points, cutouts, hatch, stroke_col);
    draw_styled_polyline(painter, &spts, true, line_style, stroke);
}

fn draw_hatch_overlay(
    painter: &Painter,
    xf: PageTransform,
    outer: &[Point2],
    cutouts: &[Vec<Point2>],
    hatch: HatchPattern,
    color: Color32,
) {
    let angles: &[f64] = match hatch {
        HatchPattern::None => return,
        HatchPattern::Diagonal45 => &[45.0],
        HatchPattern::Diagonal135 => &[135.0],
        HatchPattern::CrossHatch => &[45.0, 135.0],
        HatchPattern::Horizontal => &[0.0],
        HatchPattern::Vertical => &[90.0],
    };
    let spacing_pt = (10.0 / xf.scale).clamp(4.0, 36.0);
    let hstroke = Stroke::new(1.0, color.gamma_multiply(0.65));
    for &ang in angles {
        for (a, b) in clip_hatch_lines(outer, cutouts, ang, spacing_pt) {
            let (x0, y0) = xf.page_to_screen(a);
            let (x1, y1) = xf.page_to_screen(b);
            painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y1)], hstroke);
        }
    }
}

fn draw_styled_polyline(
    painter: &Painter,
    pts: &[Pos2],
    closed: bool,
    style: LineStyle,
    stroke: Stroke,
) {
    if pts.len() < 2 {
        return;
    }
    if style == LineStyle::Solid {
        if closed {
            painter.add(egui::Shape::closed_line(pts.to_vec(), stroke));
        } else {
            painter.add(egui::Shape::line(pts.to_vec(), stroke));
        }
        return;
    }
    let (dash, gap) = match style {
        LineStyle::Dotted => (2.5, 4.0),
        LineStyle::Dashed | LineStyle::DashDot => (8.0, 5.0),
        _ => (6.0, 4.0),
    };
    let count = if closed { pts.len() } else { pts.len() - 1 };
    for i in 0..count {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        draw_dashed_segment(painter, a, b, stroke, dash, gap);
    }
}

fn draw_dashed_rect(painter: &Painter, r: Rect, stroke: Stroke, dash: f32, gap: f32) {
    let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    for i in 0..4 {
        draw_dashed_segment(painter, c[i], c[(i + 1) % 4], stroke, dash, gap);
    }
}

fn draw_dashed_segment(painter: &Painter, a: Pos2, b: Pos2, stroke: Stroke, dash: f32, gap: f32) {
    let delta = b - a;
    let total = delta.length();
    if total < 0.5 {
        return;
    }
    let dir = delta / total;
    let mut dist = 0.0;
    while dist < total {
        let end = (dist + dash).min(total);
        painter.line_segment([a + dir * dist, a + dir * end], stroke);
        dist += dash + gap;
    }
}

fn draw_line_end(
    painter: &Painter,
    from: Pos2,
    to: Pos2,
    kind: LineEnd,
    stroke: Stroke,
    color: Color32,
) {
    if kind == LineEnd::None {
        return;
    }
    let d = to - from;
    let len = d.length();
    if len < 1.0 {
        return;
    }
    let dir = d / len;
    let perp = Vec2::new(-dir.y, dir.x);
    let sz = (stroke.width * 3.8).clamp(8.0, 15.0);

    match kind {
        LineEnd::OpenArrow => {
            let back = to - dir * sz;
            painter.line_segment([back + perp * (sz * 0.45), to], stroke);
            painter.line_segment([back - perp * (sz * 0.45), to], stroke);
        }
        LineEnd::ClosedArrow => {
            let back = to - dir * sz;
            painter.add(egui::Shape::convex_polygon(
                vec![to, back + perp * (sz * 0.45), back - perp * (sz * 0.45)],
                color,
                Stroke::NONE,
            ));
        }
        LineEnd::ROpenArrow => {
            let fwd = to - dir * sz;
            painter.line_segment([to + perp * (sz * 0.45), fwd], stroke);
            painter.line_segment([to - perp * (sz * 0.45), fwd], stroke);
        }
        LineEnd::RClosedArrow => {
            let fwd = to - dir * sz;
            painter.add(egui::Shape::convex_polygon(
                vec![fwd, to + perp * (sz * 0.45), to - perp * (sz * 0.45)],
                color,
                Stroke::NONE,
            ));
        }
        LineEnd::Slash => {
            let s = (dir + perp).normalized() * (sz * 0.55);
            painter.line_segment([to - s, to + s], stroke);
        }
        LineEnd::Butt => {
            painter.line_segment([to - perp * (sz * 0.5), to + perp * (sz * 0.5)], stroke);
        }
        LineEnd::Circle => {
            painter.circle_filled(to, sz * 0.35, color);
        }
        LineEnd::Square => {
            painter.rect_filled(Rect::from_center_size(to, Vec2::splat(sz * 0.6)), CornerRadius::ZERO, color);
        }
        LineEnd::Diamond => {
            let r = sz * 0.45;
            painter.add(egui::Shape::convex_polygon(
                vec![to + dir * r, to + perp * r, to - dir * r, to - perp * r],
                color,
                Stroke::NONE,
            ));
        }
        LineEnd::None => {}
    }
}

fn draw_count_marker(painter: &Painter, center: Pos2, style: CountStyle, color: Color32) {
    match style {
        CountStyle::Checkmark => {
            painter.circle_filled(center, 7.5, color.gamma_multiply(0.22));
            painter.add(egui::Shape::line(
                vec![
                    center + Vec2::new(-4.5, 0.5),
                    center + Vec2::new(-1.0, 4.0),
                    center + Vec2::new(5.0, -4.0),
                ],
                Stroke::new(2.2, color),
            ));
        }
        CountStyle::Circle => {
            painter.circle_filled(center, 5.5, color);
        }
        CountStyle::Square => {
            painter.rect_filled(Rect::from_center_size(center, Vec2::splat(10.0)), CornerRadius::same(1), color);
        }
        CountStyle::Triangle => {
            let r = 6.5;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(0.0, -r),
                    center + Vec2::new(r, r * 0.8),
                    center + Vec2::new(-r, r * 0.8),
                ],
                color,
                Stroke::NONE,
            ));
        }
        CountStyle::Diamond => {
            let r = 6.5;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(0.0, -r),
                    center + Vec2::new(r, 0.0),
                    center + Vec2::new(0.0, r),
                    center + Vec2::new(-r, 0.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

fn draw_caption_pill(painter: &Painter, center: Pos2, text: &str, color: Color32) {
    let w = (text.chars().count() as f32 * 6.8 + 14.0).max(42.0);
    let r = Rect::from_center_size(center, Vec2::new(w, 18.0));
    painter.rect_filled(r, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 232));
    painter.rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, color), egui::StrokeKind::Inside);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(11.0),
        color,
    );
}

fn draw_creation_preview(
    painter: &Painter,
    xf: PageTransform,
    creation: &CreationState,
    style: &MarkupStyle,
    theme: &RedBimTheme,
) {
    let sc = style.stroke_color;
    let col = Color32::from_rgb(sc[0], sc[1], sc[2]);
    let stroke = Stroke::new(2.0, col);
    let to_pos = |pt: Point2| {
        let (x, y) = xf.page_to_screen(pt);
        Pos2::new(x, y)
    };

    match creation {
        CreationState::TwoPoint {
            tool,
            start,
            current,
            ..
        } => match tool {
            ActiveTool::Line | ActiveTool::Arrow | ActiveTool::Calibrate | ActiveTool::MeasureLength | ActiveTool::MeasureDiameter => {
                painter.line_segment([to_pos(*start), to_pos(*current)], Stroke::new(2.0, theme.accent_cyan));
                painter.circle_filled(to_pos(*start), 3.5, theme.accent_red);
                painter.circle_filled(to_pos(*current), 3.5, theme.accent_red);
            }
            ActiveTool::MeasureRadius => {
                let r = (start.dist(*current) * xf.scale) as f32;
                painter.circle_stroke(to_pos(*start), r, stroke);
                painter.line_segment([to_pos(*start), to_pos(*current)], stroke);
            }
            ActiveTool::Ellipse => {
                let rect = Rect2::from_min_max(*start, *current);
                let spts: Vec<Pos2> = sample_ellipse(rect, 32).into_iter().map(to_pos).collect();
                painter.add(egui::Shape::closed_line(spts, stroke));
            }
            ActiveTool::Snapshot => {
                let r = screen_rect(xf, Rect2::from_min_max(*start, *current));
                painter.rect_filled(r, CornerRadius::ZERO, theme.accent_cyan.gamma_multiply(0.12));
                draw_dashed_rect(painter, r, Stroke::new(1.8, theme.accent_cyan), 6.0, 4.0);
            }
            ActiveTool::VisualSearch => {
                let r = screen_rect(xf, Rect2::from_min_max(*start, *current));
                painter.rect_filled(r, CornerRadius::same(2), theme.accent_amber.gamma_multiply(0.16));
                draw_dashed_rect(painter, r, Stroke::new(1.8, theme.accent_amber), 5.0, 3.0);
            }
            ActiveTool::Hyperlink => {
                let r = screen_rect(xf, Rect2::from_min_max(*start, *current));
                let blue = Color32::from_rgb(37, 99, 235);
                painter.rect_filled(r, CornerRadius::same(3), blue.gamma_multiply(0.15));
                draw_dashed_rect(painter, r, Stroke::new(1.8, blue), 6.0, 4.0);
            }
            ActiveTool::Redaction => {
                let r = screen_rect(xf, Rect2::from_min_max(*start, *current));
                painter.rect_filled(r, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(15, 15, 20, 140));
                painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.8, theme.accent_red), egui::StrokeKind::Inside);
            }
            _ => {
                let r = screen_rect(xf, Rect2::from_min_max(*start, *current));
                painter.rect_stroke(r, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            }
        },
        CreationState::ThreePoint { points, current, .. } => {
            let mut spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            spts.push(to_pos(*current));
            painter.add(egui::Shape::line(spts, stroke));
        }
        CreationState::Callout {
            arrow_tip,
            knee,
            current,
        } => {
            if let Some(k) = knee {
                let box_rect = callout_box_at(*k, *current, 165.0, 36.0);
                painter.line_segment([to_pos(*arrow_tip), to_pos(*k)], stroke);
                painter.line_segment([to_pos(*k), to_pos(callout_landing(box_rect, *k).1)], stroke);
                let rect = screen_rect(xf, box_rect);
                painter.rect_stroke(rect, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            } else {
                painter.line_segment([to_pos(*arrow_tip), to_pos(*current)], stroke);
            }
        }
        CreationState::CloudPlus { cloud_rect, current } => {
            let sr = screen_rect(xf, *cloud_rect);
            painter.rect_stroke(sr, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            let (knee, box_rect) = cloud_callout_layout(*cloud_rect, *current);
            painter.line_segment([sr.center(), to_pos(knee)], stroke);
            painter.line_segment([to_pos(knee), to_pos(*current)], stroke);
            painter.rect_stroke(
                screen_rect(xf, box_rect),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        CreationState::Poly {
            tool,
            points,
            current,
            ..
        } => {
            let mut all_pts = points.clone();
            all_pts.push(*current);
            if *tool == ActiveTool::Cloud && all_pts.len() >= 3 {
                let cloud = generate_cloud_polyline(&all_pts, style.cloud_arc_size as f64);
                let spts: Vec<Pos2> = cloud.into_iter().map(to_pos).collect();
                if spts.len() >= 2 {
                    painter.add(egui::Shape::closed_line(spts, stroke));
                }
            } else {
                let spts: Vec<Pos2> = all_pts.into_iter().map(to_pos).collect();
                painter.add(egui::Shape::line(spts, stroke));
            }
            for pt in points {
                painter.circle_filled(to_pos(*pt), 3.0, theme.accent_red);
            }
        }
        CreationState::Freehand { points, .. } => {
            let spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            painter.add(egui::Shape::line(spts, stroke));
        }
        CreationState::Count { .. } => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_magnifier_loupe(
    painter: &Painter,
    pane_rect: Rect,
    cursor_screen: Pos2,
    cursor_pt: Point2,
    page: &crate::model::document::PageModel,
    tex: &TextureHandle,
    snap_res: Option<SnapResult>,
    theme: &RedBimTheme,
) {
    let size = 96.0;
    let mut pos = cursor_screen + Vec2::new(22.0, -size - 14.0);
    if pos.y < pane_rect.top() + 8.0 {
        pos.y = cursor_screen.y + 22.0;
    }
    if pos.x + size > pane_rect.right() - 8.0 {
        pos.x = cursor_screen.x - size - 22.0;
    }
    let box_rect = Rect::from_min_size(pos, Vec2::splat(size));

    let uv_x = (cursor_pt.x / page.width_pt.max(1.0)) as f32;
    let uv_y = (cursor_pt.y / page.height_pt.max(1.0)) as f32;
    let span_u = 0.025;
    let span_v = span_u * (page.width_pt / page.height_pt.max(1.0)) as f32;
    let uv = Rect::from_min_max(
        Pos2::new((uv_x - span_u).clamp(0.0, 1.0), (uv_y - span_v).clamp(0.0, 1.0)),
        Pos2::new((uv_x + span_u).clamp(0.0, 1.0), (uv_y + span_v).clamp(0.0, 1.0)),
    );

    painter.rect_filled(box_rect, CornerRadius::same(6), Color32::WHITE);
    painter.image(tex.id(), box_rect, uv, Color32::WHITE);

    let c = box_rect.center();
    let cross_col = if snap_res.is_some() {
        theme.accent_amber
    } else {
        theme.accent_red
    };
    painter.line_segment([Pos2::new(box_rect.left(), c.y), Pos2::new(box_rect.right(), c.y)], Stroke::new(1.0, cross_col));
    painter.line_segment([Pos2::new(c.x, box_rect.top()), Pos2::new(c.x, box_rect.bottom())], Stroke::new(1.0, cross_col));
    painter.rect_stroke(box_rect, CornerRadius::same(6), Stroke::new(1.8, cross_col), egui::StrokeKind::Inside);

    if let Some(sr) = snap_res {
        painter.text(
            Pos2::new(box_rect.center().x, box_rect.bottom() - 3.0),
            egui::Align2::CENTER_BOTTOM,
            sr.kind.label_es(),
            FontId::proportional(9.5),
            Color32::from_rgb(15, 23, 42),
        );
    }
}

fn draw_uncalibrated_scale_banner(
    ctx: &egui::Context,
    pane_rect: Rect,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let bw = 445.0_f32.min((pane_rect.width() - 24.0).max(200.0));
    let pos = Pos2::new(pane_rect.center().x - bw * 0.5, pane_rect.top() + 8.0);
    egui::Area::new(egui::Id::unique("uncalibrated_scale_banner"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme.panel_bg)
                .stroke(Stroke::new(1.2, theme.accent_amber))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Sin escala calibrada ·")
                                .size(11.5)
                                .strong()
                                .color(theme.accent_amber),
                        );
                        for ratio in [50.0, 100.0, 200.0] {
                            if ui.small_button(format!("1:{ratio:.0}")).clicked() {
                                actions.push(UiAction::ApplyPageScale {
                                    scale: PageScale::from_metric_ratio(ratio),
                                    apply_all: false,
                                });
                            }
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Calibrar 2 pts")
                                        .size(11.0)
                                        .color(theme.text_on_accent),
                                )
                                .fill(theme.accent_red),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::StartTwoPointCalibration);
                        }
                    });
                });
        });
}
