use crate::geometry::math::{
    Point2, Rect2, dimension_offset_segment, dist_point_to_segment, point_in_polygon_with_cutouts,
};
use crate::model::markup::{Markup, MarkupKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    Pos,
    LineStart,
    LineEnd,
    MeasureOffset,
    ArcMid,
    RectCorner(usize), // 0=TL, 1=TR, 2=BR, 3=BL
    PointIndex(usize),
    CutoutPoint(usize, usize),
    CalloutTip,
    CalloutKnee,
    CalloutBoxCorner(usize),
    AngleP1,
    AngleVertex,
    AngleP2,
    RadiusCenter,
    RadiusEdge,
}

pub fn markup_bounds(m: &Markup) -> Rect2 {
    match &m.kind {
        MarkupKind::Note { pos } => Rect2::from_xywh(pos.x - 10.0, pos.y - 10.0, 20.0, 20.0),
        MarkupKind::Typewriter { pos, text } => {
            let lines = text.lines().count().max(1) as f64;
            let max_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(6) as f64;
            let fs = m.style.font_size as f64;
            Rect2::from_xywh(
                pos.x,
                pos.y,
                (max_chars * fs * 0.58).max(40.0),
                lines * fs * 1.35,
            )
        }
        MarkupKind::TextBox { rect, .. }
        | MarkupKind::Rectangle { rect }
        | MarkupKind::Ellipse { rect }
        | MarkupKind::Stamp { rect, .. }
        | MarkupKind::Image { rect, .. }
        | MarkupKind::SketchRect { rect, .. } => *rect,
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            ..
        } => {
            let mut r = *box_rect;
            for p in [*arrow_tip, *knee] {
                r.min_x = r.min_x.min(p.x);
                r.min_y = r.min_y.min(p.y);
                r.max_x = r.max_x.max(p.x);
                r.max_y = r.max_y.max(p.y);
            }
            r
        }
        MarkupKind::Cloud { points, .. }
        | MarkupKind::Polyline { points }
        | MarkupKind::Polygon { points }
        | MarkupKind::MeasurePolylength { points, .. }
        | MarkupKind::MeasurePerimeter { points, .. }
        | MarkupKind::MeasureArea { points, .. }
        | MarkupKind::MeasureVolume { points, .. }
        | MarkupKind::MeasureCount { points, .. }
        | MarkupKind::SketchPoly { points, .. } => Rect2::from_points(points).unwrap_or_default(),
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            ..
        } => {
            let mut r = Rect2::from_points(cloud_points).unwrap_or(*box_rect);
            r.min_x = r.min_x.min(knee.x).min(box_rect.min_x);
            r.min_y = r.min_y.min(knee.y).min(box_rect.min_y);
            r.max_x = r.max_x.max(knee.x).max(box_rect.max_x);
            r.max_y = r.max_y.max(knee.y).max(box_rect.max_y);
            r
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureDiameter { p1: start, p2: end, .. } => Rect2::from_min_max(*start, *end),
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            Rect2::from_points(&[*start, *end, da, db]).unwrap_or_default()
        }
        MarkupKind::Arc { start, mid, end } => {
            Rect2::from_points(&[*start, *mid, *end]).unwrap_or_default()
        }
        MarkupKind::Ink { strokes } | MarkupKind::Highlight { strokes, .. } => {
            let all: Vec<Point2> = strokes.iter().flatten().copied().collect();
            Rect2::from_points(&all).unwrap_or_default()
        }
        MarkupKind::Legend { pos, width_pt, .. } => {
            Rect2::from_xywh(pos.x, pos.y, *width_pt, 120.0)
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            Rect2::from_points(&[*p1, *vertex, *p2]).unwrap_or_default()
        }
        MarkupKind::MeasureRadius { center, edge, .. } => {
            let rad = center.dist(*edge);
            Rect2::from_xywh(center.x - rad, center.y - rad, rad * 2.0, rad * 2.0)
        }
    }
}

pub fn hit_test_markup(m: &Markup, pt: Point2, tol_pt: f64) -> bool {
    let bounds = markup_bounds(m).expand(tol_pt + 4.0);
    if !bounds.contains(pt) {
        return false;
    }

    match &m.kind {
        MarkupKind::Note { pos } => pt.dist(*pos) <= 12.0 + tol_pt,
        MarkupKind::TextBox { rect, .. }
        | MarkupKind::Stamp { rect, .. }
        | MarkupKind::Image { rect, .. } => rect.expand(tol_pt).contains(pt),
        MarkupKind::Typewriter { .. } | MarkupKind::Legend { .. } => {
            markup_bounds(m).expand(tol_pt).contains(pt)
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            ..
        } => {
            box_rect.expand(tol_pt).contains(pt)
                || dist_point_to_segment(pt, *arrow_tip, *knee) <= tol_pt
                || dist_point_to_segment(pt, *knee, box_rect_anchor(*box_rect, *knee)) <= tol_pt
        }
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            ..
        } => {
            box_rect.expand(tol_pt).contains(pt)
                || hit_polygon_outline_or_fill(cloud_points, &[], pt, tol_pt, m.style.fill_color.is_some())
                || dist_point_to_segment(pt, *knee, box_rect_anchor(*box_rect, *knee)) <= tol_pt
        }
        MarkupKind::Cloud { points, .. } => {
            hit_polygon_outline_or_fill(points, &[], pt, tol_pt, m.style.fill_color.is_some())
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureDiameter { p1: start, p2: end, .. } => {
            dist_point_to_segment(pt, *start, *end) <= tol_pt
        }
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            let pill_mid = da.midpoint(db);
            dist_point_to_segment(pt, da, db) <= tol_pt
                || dist_point_to_segment(pt, *start, da) <= tol_pt
                || dist_point_to_segment(pt, *end, db) <= tol_pt
                || dist_point_to_segment(pt, *start, *end) <= tol_pt
                || pt.dist(pill_mid) <= (tol_pt * 2.5).max(16.0)
        }
        MarkupKind::Polyline { points } | MarkupKind::MeasurePolylength { points, .. } => {
            points.windows(2).any(|w| dist_point_to_segment(pt, w[0], w[1]) <= tol_pt)
        }
        MarkupKind::Arc { start, mid, end } => {
            let samples = crate::geometry::math::sample_three_point_arc(*start, *mid, *end, 16);
            samples.windows(2).any(|w| dist_point_to_segment(pt, w[0], w[1]) <= tol_pt)
        }
        MarkupKind::Rectangle { rect } | MarkupKind::SketchRect { rect, .. } => {
            if m.style.fill_color.is_some() {
                rect.expand(tol_pt).contains(pt)
            } else {
                let c = rect.corners();
                hit_polygon_outline_or_fill(&c, &[], pt, tol_pt, false)
            }
        }
        MarkupKind::Ellipse { rect } => {
            let c = rect.center();
            let rx = (rect.width() * 0.5).max(1.0);
            let ry = (rect.height() * 0.5).max(1.0);
            let norm = ((pt.x - c.x) / rx).powi(2) + ((pt.y - c.y) / ry).powi(2);
            if m.style.fill_color.is_some() {
                norm <= 1.15
            } else {
                (norm.sqrt() - 1.0).abs() * rx.min(ry) <= tol_pt
            }
        }
        MarkupKind::Polygon { points } | MarkupKind::SketchPoly { points, .. } => {
            hit_polygon_outline_or_fill(points, &[], pt, tol_pt, m.style.fill_color.is_some())
        }
        MarkupKind::MeasurePerimeter { points, closed, .. } => {
            if *closed {
                hit_polygon_outline_or_fill(points, &[], pt, tol_pt, false)
            } else {
                points.windows(2).any(|w| dist_point_to_segment(pt, w[0], w[1]) <= tol_pt)
            }
        }
        MarkupKind::MeasureArea {
            points, cutouts, ..
        }
        | MarkupKind::MeasureVolume {
            points, cutouts, ..
        } => hit_polygon_outline_or_fill(points, cutouts, pt, tol_pt, true),
        MarkupKind::Ink { strokes } | MarkupKind::Highlight { strokes, .. } => {
            let extra = if matches!(m.kind, MarkupKind::Highlight { .. }) {
                (m.style.line_width as f64 * 0.5).max(tol_pt)
            } else {
                tol_pt
            };
            strokes.iter().any(|s| {
                s.windows(2)
                    .any(|w| dist_point_to_segment(pt, w[0], w[1]) <= extra)
            })
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            dist_point_to_segment(pt, *p1, *vertex) <= tol_pt
                || dist_point_to_segment(pt, *vertex, *p2) <= tol_pt
        }
        MarkupKind::MeasureRadius { center, edge, .. } => {
            let r = center.dist(*edge);
            let d = pt.dist(*center);
            (d - r).abs() <= tol_pt || dist_point_to_segment(pt, *center, *edge) <= tol_pt
        }
        MarkupKind::MeasureCount { points, .. } => {
            points.iter().any(|p| p.dist(pt) <= 10.0 + tol_pt)
        }
    }
}

pub fn box_rect_anchor(rect: Rect2, from: Point2) -> Point2 {
    let c = rect.center();
    if from.x <= c.x {
        Point2::new(rect.min_x, c.y)
    } else {
        Point2::new(rect.max_x, c.y)
    }
}

fn hit_polygon_outline_or_fill(
    points: &[Point2],
    cutouts: &[Vec<Point2>],
    pt: Point2,
    tol_pt: f64,
    allow_fill: bool,
) -> bool {
    if points.len() >= 3 && allow_fill && point_in_polygon_with_cutouts(pt, points, cutouts) {
        return true;
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if dist_point_to_segment(pt, a, b) <= tol_pt {
            return true;
        }
    }
    false
}

pub fn list_handles(m: &Markup) -> Vec<(HandleKind, Point2)> {
    let mut out = Vec::new();
    match &m.kind {
        MarkupKind::Note { pos }
        | MarkupKind::Typewriter { pos, .. }
        | MarkupKind::Legend { pos, .. } => {
            out.push((HandleKind::Pos, *pos));
        }
        MarkupKind::TextBox { rect, .. }
        | MarkupKind::Rectangle { rect }
        | MarkupKind::Ellipse { rect }
        | MarkupKind::Stamp { rect, .. }
        | MarkupKind::Image { rect, .. }
        | MarkupKind::SketchRect { rect, .. } => {
            for (i, c) in rect.corners().iter().enumerate() {
                out.push((HandleKind::RectCorner(i), *c));
            }
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            ..
        } => {
            out.push((HandleKind::CalloutTip, *arrow_tip));
            out.push((HandleKind::CalloutKnee, *knee));
            for (i, c) in box_rect.corners().iter().enumerate() {
                out.push((HandleKind::CalloutBoxCorner(i), *c));
            }
        }
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            ..
        } => {
            for (i, p) in cloud_points.iter().enumerate() {
                out.push((HandleKind::PointIndex(i), *p));
            }
            out.push((HandleKind::CalloutKnee, *knee));
            for (i, c) in box_rect.corners().iter().enumerate() {
                out.push((HandleKind::CalloutBoxCorner(i), *c));
            }
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureDiameter { p1: start, p2: end, .. } => {
            out.push((HandleKind::LineStart, *start));
            out.push((HandleKind::LineEnd, *end));
        }
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            out.push((HandleKind::LineStart, *start));
            out.push((HandleKind::LineEnd, *end));
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            // Place the offset handle at 25% along the dimension line so it never obscures the centered caption pill
            let handle_pt = Point2::new(
                da.x + (db.x - da.x) * 0.25,
                da.y + (db.y - da.y) * 0.25,
            );
            out.push((HandleKind::MeasureOffset, handle_pt));
        }
        MarkupKind::Arc { start, mid, end } => {
            out.push((HandleKind::LineStart, *start));
            out.push((HandleKind::ArcMid, *mid));
            out.push((HandleKind::LineEnd, *end));
        }
        MarkupKind::Cloud { points, .. }
        | MarkupKind::Polyline { points }
        | MarkupKind::Polygon { points }
        | MarkupKind::MeasurePolylength { points, .. }
        | MarkupKind::MeasurePerimeter { points, .. }
        | MarkupKind::MeasureCount { points, .. }
        | MarkupKind::SketchPoly { points, .. } => {
            for (i, p) in points.iter().enumerate() {
                out.push((HandleKind::PointIndex(i), *p));
            }
        }
        MarkupKind::MeasureArea {
            points, cutouts, ..
        }
        | MarkupKind::MeasureVolume {
            points, cutouts, ..
        } => {
            for (i, p) in points.iter().enumerate() {
                out.push((HandleKind::PointIndex(i), *p));
            }
            for (ci, hole) in cutouts.iter().enumerate() {
                for (pi, p) in hole.iter().enumerate() {
                    out.push((HandleKind::CutoutPoint(ci, pi), *p));
                }
            }
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            out.push((HandleKind::AngleP1, *p1));
            out.push((HandleKind::AngleVertex, *vertex));
            out.push((HandleKind::AngleP2, *p2));
        }
        MarkupKind::MeasureRadius { center, edge, .. } => {
            out.push((HandleKind::RadiusCenter, *center));
            out.push((HandleKind::RadiusEdge, *edge));
        }
        _ => {}
    }
    out
}

pub fn hit_test_handle(m: &Markup, pt: Point2, tol_pt: f64) -> Option<HandleKind> {
    for (kind, pos) in list_handles(m) {
        if pos.dist(pt) <= tol_pt {
            return Some(kind);
        }
    }
    None
}

/// Returns true when `pt` hits the visible dimension line or caption of a `MeasureLength`
/// (away from its endpoint handles), so dragging adjusts `offset_pt` while keeping measured points fixed.
pub fn is_measure_length_offset_drag_hit(m: &Markup, pt: Point2, tol_pt: f64) -> bool {
    let MarkupKind::MeasureLength {
        start,
        end,
        offset_pt,
        ..
    } = &m.kind
    else {
        return false;
    };
    if pt.dist(*start) <= tol_pt * 1.2 || pt.dist(*end) <= tol_pt * 1.2 {
        return false;
    }
    let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
    let pill_mid = da.midpoint(db);
    dist_point_to_segment(pt, da, db) <= tol_pt * 1.25
        || pt.dist(pill_mid) <= (tol_pt * 2.5).max(16.0)
}

pub fn apply_handle_drag(m: &mut Markup, handle: HandleKind, new_pt: Point2) {
    m.modified_at = chrono::Utc::now();
    match (&mut m.kind, handle) {
        (
            MarkupKind::Note { pos }
            | MarkupKind::Typewriter { pos, .. }
            | MarkupKind::Legend { pos, .. },
            HandleKind::Pos,
        ) => {
            *pos = new_pt;
        }
        (
            MarkupKind::TextBox { rect, .. }
            | MarkupKind::Rectangle { rect }
            | MarkupKind::Ellipse { rect }
            | MarkupKind::Stamp { rect, .. }
            | MarkupKind::Image { rect, .. }
            | MarkupKind::SketchRect { rect, .. },
            HandleKind::RectCorner(corner),
        ) => {
            resize_rect_by_corner(rect, corner, new_pt);
        }
        (
            MarkupKind::Callout {
                arrow_tip,
                knee,
                box_rect,
                ..
            },
            h,
        ) => match h {
            HandleKind::CalloutTip => *arrow_tip = new_pt,
            HandleKind::CalloutKnee => *knee = new_pt,
            HandleKind::CalloutBoxCorner(c) => resize_rect_by_corner(box_rect, c, new_pt),
            _ => {}
        },
        (
            MarkupKind::CloudPlus {
                cloud_points,
                knee,
                box_rect,
                ..
            },
            h,
        ) => match h {
            HandleKind::PointIndex(i) if i < cloud_points.len() => cloud_points[i] = new_pt,
            HandleKind::CalloutKnee => *knee = new_pt,
            HandleKind::CalloutBoxCorner(c) => resize_rect_by_corner(box_rect, c, new_pt),
            _ => {}
        },
        (
            MarkupKind::Line { start, end }
            | MarkupKind::Arrow { start, end }
            | MarkupKind::MeasureDiameter {
                p1: start,
                p2: end,
                ..
            },
            h,
        ) => match h {
            HandleKind::LineStart => *start = new_pt,
            HandleKind::LineEnd => *end = new_pt,
            _ => {}
        },
        (
            MarkupKind::MeasureLength {
                start,
                end,
                offset_pt,
                ..
            },
            h,
        ) => match h {
            HandleKind::LineStart => *start = new_pt,
            HandleKind::LineEnd => *end = new_pt,
            HandleKind::MeasureOffset => {
                let d = *end - *start;
                let len = d.len().max(1e-6);
                let n = Point2::new(-d.y / len, d.x / len);
                *offset_pt = (new_pt - *start).dot(n);
            }
            _ => {}
        },
        (MarkupKind::Arc { start, mid, end }, h) => match h {
            HandleKind::LineStart => *start = new_pt,
            HandleKind::ArcMid => *mid = new_pt,
            HandleKind::LineEnd => *end = new_pt,
            _ => {}
        },
        (
            MarkupKind::Cloud { points, .. }
            | MarkupKind::Polyline { points }
            | MarkupKind::Polygon { points }
            | MarkupKind::MeasurePolylength { points, .. }
            | MarkupKind::MeasurePerimeter { points, .. }
            | MarkupKind::MeasureCount { points, .. }
            | MarkupKind::SketchPoly { points, .. },
            HandleKind::PointIndex(i),
        ) if i < points.len() => {
            points[i] = new_pt;
        }
        (
            MarkupKind::MeasureArea {
                points, cutouts, ..
            }
            | MarkupKind::MeasureVolume {
                points, cutouts, ..
            },
            h,
        ) => match h {
            HandleKind::PointIndex(i) if i < points.len() => points[i] = new_pt,
            HandleKind::CutoutPoint(ci, pi)
                if ci < cutouts.len() && pi < cutouts[ci].len() =>
            {
                cutouts[ci][pi] = new_pt;
            }
            _ => {}
        },
        (MarkupKind::MeasureAngle { p1, vertex, p2, .. }, h) => match h {
            HandleKind::AngleP1 => *p1 = new_pt,
            HandleKind::AngleVertex => *vertex = new_pt,
            HandleKind::AngleP2 => *p2 = new_pt,
            _ => {}
        },
        (
            MarkupKind::MeasureRadius {
                center,
                edge,
                three_point,
                ..
            },
            h,
        ) => {
            *three_point = None;
            match h {
                HandleKind::RadiusCenter => *center = new_pt,
                HandleKind::RadiusEdge => *edge = new_pt,
                _ => {}
            }
        }
        _ => {}
    }
}

fn resize_rect_by_corner(rect: &mut Rect2, corner: usize, new_pt: Point2) {
    let opposite = match corner {
        0 => Point2::new(rect.max_x, rect.max_y),
        1 => Point2::new(rect.min_x, rect.max_y),
        2 => Point2::new(rect.min_x, rect.min_y),
        _ => Point2::new(rect.max_x, rect.min_y),
    };
    *rect = Rect2::from_min_max(opposite, new_pt);
}

pub fn translate_markup(m: &mut Markup, delta: Point2) {
    if m.locked {
        return;
    }
    m.modified_at = chrono::Utc::now();
    match &mut m.kind {
        MarkupKind::Note { pos }
        | MarkupKind::Typewriter { pos, .. }
        | MarkupKind::Legend { pos, .. } => {
            *pos = *pos + delta;
        }
        MarkupKind::TextBox { rect, .. }
        | MarkupKind::Rectangle { rect }
        | MarkupKind::Ellipse { rect }
        | MarkupKind::Stamp { rect, .. }
        | MarkupKind::Image { rect, .. }
        | MarkupKind::SketchRect { rect, .. } => {
            *rect = rect.translate(delta);
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            ..
        } => {
            *arrow_tip = *arrow_tip + delta;
            *knee = *knee + delta;
            *box_rect = box_rect.translate(delta);
        }
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            ..
        } => {
            for p in cloud_points {
                *p = *p + delta;
            }
            *knee = *knee + delta;
            *box_rect = box_rect.translate(delta);
        }
        MarkupKind::Cloud { points, .. }
        | MarkupKind::Polyline { points }
        | MarkupKind::Polygon { points }
        | MarkupKind::MeasurePolylength { points, .. }
        | MarkupKind::MeasurePerimeter { points, .. }
        | MarkupKind::MeasureCount { points, .. }
        | MarkupKind::SketchPoly { points, .. } => {
            for p in points {
                *p = *p + delta;
            }
        }
        MarkupKind::MeasureArea {
            points, cutouts, ..
        }
        | MarkupKind::MeasureVolume {
            points, cutouts, ..
        } => {
            for p in points {
                *p = *p + delta;
            }
            for c in cutouts {
                for p in c {
                    *p = *p + delta;
                }
            }
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureLength { start, end, .. }
        | MarkupKind::MeasureDiameter { p1: start, p2: end, .. } => {
            *start = *start + delta;
            *end = *end + delta;
        }
        MarkupKind::Arc { start, mid, end } => {
            *start = *start + delta;
            *mid = *mid + delta;
            *end = *end + delta;
        }
        MarkupKind::Ink { strokes } | MarkupKind::Highlight { strokes, .. } => {
            for s in strokes {
                for p in s {
                    *p = *p + delta;
                }
            }
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            *p1 = *p1 + delta;
            *vertex = *vertex + delta;
            *p2 = *p2 + delta;
        }
        MarkupKind::MeasureRadius {
            center,
            edge,
            three_point,
            ..
        } => {
            *center = *center + delta;
            *edge = *edge + delta;
            if let Some(arr) = three_point {
                for p in arr {
                    *p = *p + delta;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::markup::{MarkupStyle};

    #[test]
    fn hit_test_and_drag_callout_work() {
        let mut m = Markup::new(
            0,
            "Obs",
            "Autor",
            MarkupStyle::default(),
            MarkupKind::Callout {
                arrow_tip: Point2::new(100.0, 100.0),
                knee: Point2::new(140.0, 80.0),
                box_rect: Rect2::from_xywh(160.0, 65.0, 100.0, 30.0),
                text: "Revisar".to_string(),
            },
        );
        assert!(hit_test_markup(&m, Point2::new(200.0, 80.0), 6.0));
        assert!(hit_test_markup(&m, Point2::new(120.0, 90.0), 6.0));
        let h = hit_test_handle(&m, Point2::new(101.0, 99.0), 6.0).unwrap();
        assert_eq!(h, HandleKind::CalloutTip);
        apply_handle_drag(&mut m, h, Point2::new(50.0, 50.0));
        if let MarkupKind::Callout { arrow_tip, .. } = m.kind {
            assert_eq!(arrow_tip, Point2::new(50.0, 50.0));
        } else {
            panic!("wrong kind");
        }
    }

    #[test]
    fn measure_length_offset_hit_and_bounds_work() {
        use crate::model::markup::MeasurementOptions;
        let m = Markup::new(
            0,
            "Longitud",
            "Autor",
            MarkupStyle::default(),
            MarkupKind::MeasureLength {
                start: Point2::new(100.0, 100.0),
                end: Point2::new(100.0, 300.0),
                offset_pt: 25.0,
                opts: MeasurementOptions::default(),
            },
        );
        // Offset dimension line is at x = 75.0 (left normal of (0, 200) is (-1, 0))
        let b = markup_bounds(&m);
        assert!(b.min_x <= 75.1 && b.max_x >= 99.9);
        assert!(hit_test_markup(&m, Point2::new(75.0, 200.0), 5.0));
        assert!(is_measure_length_offset_drag_hit(
            &m,
            Point2::new(75.0, 200.0),
            5.0
        ));
        assert!(!is_measure_length_offset_drag_hit(
            &m,
            Point2::new(100.0, 100.0),
            5.0
        ));
    }
}
