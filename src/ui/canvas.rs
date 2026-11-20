use crate::geometry::hit_test::{
    HandleKind, apply_handle_drag, box_rect_anchor, hit_test_handle, hit_test_markup, list_handles,
    markup_bounds, translate_markup,
};
use crate::geometry::math::{
    PageTransform, Point2, Rect2, angle_three_points_deg, clip_hatch_lines,
    dimension_offset_segment, generate_cloud_polyline, polygon_centroid, sample_ellipse,
    sample_three_point_arc,
};
use crate::geometry::snap::{SnapIndex, SnapKind, SnapResult, apply_ortho_constraint, find_best_snap};
use crate::model::document::{AlignKind, DocumentTab, SplitViewMode};
use crate::model::markup::{
    BoxShape, CountStyle, HatchPattern, LineEnd, LineStyle, Markup, MarkupKind, MarkupStatus,
    MarkupStyle, MeasurementOptions, NoteIcon,
};
use crate::model::scale::{PageScale, ViewportScale, resolve_scale_at};
use crate::model::settings::AppSettings;
use crate::model::tool_chest::ActiveTool;
use crate::model::units::{format_angle, format_area, format_length, format_volume};
use crate::ui::actions::{DialogRequest, UiAction};
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
        pushed_undo: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SmoothZoomState {
    pub is_secondary: bool,
    pub target_zoom: f64,
    pub anchor_page_pt: Point2,
    pub anchor_screen: Pos2,
}

#[derive(Debug, Default)]
pub struct CanvasState {
    pub creation: Option<CreationState>,
    pub drag: Option<DragMode>,
    pub inline_edit: Option<(Uuid, String, bool)>,
    pub active_cutout_target: Option<Uuid>,
    pub last_viewport_size: (f64, f64),
    pub hover_page_pt: Option<Point2>,
    pub hover_snap: Option<SnapResult>,
    pub smooth_zoom: Option<SmoothZoomState>,
    pub last_zoom_activity: Option<std::time::Instant>,
}

pub struct TileRequest {
    pub doc_id: Uuid,
    pub page_index: usize,
    pub crop_rect_pt: Rect2,
    pub out_w: u16,
    pub out_h: u16,
}

pub struct CanvasResources<'a> {
    pub base_textures: &'a HashMap<(Uuid, usize), TextureHandle>,
    pub tile_textures: &'a HashMap<(Uuid, usize), CachedTile>,
    pub image_textures: &'a HashMap<Uuid, TextureHandle>,
    pub snap_indices: &'a HashMap<(Uuid, usize), SnapIndex>,
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
    canvas.last_viewport_size = (full_rect.width() as f64, full_rect.height() as f64);

    match tab.split_mode {
        SplitViewMode::None => {
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
    if resp.clicked() || resp.drag_started() {
        tab.active_pane_secondary = is_secondary;
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

    // Fit page if pending (first open or FitPage action)
    if cam.fit_pending && pane_rect.width() > 30.0 && pane_rect.height() > 30.0 {
        let margin = 28.0;
        let zx = (pane_rect.width() as f64 - margin * 2.0) / page.width_pt.max(1.0);
        let zy = (pane_rect.height() as f64 - margin * 2.0) / page.height_pt.max(1.0);
        cam.zoom = zx.min(zy).clamp(0.05, 8.0);
        let drawn_w = page.width_pt * cam.zoom;
        let drawn_h = page.height_pt * cam.zoom;
        let pad_x = ((pane_rect.width() as f64 - drawn_w) * 0.5) / cam.zoom;
        let pad_y = ((pane_rect.height() as f64 - drawn_h) * 0.5) / cam.zoom;
        cam.pan = Point2::new(-pad_x, -pad_y);
        cam.fit_pending = false;
        canvas.smooth_zoom = None;
    }

    // Handle smooth mouse wheel zoom anchored at cursor (60/120fps exponential ease)
    let pointer_pos = pane_ui.input(|i| i.pointer.hover_pos());
    let hovered = resp.hovered() || pointer_pos.is_some_and(|p| pane_rect.contains(p));
    if hovered {
        let scroll = pane_ui.input(|i| i.smooth_scroll_delta);
        if scroll.y.abs() > 0.1 {
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
                .filter(|s| s.is_secondary == is_secondary)
                .map(|s| s.target_zoom)
                .unwrap_or(cam.zoom);
            canvas.smooth_zoom = Some(SmoothZoomState {
                is_secondary,
                target_zoom: (base_target * factor).clamp(0.05, 16.0),
                anchor_page_pt: before_pt,
                anchor_screen,
            });
            canvas.last_zoom_activity = Some(std::time::Instant::now());
        }
    }

    if let Some(sz) = canvas.smooth_zoom.clone()
        && sz.is_secondary == is_secondary
    {
        let dt = pane_ui.input(|i| i.stable_dt).clamp(0.001, 0.05) as f64;
        let alpha = 1.0 - (-18.0 * dt).exp();
        let next_zoom = cam.zoom + (sz.target_zoom - cam.zoom) * alpha;
        if (sz.target_zoom - next_zoom).abs() < 0.0005 * next_zoom.max(0.1) {
            cam.zoom = sz.target_zoom;
            canvas.smooth_zoom = None;
        } else {
            cam.zoom = next_zoom;
            pane_ui.ctx().request_repaint();
            canvas.last_zoom_activity = Some(std::time::Instant::now());
        }
        cam.pan.x = sz.anchor_page_pt.x - (sz.anchor_screen.x as f64 - pane_rect.left() as f64) / cam.zoom;
        cam.pan.y = sz.anchor_page_pt.y - (sz.anchor_screen.y as f64 - pane_rect.top() as f64) / cam.zoom;
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
    if let Some(tex) = base_tex {
        painter.image(
            tex.id(),
            page_screen_rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
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

        let mut need_tile = true;
        if let Some(tile) = res.tile_textures.get(&(tab.id, page_idx)) {
            let covers = tile.crop_rect_pt.min_x <= crop_rect.min_x + 1.0
                && tile.crop_rect_pt.min_y <= crop_rect.min_y + 1.0
                && tile.crop_rect_pt.max_x >= crop_rect.max_x - 1.0
                && tile.crop_rect_pt.max_y >= crop_rect.max_y - 1.0;
            let res_ok = (tile.scale_px_per_pt - pixels_per_point).abs() / pixels_per_point.max(0.1) < 0.25;
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
        let zoom_settled = canvas.smooth_zoom.is_none()
            && canvas
                .last_zoom_activity
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(90));
        if need_tile && crop_rect.width() > 2.0 && crop_rect.height() > 2.0 {
            if zoom_settled {
                let padded = crop_rect.expand(crop_rect.width().max(crop_rect.height()) * 0.15);
                let clamped = Rect2::from_min_max(
                    Point2::new(padded.min_x.max(0.0), padded.min_y.max(0.0)),
                    Point2::new(padded.max_x.min(page.width_pt), padded.max_y.min(page.height_pt)),
                );
                let out_w = ((clamped.width() * pixels_per_point).round() as u32).clamp(64, 2200) as u16;
                let out_h = ((clamped.height() * pixels_per_point).round() as u32).clamp(64, 2200) as u16;
                tile_requests.push(TileRequest {
                    doc_id: tab.id,
                    page_index: page_idx,
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

    // Only process editing interactions in active pane (or if hovered)
    let is_active_pane = tab.split_mode == SplitViewMode::None || tab.active_pane_secondary == is_secondary;
    if is_active_pane {
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

    // Inline text editor floating directly over markup when double-clicked or newly placed
    let mut inline_commit: Option<(Uuid, String)> = None;
    let mut inline_cancel = false;
    if let Some((edit_id, ref mut buf, ref mut needs_focus)) = canvas.inline_edit
        && let Some(m) = tab.markups.iter().find(|x| x.id == edit_id && x.page_index == page_idx)
    {
            let b = markup_bounds(m);
            let (sx, sy) = xf.page_to_screen(b.min());
            let (ex, ey) = xf.page_to_screen(b.max());
            let w = (ex - sx).clamp(160.0, 340.0);
            let h = (ey - sy).clamp(44.0, 140.0);
            let area_pos = Pos2::new(
                sx.clamp(pane_rect.left() + 8.0, (pane_rect.right() - w - 8.0).max(pane_rect.left())),
                sy.clamp(pane_rect.top() + 8.0, (pane_rect.bottom() - h - 8.0).max(pane_rect.top())),
            );

            let mut commit = false;
            let mut cancel = false;
            egui::Area::new(egui::Id::unique(("inline_edit_area", edit_id)))
                .fixed_pos(area_pos)
                .order(egui::Order::Foreground)
                .show(pane_ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(theme.panel_bg)
                        .stroke(Stroke::new(1.5, theme.accent_red))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            let te = ui.add(
                                egui::TextEdit::multiline(buf)
                                    .desired_width(w)
                                    .desired_rows(3),
                            );
                            if *needs_focus {
                                te.request_focus();
                                *needs_focus = false;
                            }
                            ui.horizontal(|ui| {
                                if ui.small_button("Guardar (Ctrl+Enter)").clicked() {
                                    commit = true;
                                }
                                if ui.small_button("Cancelar (Esc)").clicked() {
                                    cancel = true;
                                }
                            });
                            if ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl) {
                                commit = true;
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                cancel = true;
                            }
                        });
                });

            if commit {
                inline_commit = Some((edit_id, buf.clone()));
            } else if cancel {
                inline_cancel = true;
            }
    }
    if let Some((edit_id, text_to_save)) = inline_commit {
        canvas.inline_edit = None;
        tab.push_undo();
        if let Some(m_mut) = tab.markups.iter_mut().find(|x| x.id == edit_id) {
            m_mut.set_text_content(text_to_save);
        }
    } else if inline_cancel {
        canvas.inline_edit = None;
    }
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
    let middle_down = ui.input(|i| i.pointer.middle_down());
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_pressed = resp.hovered() && ui.input(|i| i.pointer.primary_pressed());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let double_clicked = resp.double_clicked();
    let secondary_clicked = resp.secondary_clicked();

    // Update cursor icon
    if space_down || *active_tool == ActiveTool::Pan || matches!(canvas.drag, Some(DragMode::Pan { .. })) {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    } else if *active_tool != ActiveTool::Select {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
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

    // Compute current cursor page coordinate + snap
    let Some(screen_pos) = pointer_pos else {
        canvas.hover_page_pt = None;
        canvas.hover_snap = None;
        return;
    };
    let in_pane = pane_rect.contains(screen_pos);
    let raw_pt = xf.screen_to_page(screen_pos.x as f64, screen_pos.y as f64);
    let tol_pt = 10.0 / xf.scale.max(0.05);

    let snap_res = if *active_tool != ActiveTool::Select && *active_tool != ActiveTool::Pan {
        find_best_snap(
            raw_pt,
            tol_pt,
            &settings.snap,
            snap_index,
            &tab.markups,
            cam_page_idx,
            grid_step_pt,
        )
    } else {
        None
    };
    let mut pt = snap_res.map(|s| s.point).unwrap_or(raw_pt);
    canvas.hover_page_pt = Some(pt);
    canvas.hover_snap = snap_res;

    // Paint Snap indicator + Magnifier Loupe during Calibrate or Measurement tools
    if in_pane && *active_tool != ActiveTool::Select && *active_tool != ActiveTool::Pan {
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
        if matches!(
            *active_tool,
            ActiveTool::Calibrate
                | ActiveTool::MeasureLength
                | ActiveTool::MeasurePolylength
                | ActiveTool::MeasurePerimeter
                | ActiveTool::MeasureArea
                | ActiveTool::MeasureVolume
                | ActiveTool::MeasureAngle
        ) && (snap_res.is_some() || canvas.creation.is_some())
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

    // If Select tool active: handle selection, handles, dragging, double-click inline edit & context menu
    if *active_tool == ActiveTool::Select {
        if double_clicked
            && in_pane
            && let Some(m) = tab
                .markups
                .iter()
                .rev()
                .find(|m| m.page_index == cam_page_idx && tab.is_layer_visible(&m.layer) && hit_test_markup(m, raw_pt, tol_pt))
        {
            let txt = m
                .text_content()
                .map(|s| s.to_string())
                .unwrap_or_else(|| m.comment.clone());
            canvas.inline_edit = Some((m.id, txt, true));
            return;
        }

        if primary_pressed && in_pane {
            // Check selected markup handles first
            for &sid in &tab.selected_ids {
                if let Some(m) = tab.markups.iter().find(|m| m.id == sid && m.page_index == cam_page_idx)
                    && !m.locked
                    && let Some(handle) = hit_test_handle(m, raw_pt, tol_pt * 1.1)
                {
                    canvas.drag = Some(DragMode::MoveHandle {
                        markup_id: sid,
                        handle,
                        pushed_undo: false,
                    });
                    return;
                }
            }

            // Check markup body hit top-to-bottom
            if let Some(hit_id) = tab
                .markups
                .iter()
                .rev()
                .find(|m| m.page_index == cam_page_idx && tab.is_layer_visible(&m.layer) && hit_test_markup(m, raw_pt, tol_pt))
                .map(|m| m.id)
            {
                if shift_down || ctrl_down {
                    tab.toggle_select(hit_id);
                } else if !tab.selected_ids.contains(&hit_id) {
                    tab.select_only(hit_id);
                }
                canvas.drag = Some(DragMode::MoveSelected {
                    last_pt: raw_pt,
                    pushed_undo: false,
                });
                return;
            }

            if !shift_down && !ctrl_down {
                tab.selected_ids.clear();
            }
            canvas.drag = Some(DragMode::Marquee {
                start_pt: raw_pt,
                current_pt: raw_pt,
            });
        }

        match &mut canvas.drag {
            Some(DragMode::MoveHandle {
                markup_id,
                handle,
                pushed_undo,
            }) => {
                if primary_down {
                    let mid = *markup_id;
                    let h = *handle;
                    if !*pushed_undo {
                        tab.push_undo();
                        *pushed_undo = true;
                    }
                    if let Some(m) = tab.markups.iter_mut().find(|x| x.id == mid) {
                        apply_handle_drag(m, h, raw_pt);
                    }
                } else {
                    canvas.drag = None;
                }
            }
            Some(DragMode::MoveSelected {
                last_pt,
                pushed_undo,
            }) => {
                if primary_down {
                    let delta = raw_pt - *last_pt;
                    if delta.len() > 0.05 {
                        if !*pushed_undo {
                            tab.push_undo();
                            *pushed_undo = true;
                        }
                        *last_pt = raw_pt;
                        let sel = tab.selected_ids.clone();
                        for m in &mut tab.markups {
                            if sel.contains(&m.id) {
                                translate_markup(m, delta);
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
                    p.rect_filled(sr, CornerRadius::ZERO, theme.accent_red_soft);
                    p.rect_stroke(sr, CornerRadius::ZERO, Stroke::new(1.0, theme.accent_red), egui::StrokeKind::Inside);
                } else {
                    let r = Rect2::from_min_max(*start_pt, *current_pt);
                    if r.width() > 3.0 && r.height() > 3.0 {
                        tab.selected_ids = tab
                            .markups
                            .iter()
                            .filter(|m| {
                                m.page_index == cam_page_idx
                                    && tab.is_layer_visible(&m.layer)
                                    && markup_bounds(m).intersects(r)
                            })
                            .map(|m| m.id)
                            .collect();
                    }
                    canvas.drag = None;
                }
            }
            _ => {}
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
        resp.context_menu(|ui| {
            if !sel_snapshot.is_empty() {
                if ui.button("Editar texto / comentario").clicked() {
                    actions.push(UiAction::StartInlineEdit(sel_snapshot[0]));
                    ui.close();
                }
                if can_cutout && ui.button("Agregar Recorte (descontar área)").clicked() {
                    actions.push(UiAction::CreateCutoutForMarkup(sel_snapshot[0]));
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

    // Apply orthogonal constraint when Shift or Ortho setting is active relative to last anchor
    if let Some(origin) = creation_anchor(&canvas.creation)
        && (shift_down || settings.snap.ortho_45)
        && snap_res.is_none()
    {
        pt = apply_ortho_constraint(origin, pt, true);
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
                    canvas.inline_edit = Some((id, String::new(), true));
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
                    canvas.inline_edit = Some((id, "Comentario".to_string(), true));
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Stamp => {
                    tab.push_undo();
                    let rect = Rect2::from_xywh(pt.x - 105.0, pt.y - 26.0, 210.0, 52.0);
                    let m = Markup::new(
                        cam_page_idx,
                        "Sello",
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
                            title: "REVISADO".to_string(),
                            subtitle: "RedBIM Chile".to_string(),
                            show_date: true,
                            show_author: true,
                        },
                    );
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
                    // Insert clean image placeholder or prompt via file dialog
                    if let Some(img_path) = rfd::FileDialog::new()
                        .add_filter("Imágenes", &["png", "jpg", "jpeg"])
                        .pick_file()
                        && let Ok(bytes) = std::fs::read(&img_path)
                    {
                        tab.push_undo();
                        let b64 = crate::pdf::base64_encode(&bytes);
                        let m = Markup::new(
                            cam_page_idx,
                            "Imagen",
                            &settings.author_name,
                            default_style.clone(),
                            MarkupKind::Image {
                                rect: Rect2::from_xywh(pt.x, pt.y, 180.0, 120.0),
                                png_b64: b64,
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
                    if !settings.reuse_tool {
                        *active_tool = ActiveTool::Select;
                    }
                    return;
                }
                ActiveTool::Snapshot => {
                    canvas.creation = Some(CreationState::TwoPoint {
                        tool: ActiveTool::Snapshot,
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
                | ActiveTool::CutoutPoly => {
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
                        let box_rect = Rect2::from_xywh(
                            pt.x,
                            pt.y - 18.0,
                            165.0,
                            36.0,
                        );
                        canvas.creation = None;
                        tab.push_undo();
                        let mut st = default_style.clone();
                        st.end_end = LineEnd::ClosedArrow;
                        if st.fill_color.is_none() {
                            st.fill_color = Some([255, 255, 255, 240]);
                            st.fill_opacity = 0.92;
                        }
                        let m = Markup::new(
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
                        let id = m.id;
                        tab.markups.push(m);
                        tab.select_only(id);
                        canvas.inline_edit = Some((id, "Revisar detalle".to_string(), true));
                        if !settings.reuse_tool {
                            *active_tool = ActiveTool::Select;
                        }
                        return;
                    }
                }
                Some(CreationState::CloudPlus { cloud_rect, .. }) => {
                    let cr = *cloud_rect;
                    let box_rect = Rect2::from_xywh(pt.x, pt.y - 18.0, 175.0, 38.0);
                    let c_cent = cr.center();
                    let knee = Point2::new(
                        (c_cent.x + pt.x) * 0.5,
                        pt.y,
                    );
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
                    canvas.inline_edit = Some((id, "Modificar según plano".to_string(), true));
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
                        && let Some(m) = tab.markups.iter_mut().find(|x| x.id == mid)
                        && let MarkupKind::MeasureCount { points, .. } = &mut m.kind
                    {
                        points.push(pt);
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
            cam.fit_pending = false;
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
            style.start_end = LineEnd::Slash;
            style.end_end = LineEnd::Slash;
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
    let init_txt = m.text_content().unwrap_or("").to_string();
    tab.markups.push(m);
    tab.select_only(id);
    if open_inline {
        canvas.inline_edit = Some((id, init_txt, true));
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
) {
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
            let p = to_pos(*pos);
            let bg = fill_col.unwrap_or(Color32::from_rgb(254, 240, 138));
            let r = Rect::from_center_size(p, Vec2::splat(22.0));
            painter.rect(r, CornerRadius::same(4), bg, Stroke::new(1.5, stroke_col), egui::StrokeKind::Inside);
            let glyph = match m.style.note_icon {
                NoteIcon::Checkmark => "OK",
                NoteIcon::Cross => "X",
                NoteIcon::Help => "?",
                NoteIcon::Star => "*",
                _ => "N",
            };
            painter.text(p, egui::Align2::CENTER_CENTER, glyph, FontId::proportional(11.5), Color32::from_rgb(30, 30, 35));
        }
        MarkupKind::TextBox { rect, text } => {
            draw_box_shape(painter, xf, *rect, m.style.box_shape, fill_col, stroke);
            let r = screen_rect(xf, *rect);
            draw_wrapped_box_text(painter, r, text, font_px, text_col);
        }
        MarkupKind::Typewriter { pos, text } => {
            painter.text(
                to_pos(*pos),
                egui::Align2::LEFT_TOP,
                text,
                FontId::proportional(font_px),
                text_col,
            );
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            text,
        } => {
            let anchor = box_rect_anchor(*box_rect, *knee);
            painter.line_segment([to_pos(*arrow_tip), to_pos(*knee)], stroke);
            painter.line_segment([to_pos(*knee), to_pos(anchor)], stroke);
            draw_line_end(painter, to_pos(*knee), to_pos(*arrow_tip), m.style.end_end, stroke, stroke_col);
            draw_box_shape(painter, xf, *box_rect, m.style.box_shape, fill_col, stroke);
            let sr = screen_rect(xf, *box_rect);
            draw_wrapped_box_text(painter, sr, text, font_px, text_col);
        }
        MarkupKind::Cloud { points, .. } => {
            let cloud = generate_cloud_polyline(points, m.style.cloud_arc_size as f64);
            let spts: Vec<Pos2> = cloud.into_iter().map(to_pos).collect();
            if let Some(fc) = fill_col
                && points.len() >= 3
            {
                let base: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
                painter.add(egui::Shape::convex_polygon(base, fc, Stroke::NONE));
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
            let sr = screen_rect(xf, *box_rect);
            draw_wrapped_box_text(painter, sr, text, font_px, text_col);
        }
        MarkupKind::Line { start, end } => {
            draw_styled_polyline(painter, &[to_pos(*start), to_pos(*end)], false, m.style.line_style, stroke);
            draw_line_end(painter, to_pos(*end), to_pos(*start), m.style.start_end, stroke, stroke_col);
            draw_line_end(painter, to_pos(*start), to_pos(*end), m.style.end_end, stroke, stroke_col);
        }
        MarkupKind::Arrow { start, end } => {
            draw_styled_polyline(painter, &[to_pos(*start), to_pos(*end)], false, m.style.line_style, stroke);
            draw_line_end(painter, to_pos(*start), to_pos(*end), LineEnd::ClosedArrow, stroke, stroke_col);
        }
        MarkupKind::Polyline { points } | MarkupKind::MeasurePolylength { points, .. } => {
            let spts: Vec<Pos2> = points.iter().copied().map(to_pos).collect();
            draw_styled_polyline(painter, &spts, false, m.style.line_style, stroke);
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
            painter.add(egui::Shape::closed_line(spts, stroke));
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
            clipped.text(
                Pos2::new(sr.center().x, sr.top() + sr.height() * 0.35),
                egui::Align2::CENTER_CENTER,
                title,
                FontId::proportional(title_px),
                text_col,
            );
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
            if !sub_parts.is_empty() {
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
            painter.text(
                tl + Vec2::new(8.0, 12.0),
                egui::Align2::LEFT_CENTER,
                title,
                FontId::proportional(12.0),
                Color32::WHITE,
            );
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
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            let ext_stroke = Stroke::new(1.0, stroke_col.gamma_multiply(0.75));
            painter.line_segment([to_pos(*start), to_pos(da)], ext_stroke);
            painter.line_segment([to_pos(*end), to_pos(db)], ext_stroke);
            painter.line_segment([to_pos(da), to_pos(db)], stroke);
            draw_line_end(painter, to_pos(db), to_pos(da), m.style.start_end, stroke, stroke_col);
            draw_line_end(painter, to_pos(da), to_pos(db), m.style.end_end, stroke, stroke_col);
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

    // Selection bounding box & control handles
    if selected {
        let b = markup_bounds(m);
        let sr = screen_rect(xf, b).expand(4.0);
        draw_dashed_rect(painter, sr, Stroke::new(1.2, theme.accent_cyan), 5.0, 4.0);
        for (_, hpos) in list_handles(m) {
            let hp = to_pos(hpos);
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

fn build_legend_summary(tab: &DocumentTab, page_idx: usize) -> Vec<(String, usize, String)> {
    let Some(page) = tab.pages.get(page_idx) else {
        return Vec::new();
    };
    let mut map: BTreeMap<String, (usize, f64, f64, f64, usize)> = BTreeMap::new();
    for m in &tab.markups {
        if m.page_index != page_idx || !m.is_measurement() {
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
        entry.1 += metrics.length_m.unwrap_or(0.0);
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
        // Fan triangles from centroid or convex_polygon fallback
        painter.add(egui::Shape::convex_polygon(spts.clone(), fc, Stroke::NONE));
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
                painter.line_segment([to_pos(*arrow_tip), to_pos(*k)], stroke);
                painter.line_segment([to_pos(*k), to_pos(*current)], stroke);
            } else {
                painter.line_segment([to_pos(*arrow_tip), to_pos(*current)], stroke);
            }
        }
        CreationState::CloudPlus { cloud_rect, current } => {
            let sr = screen_rect(xf, *cloud_rect);
            painter.rect_stroke(sr, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            painter.line_segment([sr.center(), to_pos(*current)], stroke);
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
                                        .color(Color32::WHITE),
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
