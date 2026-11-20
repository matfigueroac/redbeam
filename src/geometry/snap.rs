use crate::geometry::math::Point2;
use crate::model::markup::{Markup, MarkupKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
    MarkupVertex,
    Grid,
}

impl SnapKind {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::Endpoint => "Punto final",
            Self::Midpoint => "Punto medio",
            Self::Center => "Centro",
            Self::MarkupVertex => "Vértice marca",
            Self::Grid => "Grilla",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapResult {
    pub point: Point2,
    pub kind: SnapKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapSettings {
    pub snap_to_content: bool,
    pub snap_to_markup: bool,
    pub snap_to_grid: bool,
    pub ortho_45: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            snap_to_content: true,
            snap_to_markup: true,
            snap_to_grid: false,
            ortho_45: false,
        }
    }
}

/// Spatial grid index of PDF vector endpoints, midpoints and segments for fast O(1) snapping.
#[derive(Debug, Clone, Default)]
pub struct SnapIndex {
    cell_size: f64,
    points: HashMap<(i32, i32), Vec<(Point2, SnapKind)>>,
    pub segment_count: usize,
}

impl SnapIndex {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size: cell_size.max(4.0),
            points: HashMap::new(),
            segment_count: 0,
        }
    }

    #[inline]
    fn cell_of(&self, p: Point2) -> (i32, i32) {
        (
            (p.x / self.cell_size).floor() as i32,
            (p.y / self.cell_size).floor() as i32,
        )
    }

    pub fn insert_point(&mut self, pt: Point2, kind: SnapKind) {
        if !pt.x.is_finite() || !pt.y.is_finite() {
            return;
        }
        let cell = self.cell_of(pt);
        let bucket = self.points.entry(cell).or_default();
        // Avoid duplicates within 0.25 pt
        if bucket.iter().any(|(existing, k)| *k == kind && existing.dist_sq(pt) < 0.0625) {
            return;
        }
        if bucket.len() < 96 {
            bucket.push((pt, kind));
        }
    }

    pub fn insert_segment(&mut self, a: Point2, b: Point2) {
        let d = a.dist(b);
        if d < 0.2 {
            return;
        }
        self.segment_count += 1;
        self.insert_point(a, SnapKind::Endpoint);
        self.insert_point(b, SnapKind::Endpoint);
        if d > 3.0 {
            self.insert_point(a.midpoint(b), SnapKind::Midpoint);
        }
    }

    pub fn query_nearest(&self, target: Point2, radius_pt: f64) -> Option<SnapResult> {
        let c = self.cell_of(target);
        let reach = ((radius_pt / self.cell_size).ceil() as i32).clamp(1, 3);
        let max_sq = radius_pt * radius_pt;
        let mut best: Option<(f64, SnapResult)> = None;

        for dx in -reach..=reach {
            for dy in -reach..=reach {
                if let Some(bucket) = self.points.get(&(c.0 + dx, c.1 + dy)) {
                    for &(pt, kind) in bucket {
                        let d2 = pt.dist_sq(target);
                        if d2 <= max_sq {
                            // Prefer Endpoint slightly over Midpoint when equidistant
                            let score = match kind {
                                SnapKind::Endpoint | SnapKind::Center => d2 * 0.9,
                                _ => d2,
                            };
                            if best.as_ref().is_none_or(|(b_score, _)| score < *b_score) {
                                best = Some((score, SnapResult { point: pt, kind }));
                            }
                        }
                    }
                }
            }
        }
        best.map(|(_, r)| r)
    }
}

pub fn find_best_snap(
    cursor_pt: Point2,
    tolerance_pt: f64,
    settings: &SnapSettings,
    pdf_index: Option<&SnapIndex>,
    markups: &[Markup],
    page_index: usize,
    grid_step_pt: f64,
) -> Option<SnapResult> {
    let mut best: Option<(f64, SnapResult)> = None;
    let tol_sq = tolerance_pt * tolerance_pt;

    if settings.snap_to_markup {
        for m in markups {
            if m.page_index != page_index {
                continue;
            }
            for pt in markup_snap_points(m) {
                let d2 = pt.dist_sq(cursor_pt);
                if d2 <= tol_sq && best.as_ref().is_none_or(|(b, _)| d2 < *b) {
                    best = Some((
                        d2,
                        SnapResult {
                            point: pt,
                            kind: SnapKind::MarkupVertex,
                        },
                    ));
                }
            }
        }
    }

    if settings.snap_to_content
        && let Some(idx) = pdf_index
        && let Some(hit) = idx.query_nearest(cursor_pt, tolerance_pt)
    {
        let d2 = hit.point.dist_sq(cursor_pt);
        if best.as_ref().is_none_or(|(b, _)| d2 < *b) {
            best = Some((d2, hit));
        }
    }

    if best.is_none() && settings.snap_to_grid && grid_step_pt > 1.0 {
        let gx = (cursor_pt.x / grid_step_pt).round() * grid_step_pt;
        let gy = (cursor_pt.y / grid_step_pt).round() * grid_step_pt;
        let g = Point2::new(gx, gy);
        if g.dist_sq(cursor_pt) <= tol_sq * 2.25 {
            best = Some((
                g.dist_sq(cursor_pt),
                SnapResult {
                    point: g,
                    kind: SnapKind::Grid,
                },
            ));
        }
    }

    best.map(|(_, r)| r)
}

fn markup_snap_points(m: &Markup) -> Vec<Point2> {
    match &m.kind {
        MarkupKind::Note { pos } | MarkupKind::Typewriter { pos, .. } => vec![*pos],
        MarkupKind::TextBox { rect, .. }
        | MarkupKind::Rectangle { rect }
        | MarkupKind::Stamp { rect, .. }
        | MarkupKind::Image { rect, .. }
        | MarkupKind::SketchRect { rect, .. } => rect.corners().to_vec(),
        MarkupKind::Ellipse { rect } => vec![rect.center()],
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            ..
        } => vec![*arrow_tip, *knee, box_rect.center()],
        MarkupKind::Cloud { points, .. }
        | MarkupKind::Polyline { points }
        | MarkupKind::Polygon { points }
        | MarkupKind::MeasurePolylength { points, .. }
        | MarkupKind::MeasurePerimeter { points, .. }
        | MarkupKind::MeasureArea { points, .. }
        | MarkupKind::MeasureVolume { points, .. }
        | MarkupKind::MeasureCount { points, .. }
        | MarkupKind::SketchPoly { points, .. } => points.clone(),
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            ..
        } => {
            let mut pts = cloud_points.clone();
            pts.push(*knee);
            pts
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureLength { start, end, .. } => {
            vec![*start, *end, start.midpoint(*end)]
        }
        MarkupKind::Arc { start, mid, end } => vec![*start, *mid, *end],
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => vec![*p1, *vertex, *p2],
        MarkupKind::MeasureRadius { center, edge, .. } => vec![*center, *edge],
        MarkupKind::MeasureDiameter { p1, p2, .. } => vec![*p1, *p2, p1.midpoint(*p2)],
        _ => Vec::new(),
    }
}

/// Constrains `target` relative to `origin` to 45° / 90° steps when Shift or Ortho is active.
pub fn apply_ortho_constraint(origin: Point2, target: Point2, active: bool) -> Point2 {
    if !active {
        return target;
    }
    let delta = target - origin;
    let len = delta.len();
    if len < 1e-6 {
        return target;
    }
    let angle = delta.y.atan2(delta.x);
    let step = std::f64::consts::FRAC_PI_4; // 45° steps
    let snapped = (angle / step).round() * step;
    Point2::new(origin.x + len * snapped.cos(), origin.y + len * snapped.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_index_finds_endpoint_and_midpoint() {
        let mut idx = SnapIndex::new(16.0);
        idx.insert_segment(Point2::new(100.0, 100.0), Point2::new(200.0, 100.0));
        let end_hit = idx.query_nearest(Point2::new(103.0, 102.0), 8.0).unwrap();
        assert_eq!(end_hit.kind, SnapKind::Endpoint);
        assert_eq!(end_hit.point, Point2::new(100.0, 100.0));

        let mid_hit = idx.query_nearest(Point2::new(149.0, 102.0), 8.0).unwrap();
        assert_eq!(mid_hit.kind, SnapKind::Midpoint);
        assert_eq!(mid_hit.point, Point2::new(150.0, 100.0));
    }

    #[test]
    fn ortho_constraint_snaps_to_horizontal() {
        let p = apply_ortho_constraint(Point2::new(0.0, 0.0), Point2::new(100.0, 8.0), true);
        assert!((p.y).abs() < 1e-6);
        assert!(p.x > 99.0);
    }
}
