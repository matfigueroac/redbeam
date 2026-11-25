use crate::model::tool_chest::{ActiveTool, ToolChestItem};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    NewDoc,
    OpenFile,
    SaveFile,
    FlattenPdf,
    ExportCsv,
    Undo,
    Redo,
    Select,
    Pan,
    ZoomWindow,
    FitPage,
    FitWidth,
    Note,
    TextBox,
    Typewriter,
    Callout,
    Cloud,
    CloudPlus,
    Line,
    Arrow,
    Polyline,
    Arc,
    Rectangle,
    Ellipse,
    Polygon,
    Ink,
    Highlight,
    Stamp,
    Image,
    Snapshot,
    Legend,
    Calibrate,
    MeasureLength,
    MeasurePolylength,
    MeasurePerimeter,
    MeasureArea,
    MeasureVolume,
    MeasureAngle,
    MeasureRadius,
    MeasureDiameter,
    MeasureCount,
    Cutout,
    SketchScale,
    Viewport,
    Hyperlink,
    Space,
    ToolChest,
    Thumbnails,
    Layers,
    Bookmarks,
    Properties,
    Duplicate,
    Lock,
    Trash,
    Measurements,
    Search,
    MarkupsList,
    SplitVertical,
    SplitHorizontal,
    SnapContent,
    SnapMarkup,
    SnapGrid,
    Dimmer,
    Reuse,
    ThemeToggle,
}

impl IconKind {
    pub fn from_tool(t: ActiveTool) -> Self {
        match t {
            ActiveTool::Select => Self::Select,
            ActiveTool::Pan => Self::Pan,
            ActiveTool::ZoomWindow => Self::ZoomWindow,
            ActiveTool::Note => Self::Note,
            ActiveTool::TextBox => Self::TextBox,
            ActiveTool::Typewriter => Self::Typewriter,
            ActiveTool::Callout => Self::Callout,
            ActiveTool::Cloud => Self::Cloud,
            ActiveTool::CloudPlus => Self::CloudPlus,
            ActiveTool::Line => Self::Line,
            ActiveTool::Arrow => Self::Arrow,
            ActiveTool::Polyline => Self::Polyline,
            ActiveTool::Arc => Self::Arc,
            ActiveTool::Rectangle => Self::Rectangle,
            ActiveTool::Ellipse => Self::Ellipse,
            ActiveTool::Polygon => Self::Polygon,
            ActiveTool::Ink => Self::Ink,
            ActiveTool::Highlight => Self::Highlight,
            ActiveTool::Stamp => Self::Stamp,
            ActiveTool::Image => Self::Image,
            ActiveTool::Snapshot => Self::Snapshot,
            ActiveTool::Legend => Self::Legend,
            ActiveTool::Calibrate => Self::Calibrate,
            ActiveTool::MeasureLength => Self::MeasureLength,
            ActiveTool::MeasurePolylength => Self::MeasurePolylength,
            ActiveTool::MeasurePerimeter => Self::MeasurePerimeter,
            ActiveTool::MeasureArea => Self::MeasureArea,
            ActiveTool::MeasureVolume => Self::MeasureVolume,
            ActiveTool::MeasureAngle => Self::MeasureAngle,
            ActiveTool::MeasureRadius => Self::MeasureRadius,
            ActiveTool::MeasureDiameter => Self::MeasureDiameter,
            ActiveTool::MeasureCount => Self::MeasureCount,
            ActiveTool::CutoutPoly => Self::Cutout,
            ActiveTool::SketchRect | ActiveTool::SketchPoly => Self::SketchScale,
            ActiveTool::ViewportRegion => Self::Viewport,
            ActiveTool::Hyperlink => Self::Hyperlink,
            ActiveTool::Space => Self::Space,
            ActiveTool::VisualSearch => Self::Search,
            ActiveTool::Redaction => Self::Rectangle,
        }
    }
}

pub fn icon_button(
    ui: &mut egui::Ui,
    size: f32,
    kind: IconKind,
    active: bool,
    enabled: bool,
    tooltip: &str,
    theme: &RedBimTheme,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let enabled = enabled && ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, active, tooltip));
    if ui.is_rect_visible(rect) {
        let hover_t = ui
            .ctx()
            .animate_bool_with_time(resp.id.with("hov"), resp.hovered() && enabled, 0.11);
        let active_t = ui
            .ctx()
            .animate_bool_with_time(resp.id.with("act"), active && enabled, 0.13);
        let pressed = resp.is_pointer_button_down_on() && enabled;

        let blend = |a: Color32, b: Color32, t: f32| -> Color32 {
            let t = t.clamp(0.0, 1.0);
            let lerp = |x: u8, y: u8| ((x as f32) + (y as f32 - x as f32) * t).round() as u8;
            Color32::from_rgba_unmultiplied(
                lerp(a.r(), b.r()),
                lerp(a.g(), b.g()),
                lerp(a.b(), b.b()),
                lerp(a.a(), b.a()),
            )
        };

        let bg = if !enabled {
            Color32::TRANSPARENT
        } else if active_t > 0.01 {
            let base_hov = blend(Color32::TRANSPARENT, theme.control_hover, hover_t);
            blend(base_hov, theme.accent_red_soft, active_t)
        } else if hover_t > 0.01 {
            blend(Color32::TRANSPARENT, theme.control_hover, hover_t)
        } else {
            Color32::TRANSPARENT
        };

        let border = if !enabled {
            Stroke::NONE
        } else if resp.has_focus() {
            Stroke::new(2.0, theme.text_primary)
        } else if active_t > 0.05 {
            Stroke::new(
                1.1,
                theme.accent_red.gamma_multiply(0.35 + 0.65 * active_t),
            )
        } else if hover_t > 0.05 {
            Stroke::new(1.0, theme.border_strong.gamma_multiply(hover_t * 0.85))
        } else {
            Stroke::NONE
        };

        let draw_rect = if pressed { rect.shrink(0.75) } else { rect };
        ui.painter().rect(
            draw_rect,
            CornerRadius::same(5),
            bg,
            border,
            egui::StrokeKind::Inside,
        );

        // Crisp crimson accent pill along the bottom edge of active tool buttons
        if active_t > 0.05 && enabled {
            let bar_w = (draw_rect.width() * 0.52 * active_t).max(4.0);
            let bar = Rect::from_center_size(
                Pos2::new(draw_rect.center().x, draw_rect.bottom() - 1.6),
                Vec2::new(bar_w, 2.0),
            );
            ui.painter().rect_filled(
                bar,
                CornerRadius::same(1),
                theme.accent_red.gamma_multiply(active_t),
            );
        }

        let fg = if !enabled {
            theme.text_muted.gamma_multiply(0.42)
        } else {
            blend(theme.text_primary, theme.accent_red, active_t)
        };
        let icon_rect = draw_rect.shrink((size * 0.21).max(4.0));
        paint_icon(ui.painter(), icon_rect, kind, fg, theme.accent_cyan);
    }
    resp.on_hover_text(tooltip)
}

pub fn paint_tool_chest_preview(
    painter: &Painter,
    rect: Rect,
    item: &ToolChestItem,
    _theme: &RedBimTheme,
) {
    let sc = item.style.stroke_color;
    let color = Color32::from_rgba_unmultiplied(sc[0], sc[1], sc[2], 255);
    let icon = IconKind::from_tool(item.tool);
    paint_icon(painter, rect.shrink(4.0), icon, color, color);
}

pub fn paint_icon(painter: &Painter, r: Rect, kind: IconKind, fg: Color32, accent: Color32) {
    let c = r.center();
    let w = r.width();
    let h = r.height();
    let st = Stroke::new(1.5, fg);
    let st_thin = Stroke::new(1.15, fg);
    let st_acc = Stroke::new(1.5, accent);
    let p = |fx: f32, fy: f32| Pos2::new(r.left() + w * fx, r.top() + h * fy);

    match kind {
        IconKind::NewDoc => {
            let pts = vec![p(0.2, 0.1), p(0.65, 0.1), p(0.85, 0.3), p(0.85, 0.9), p(0.2, 0.9)];
            painter.add(egui::Shape::closed_line(pts, st));
            painter.line_segment([p(0.5, 0.45), p(0.5, 0.72)], st_acc);
            painter.line_segment([p(0.37, 0.58), p(0.63, 0.58)], st_acc);
        }
        IconKind::OpenFile => {
            let pts = vec![
                p(0.1, 0.22),
                p(0.42, 0.22),
                p(0.5, 0.35),
                p(0.9, 0.35),
                p(0.9, 0.85),
                p(0.1, 0.85),
            ];
            painter.add(egui::Shape::closed_line(pts, st));
        }
        IconKind::SaveFile => {
            let pts = vec![p(0.15, 0.12), p(0.75, 0.12), p(0.88, 0.25), p(0.88, 0.88), p(0.15, 0.88)];
            painter.add(egui::Shape::closed_line(pts, st));
            painter.rect_stroke(
                Rect::from_min_max(p(0.3, 0.14), p(0.66, 0.38)),
                CornerRadius::ZERO,
                st_thin,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.28, 0.55), p(0.74, 0.86)),
                CornerRadius::ZERO,
                st_thin,
                egui::StrokeKind::Inside,
            );
        }
        IconKind::FlattenPdf => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.18, 0.18), p(0.82, 0.65)),
                CornerRadius::same(1),
                st_thin,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.5, 0.35), p(0.5, 0.85)], st_acc);
            painter.line_segment([p(0.35, 0.72), p(0.5, 0.88)], st_acc);
            painter.line_segment([p(0.65, 0.72), p(0.5, 0.88)], st_acc);
        }
        IconKind::ExportCsv => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.14, 0.16), p(0.86, 0.84)),
                CornerRadius::same(1),
                st,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.14, 0.40), p(0.86, 0.40)], st_thin);
            painter.line_segment([p(0.14, 0.62), p(0.86, 0.62)], st_thin);
            painter.line_segment([p(0.45, 0.16), p(0.45, 0.84)], st_thin);
        }
        IconKind::Undo => {
            let pts = vec![p(0.2, 0.42), p(0.6, 0.42), p(0.8, 0.55), p(0.8, 0.75), p(0.5, 0.8)];
            painter.add(egui::Shape::line(pts, st));
            painter.line_segment([p(0.36, 0.25), p(0.18, 0.42)], st);
            painter.line_segment([p(0.36, 0.59), p(0.18, 0.42)], st);
        }
        IconKind::Redo => {
            let pts = vec![p(0.8, 0.42), p(0.4, 0.42), p(0.2, 0.55), p(0.2, 0.75), p(0.5, 0.8)];
            painter.add(egui::Shape::line(pts, st));
            painter.line_segment([p(0.64, 0.25), p(0.82, 0.42)], st);
            painter.line_segment([p(0.64, 0.59), p(0.82, 0.42)], st);
        }
        IconKind::Select => {
            // Split concave pointer cursor into two convex shapes (head triangle + stem quad)
            let head = vec![p(0.24, 0.12), p(0.24, 0.78), p(0.76, 0.56)];
            let stem = vec![p(0.42, 0.58), p(0.54, 0.54), p(0.68, 0.84), p(0.56, 0.89)];
            painter.add(egui::Shape::convex_polygon(head, fg, Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(stem, fg, Stroke::NONE));
        }
        IconKind::Pan => {
            for dx in [-0.18, -0.05, 0.08, 0.21] {
                painter.line_segment(
                    [Pos2::new(c.x + w * dx, r.top() + h * 0.20), Pos2::new(c.x + w * dx, r.top() + h * 0.55)],
                    st,
                );
            }
            painter.rect_stroke(
                Rect::from_min_max(p(0.25, 0.48), p(0.76, 0.84)),
                CornerRadius::same(3),
                st,
                egui::StrokeKind::Inside,
            );
        }
        IconKind::ZoomWindow => {
            painter.circle_stroke(p(0.43, 0.43), w * 0.26, st);
            painter.line_segment([p(0.62, 0.62), p(0.88, 0.88)], Stroke::new(2.0, fg));
        }
        IconKind::FitPage => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(2), st_thin, egui::StrokeKind::Inside);
            painter.rect_filled(r.shrink(5.0), CornerRadius::same(1), fg.gamma_multiply(0.35));
        }
        IconKind::FitWidth => {
            painter.line_segment([p(0.12, 0.5), p(0.88, 0.5)], st);
            painter.line_segment([p(0.25, 0.34), p(0.12, 0.5)], st);
            painter.line_segment([p(0.25, 0.66), p(0.12, 0.5)], st);
            painter.line_segment([p(0.75, 0.34), p(0.88, 0.5)], st);
            painter.line_segment([p(0.75, 0.66), p(0.88, 0.5)], st);
        }
        IconKind::Note => {
            let pts = vec![p(0.16, 0.16), p(0.84, 0.16), p(0.84, 0.68), p(0.64, 0.86), p(0.16, 0.86)];
            painter.add(egui::Shape::closed_line(pts, st));
            painter.line_segment([p(0.64, 0.68), p(0.84, 0.68)], st_thin);
            painter.line_segment([p(0.64, 0.68), p(0.64, 0.86)], st_thin);
        }
        IconKind::TextBox => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.12, 0.18), p(0.88, 0.82)),
                CornerRadius::same(2),
                st,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.3, 0.36), p(0.7, 0.36)], st);
            painter.line_segment([p(0.5, 0.36), p(0.5, 0.7)], st);
        }
        IconKind::Typewriter => {
            painter.line_segment([p(0.2, 0.25), p(0.8, 0.25)], Stroke::new(2.0, fg));
            painter.line_segment([p(0.5, 0.25), p(0.5, 0.8)], Stroke::new(2.0, fg));
            painter.line_segment([p(0.35, 0.8), p(0.65, 0.8)], st);
        }
        IconKind::Callout => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.42, 0.14), p(0.90, 0.54)),
                CornerRadius::same(2),
                st,
                egui::StrokeKind::Inside,
            );
            painter.add(egui::Shape::line(vec![p(0.42, 0.36), p(0.28, 0.55), p(0.12, 0.84)], st));
            painter.line_segment([p(0.12, 0.84), p(0.27, 0.78)], st);
            painter.line_segment([p(0.12, 0.84), p(0.14, 0.68)], st);
        }
        IconKind::Cloud => {
            for &(fx, fy, fr) in &[
                (0.32, 0.56, 0.18),
                (0.48, 0.42, 0.21),
                (0.68, 0.52, 0.18),
                (0.52, 0.65, 0.18),
            ] {
                painter.circle_stroke(p(fx, fy), w * fr, st_thin);
            }
        }
        IconKind::CloudPlus => {
            for &(fx, fy, fr) in &[(0.32, 0.38, 0.16), (0.50, 0.30, 0.18), (0.68, 0.40, 0.15)] {
                painter.circle_stroke(p(fx, fy), w * fr, st_thin);
            }
            painter.add(egui::Shape::line(vec![p(0.52, 0.52), p(0.52, 0.76), p(0.65, 0.76)], st));
            painter.rect_stroke(
                Rect::from_min_max(p(0.65, 0.62), p(0.92, 0.88)),
                CornerRadius::same(1),
                st,
                egui::StrokeKind::Inside,
            );
        }
        IconKind::Line => {
            painter.line_segment([p(0.16, 0.84), p(0.84, 0.16)], Stroke::new(1.8, fg));
        }
        IconKind::Arrow => {
            painter.line_segment([p(0.16, 0.84), p(0.84, 0.16)], Stroke::new(1.8, fg));
            painter.line_segment([p(0.84, 0.16), p(0.58, 0.18)], st);
            painter.line_segment([p(0.84, 0.16), p(0.82, 0.42)], st);
        }
        IconKind::Polyline => {
            let pts = vec![p(0.14, 0.76), p(0.38, 0.26), p(0.64, 0.66), p(0.86, 0.22)];
            painter.add(egui::Shape::line(pts, st));
        }
        IconKind::Arc => {
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let t = std::f32::consts::PI * (i as f32 / 10.0);
                    Pos2::new(c.x - w * 0.34 * t.cos(), r.bottom() - h * 0.25 - h * 0.45 * t.sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, st));
        }
        IconKind::Rectangle => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.15, 0.22), p(0.85, 0.78)),
                CornerRadius::ZERO,
                st,
                egui::StrokeKind::Inside,
            );
        }
        IconKind::Ellipse => {
            painter.circle_stroke(c, w * 0.34, st);
        }
        IconKind::Polygon => {
            let pts = vec![p(0.5, 0.14), p(0.88, 0.42), p(0.74, 0.86), p(0.26, 0.86), p(0.12, 0.42)];
            painter.add(egui::Shape::closed_line(pts, st));
        }
        IconKind::Ink => {
            let pts = vec![p(0.16, 0.72), p(0.35, 0.35), p(0.55, 0.65), p(0.78, 0.25)];
            painter.add(egui::Shape::line(pts, Stroke::new(1.8, fg)));
        }
        IconKind::Highlight => {
            painter.line_segment([p(0.18, 0.65), p(0.82, 0.35)], Stroke::new(5.5, Color32::from_rgba_unmultiplied(253, 224, 71, 200)));
        }
        IconKind::Stamp => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.12, 0.30), p(0.88, 0.74)),
                CornerRadius::same(2),
                st,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.24, 0.52), p(0.76, 0.52)], st_acc);
        }
        IconKind::Image => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(1), st, egui::StrokeKind::Inside);
            painter.add(egui::Shape::line(vec![p(0.2, 0.75), p(0.45, 0.46), p(0.62, 0.62), p(0.8, 0.4)], st_thin));
            painter.circle_filled(p(0.34, 0.34), 2.0, fg);
        }
        IconKind::Snapshot => {
            painter.rect_stroke(r.shrink(2.5), CornerRadius::same(2), st, egui::StrokeKind::Inside);
            painter.circle_stroke(c, w * 0.16, st_acc);
        }
        IconKind::Legend => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(1), st, egui::StrokeKind::Inside);
            painter.line_segment([p(0.22, 0.36), p(0.78, 0.36)], st_acc);
            painter.line_segment([p(0.22, 0.56), p(0.78, 0.56)], st_thin);
            painter.line_segment([p(0.22, 0.74), p(0.64, 0.74)], st_thin);
        }
        IconKind::Calibrate => {
            painter.line_segment([p(0.14, 0.76), p(0.86, 0.24)], Stroke::new(1.8, accent));
            painter.circle_filled(p(0.14, 0.76), 2.6, fg);
            painter.circle_filled(p(0.86, 0.24), 2.6, fg);
            painter.line_segment([p(0.32, 0.22), p(0.68, 0.22)], st_thin);
        }
        IconKind::MeasureLength => {
            painter.line_segment([p(0.14, 0.5), p(0.86, 0.5)], st_acc);
            painter.line_segment([p(0.14, 0.32), p(0.14, 0.68)], st);
            painter.line_segment([p(0.86, 0.32), p(0.86, 0.68)], st);
        }
        IconKind::MeasurePolylength => {
            let pts = vec![p(0.14, 0.72), p(0.40, 0.28), p(0.66, 0.66), p(0.88, 0.24)];
            painter.add(egui::Shape::line(pts.clone(), st_acc));
            for pt in pts {
                painter.circle_filled(pt, 2.0, fg);
            }
        }
        IconKind::MeasurePerimeter => {
            let pts = vec![p(0.18, 0.22), p(0.82, 0.22), p(0.82, 0.78), p(0.18, 0.78)];
            painter.add(egui::Shape::closed_line(pts, st_acc));
        }
        IconKind::MeasureArea => {
            let rect = Rect::from_min_max(p(0.16, 0.22), p(0.84, 0.78));
            painter.rect_filled(rect, CornerRadius::ZERO, accent.gamma_multiply(0.28));
            painter.rect_stroke(rect, CornerRadius::ZERO, st_acc, egui::StrokeKind::Inside);
            painter.line_segment([p(0.20, 0.74), p(0.76, 0.26)], st_thin);
        }
        IconKind::MeasureVolume => {
            let front = Rect::from_min_max(p(0.14, 0.36), p(0.68, 0.86));
            painter.rect_filled(front, CornerRadius::ZERO, accent.gamma_multiply(0.22));
            painter.rect_stroke(front, CornerRadius::ZERO, st_acc, egui::StrokeKind::Inside);
            painter.add(egui::Shape::line(vec![p(0.14, 0.36), p(0.36, 0.16), p(0.88, 0.16), p(0.88, 0.66), p(0.68, 0.86)], st));
            painter.line_segment([p(0.68, 0.36), p(0.88, 0.16)], st);
        }
        IconKind::MeasureAngle => {
            painter.add(egui::Shape::line(vec![p(0.82, 0.24), p(0.18, 0.76), p(0.86, 0.76)], st_acc));
            painter.circle_stroke(p(0.32, 0.68), w * 0.14, st_thin);
        }
        IconKind::MeasureRadius => {
            painter.circle_stroke(c, w * 0.34, st_acc);
            painter.line_segment([c, p(0.76, 0.28)], st);
            painter.circle_filled(c, 2.0, fg);
        }
        IconKind::MeasureDiameter => {
            painter.circle_stroke(c, w * 0.34, st_acc);
            painter.line_segment([p(0.22, 0.78), p(0.78, 0.22)], st);
        }
        IconKind::MeasureCount => {
            painter.add(egui::Shape::line(vec![p(0.18, 0.56), p(0.40, 0.78), p(0.84, 0.24)], Stroke::new(2.0, accent)));
        }
        IconKind::Cutout => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::ZERO, st, egui::StrokeKind::Inside);
            painter.rect_stroke(r.shrink(5.5), CornerRadius::ZERO, st_acc, egui::StrokeKind::Inside);
        }
        IconKind::SketchScale => {
            painter.rect_stroke(r.shrink(2.5), CornerRadius::ZERO, st_acc, egui::StrokeKind::Inside);
            painter.line_segment([p(0.2, 0.2), p(0.8, 0.2)], st);
        }
        IconKind::Viewport => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(2), st_acc, egui::StrokeKind::Inside);
            painter.line_segment([p(0.15, 0.5), p(0.85, 0.5)], st_thin);
            painter.line_segment([p(0.5, 0.15), p(0.5, 0.85)], st_thin);
        }
        IconKind::ToolChest => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.12, 0.30), p(0.88, 0.84)),
                CornerRadius::same(2),
                st,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.36, 0.16), p(0.64, 0.30)),
                CornerRadius::same(1),
                st,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.12, 0.52), p(0.88, 0.52)], st_thin);
        }
        IconKind::Thumbnails => {
            for (x0, y0) in [(0.16, 0.16), (0.56, 0.16), (0.16, 0.56), (0.56, 0.56)] {
                painter.rect_stroke(
                    Rect::from_min_max(p(x0, y0), p(x0 + 0.28, y0 + 0.28)),
                    CornerRadius::same(1),
                    st_thin,
                    egui::StrokeKind::Inside,
                );
            }
        }
        IconKind::Layers => {
            for dy in [-0.20, 0.0, 0.20] {
                let pts = vec![
                    Pos2::new(c.x, c.y + h * (dy - 0.12)),
                    Pos2::new(c.x + w * 0.36, c.y + h * dy),
                    Pos2::new(c.x, c.y + h * (dy + 0.12)),
                    Pos2::new(c.x - w * 0.36, c.y + h * dy),
                ];
                painter.add(egui::Shape::closed_line(pts, st_thin));
            }
        }
        IconKind::Bookmarks => {
            let pts = vec![p(0.24, 0.14), p(0.76, 0.14), p(0.76, 0.86), p(0.50, 0.66), p(0.24, 0.86)];
            painter.add(egui::Shape::closed_line(pts, st));
        }
        IconKind::Duplicate => {
            painter.line_segment([p(0.52, 0.15), p(0.15, 0.15)], st);
            painter.line_segment([p(0.15, 0.15), p(0.15, 0.59)], st);
            painter.rect_stroke(
                Rect::from_min_max(p(0.36, 0.35), p(0.86, 0.86)),
                CornerRadius::same(2), st, egui::StrokeKind::Inside,
            );
        }
        IconKind::Lock => {
            painter.add(egui::Shape::line(
                vec![p(0.31, 0.47), p(0.31, 0.29), p(0.39, 0.17), p(0.61, 0.17), p(0.69, 0.29), p(0.69, 0.47)],
                st,
            ));
            painter.rect_stroke(
                Rect::from_min_max(p(0.22, 0.47), p(0.78, 0.86)),
                CornerRadius::same(2), st, egui::StrokeKind::Inside,
            );
        }
        IconKind::Trash => {
            painter.line_segment([p(0.17, 0.28), p(0.83, 0.28)], st);
            painter.line_segment([p(0.39, 0.14), p(0.61, 0.14)], st);
            painter.add(egui::Shape::line(
                vec![p(0.27, 0.35), p(0.32, 0.85), p(0.68, 0.85), p(0.73, 0.35)],
                st,
            ));
            painter.line_segment([p(0.50, 0.43), p(0.50, 0.73)], st_thin);
        }
        IconKind::Properties => {
            for fy in [0.28, 0.50, 0.72] {
                painter.line_segment([p(0.16, fy), p(0.84, fy)], st_thin);
            }
            painter.circle_filled(p(0.34, 0.28), 2.5, accent);
            painter.circle_filled(p(0.66, 0.50), 2.5, accent);
            painter.circle_filled(p(0.42, 0.72), 2.5, accent);
        }
        IconKind::Measurements => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.12, 0.32), p(0.88, 0.68)),
                CornerRadius::same(1),
                st_acc,
                egui::StrokeKind::Inside,
            );
            for fx in [0.28, 0.44, 0.60, 0.74] {
                painter.line_segment([p(fx, 0.32), p(fx, 0.50)], st_thin);
            }
        }
        IconKind::Search => {
            painter.circle_stroke(p(0.42, 0.42), w * 0.24, st);
            painter.line_segment([p(0.60, 0.60), p(0.86, 0.86)], Stroke::new(1.8, fg));
        }
        IconKind::MarkupsList => {
            for fy in [0.26, 0.50, 0.74] {
                painter.circle_filled(p(0.20, fy), 2.0, accent);
                painter.line_segment([p(0.32, fy), p(0.84, fy)], st);
            }
        }
        IconKind::SplitVertical => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(1), st, egui::StrokeKind::Inside);
            painter.line_segment([p(0.5, 0.15), p(0.5, 0.85)], st_acc);
        }
        IconKind::SplitHorizontal => {
            painter.rect_stroke(r.shrink(2.0), CornerRadius::same(1), st, egui::StrokeKind::Inside);
            painter.line_segment([p(0.15, 0.5), p(0.85, 0.5)], st_acc);
        }
        IconKind::SnapContent => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.22, 0.22), p(0.78, 0.78)),
                CornerRadius::ZERO,
                st_acc,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(p(0.22, 0.22), 2.2, fg);
        }
        IconKind::SnapMarkup => {
            let pts = vec![p(0.5, 0.14), p(0.86, 0.5), p(0.5, 0.86), p(0.14, 0.5)];
            painter.add(egui::Shape::closed_line(pts, st_acc));
        }
        IconKind::SnapGrid => {
            for f in [0.28, 0.50, 0.72] {
                painter.line_segment([p(f, 0.16), p(f, 0.84)], st_thin);
                painter.line_segment([p(0.16, f), p(0.84, f)], st_thin);
            }
        }
        IconKind::Dimmer => {
            painter.circle_stroke(c, w * 0.28, st);
            painter.circle_filled(c, w * 0.16, fg.gamma_multiply(0.5));
        }
        IconKind::Reuse => {
            painter.circle_stroke(c, w * 0.28, st_acc);
            painter.line_segment([p(0.76, 0.34), p(0.78, 0.56)], st_acc);
        }
        IconKind::ThemeToggle => {
            painter.circle_stroke(c, w * 0.26, st);
            painter.circle_filled(p(0.45, 0.5), w * 0.12, fg);
        }
        IconKind::Hyperlink => {
            let left = Rect::from_min_max(p(0.14, 0.34), p(0.56, 0.66));
            let right = Rect::from_min_max(p(0.44, 0.34), p(0.86, 0.66));
            painter.rect_stroke(left, CornerRadius::same(5), st_acc, egui::StrokeKind::Inside);
            painter.rect_stroke(right, CornerRadius::same(5), st, egui::StrokeKind::Inside);
            painter.line_segment([p(0.38, 0.50), p(0.62, 0.50)], st_acc);
        }
        IconKind::Space => {
            let pts = vec![p(0.16, 0.22), p(0.84, 0.22), p(0.84, 0.78), p(0.46, 0.78), p(0.46, 0.54), p(0.16, 0.54)];
            painter.add(egui::Shape::closed_line(pts, st_acc));
            painter.rect_filled(
                Rect::from_min_max(p(0.24, 0.30), p(0.76, 0.48)),
                CornerRadius::same(1),
                accent.gamma_multiply(0.25),
            );
        }
    }
}

pub const REDBIM_LOGO_PNG: &[u8] = include_bytes!("../../assets/redbim_icon.png");

pub fn generate_app_icon() -> egui::IconData {
    if let Ok(img) = image::load_from_memory(REDBIM_LOGO_PNG) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        return egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        };
    }
    egui::IconData {
        rgba: vec![220, 38, 38, 255],
        width: 1,
        height: 1,
    }
}
