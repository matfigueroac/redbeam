use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

/// 2D point in normalized top-left page points (0..width_pt, 0..height_pt).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn dist(self, other: Self) -> f64 {
        self.dist_sq(other).sqrt()
    }

    #[inline]
    pub fn dist_sq(self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }

    #[inline]
    pub fn len(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[inline]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    #[inline]
    pub fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    #[inline]
    pub fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let l = self.len();
        if l <= 1e-12 {
            Self::new(1.0, 0.0)
        } else {
            Self::new(self.x / l, self.y / l)
        }
    }

    #[inline]
    pub fn midpoint(self, other: Self) -> Self {
        Self::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    #[inline]
    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl Add for Point2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// Axis-aligned rectangle in normalized top-left page points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect2 {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect2 {
    pub fn from_min_max(a: Point2, b: Point2) -> Self {
        Self {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            max_x: a.x.max(b.x),
            max_y: a.y.max(b.y),
        }
    }

    pub fn from_xywh(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self::from_min_max(Point2::new(x, y), Point2::new(x + w, y + h))
    }

    pub fn from_points(points: &[Point2]) -> Option<Self> {
        let first = *points.first()?;
        let mut r = Self::from_min_max(first, first);
        for &p in &points[1..] {
            r.min_x = r.min_x.min(p.x);
            r.min_y = r.min_y.min(p.y);
            r.max_x = r.max_x.max(p.x);
            r.max_y = r.max_y.max(p.y);
        }
        Some(r)
    }

    #[inline]
    pub fn width(self) -> f64 {
        (self.max_x - self.min_x).max(0.0)
    }

    #[inline]
    pub fn height(self) -> f64 {
        (self.max_y - self.min_y).max(0.0)
    }

    #[inline]
    pub fn center(self) -> Point2 {
        Point2::new(
            (self.min_x + self.max_x) * 0.5,
            (self.min_y + self.max_y) * 0.5,
        )
    }

    #[inline]
    pub fn min(self) -> Point2 {
        Point2::new(self.min_x, self.min_y)
    }

    #[inline]
    pub fn max(self) -> Point2 {
        Point2::new(self.max_x, self.max_y)
    }

    #[inline]
    pub fn contains(self, p: Point2) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }

    #[inline]
    pub fn intersects(self, other: Self) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    #[inline]
    pub fn expand(self, margin: f64) -> Self {
        Self {
            min_x: self.min_x - margin,
            min_y: self.min_y - margin,
            max_x: self.max_x + margin,
            max_y: self.max_y + margin,
        }
    }

    #[inline]
    pub fn translate(self, delta: Point2) -> Self {
        Self {
            min_x: self.min_x + delta.x,
            min_y: self.min_y + delta.y,
            max_x: self.max_x + delta.x,
            max_y: self.max_y + delta.y,
        }
    }

    pub fn corners(self) -> [Point2; 4] {
        [
            Point2::new(self.min_x, self.min_y),
            Point2::new(self.max_x, self.min_y),
            Point2::new(self.max_x, self.max_y),
            Point2::new(self.min_x, self.max_y),
        ]
    }
}

/// Single source of truth for coordinate transforms:
/// Normalized page points (origin top-left, Y down) <-> Screen pixels & PDF bottom-left space.
#[derive(Debug, Clone, Copy)]
pub struct PageTransform {
    pub page_width_pt: f64,
    pub page_height_pt: f64,
    pub viewport_origin_x: f64,
    pub viewport_origin_y: f64,
    /// Scale factor in screen logical points per 1 PDF point.
    pub scale: f64,
    /// Pan offset in normalized page points at top-left of viewport.
    pub pan_x: f64,
    pub pan_y: f64,
}

impl PageTransform {
    #[inline]
    pub fn page_to_screen(self, pt: Point2) -> (f32, f32) {
        let sx = self.viewport_origin_x + (pt.x - self.pan_x) * self.scale;
        let sy = self.viewport_origin_y + (pt.y - self.pan_y) * self.scale;
        (sx as f32, sy as f32)
    }

    #[inline]
    pub fn screen_to_page(self, sx: f64, sy: f64) -> Point2 {
        Point2::new(
            self.pan_x + (sx - self.viewport_origin_x) / self.scale.max(1e-9),
            self.pan_y + (sy - self.viewport_origin_y) / self.scale.max(1e-9),
        )
    }

    #[inline]
    pub fn page_to_pdf_bottom_left(self, pt: Point2) -> (f64, f64) {
        (pt.x, self.page_height_pt - pt.y)
    }

    #[inline]
    pub fn pdf_bottom_left_to_page(self, x: f64, y: f64) -> Point2 {
        Point2::new(x, self.page_height_pt - y)
    }

    pub fn rect_to_pdf_bbox(self, rect: Rect2) -> [f64; 4] {
        [
            rect.min_x,
            self.page_height_pt - rect.max_y,
            rect.max_x,
            self.page_height_pt - rect.min_y,
        ]
    }
}

/// Signed shoelace area in normalized top-left coordinates (positive = clockwise on screen).
pub fn polygon_signed_area(points: &[Point2]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum * 0.5
}

pub fn polygon_area(points: &[Point2]) -> f64 {
    polygon_signed_area(points).abs()
}

pub fn polygon_centroid(points: &[Point2]) -> Point2 {
    if points.is_empty() {
        return Point2::default();
    }
    let signed = polygon_signed_area(points);
    if signed.abs() < 1e-9 {
        let mut sx = 0.0;
        let mut sy = 0.0;
        for p in points {
            sx += p.x;
            sy += p.y;
        }
        let n = points.len() as f64;
        return Point2::new(sx / n, sy / n);
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let f = a.x * b.y - b.x * a.y;
        cx += (a.x + b.x) * f;
        cy += (a.y + b.y) * f;
    }
    let k = 1.0 / (6.0 * signed);
    Point2::new(cx * k, cy * k)
}

pub fn point_in_polygon(pt: Point2, poly: &[Point2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let pi = poly[i];
        let pj = poly[j];
        let intersect = ((pi.y > pt.y) != (pj.y > pt.y))
            && (pt.x < (pj.x - pi.x) * (pt.y - pi.y) / (pj.y - pi.y + 1e-18) + pi.x);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn point_in_polygon_with_cutouts(pt: Point2, outer: &[Point2], cutouts: &[Vec<Point2>]) -> bool {
    if !point_in_polygon(pt, outer) {
        return false;
    }
    for hole in cutouts {
        if point_in_polygon(pt, hole) {
            return false;
        }
    }
    true
}

pub fn project_point_to_segment(p: Point2, a: Point2, b: Point2) -> Point2 {
    let ab = b - a;
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq <= 1e-12 {
        return a;
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    a + ab * t
}

pub fn dist_point_to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    p.dist(project_point_to_segment(p, a, b))
}

/// Angle in degrees (0..=180) at `vertex` between rays `vertex->p1` and `vertex->p2`.
pub fn angle_three_points_deg(p1: Point2, vertex: Point2, p2: Point2) -> f64 {
    let v1 = (p1 - vertex).normalize();
    let v2 = (p2 - vertex).normalize();
    let d = v1.dot(v2).clamp(-1.0, 1.0);
    d.acos().to_degrees()
}

/// Computes circle center & radius from 3 non-collinear points.
pub fn circle_from_three_points(p1: Point2, p2: Point2, p3: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (p1.x * (p2.y - p3.y) + p2.x * (p3.y - p1.y) + p3.x * (p1.y - p2.y));
    if d.abs() < 1e-9 {
        return None;
    }
    let p1_sq = p1.x * p1.x + p1.y * p1.y;
    let p2_sq = p2.x * p2.x + p2.y * p2.y;
    let p3_sq = p3.x * p3.x + p3.y * p3.y;
    let ux = (p1_sq * (p2.y - p3.y) + p2_sq * (p3.y - p1.y) + p3_sq * (p1.y - p2.y)) / d;
    let uy = (p1_sq * (p3.x - p2.x) + p2_sq * (p1.x - p3.x) + p3_sq * (p2.x - p1.x)) / d;
    let center = Point2::new(ux, uy);
    Some((center, center.dist(p1)))
}

/// Samples a 3-point circular arc from `start` through `mid` to `end`.
pub fn sample_three_point_arc(
    start: Point2,
    mid: Point2,
    end: Point2,
    segments: usize,
) -> Vec<Point2> {
    let Some((center, radius)) = circle_from_three_points(start, mid, end) else {
        return vec![start, mid, end];
    };
    let a0 = (start.y - center.y).atan2(start.x - center.x);
    let a1 = (mid.y - center.y).atan2(mid.x - center.x);
    let a2 = (end.y - center.y).atan2(end.x - center.x);

    let norm = |a: f64| {
        let mut r = a % std::f64::consts::TAU;
        if r < 0.0 {
            r += std::f64::consts::TAU;
        }
        r
    };
    let d_mid = norm(a1 - a0);
    let d_end = norm(a2 - a0);
    let ccw = d_mid <= d_end;
    let sweep = if ccw {
        d_end
    } else {
        d_end - std::f64::consts::TAU
    };

    let n = segments.max(8);
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let ang = a0 + sweep * t;
            Point2::new(center.x + radius * ang.cos(), center.y + radius * ang.sin())
        })
        .collect()
}

pub fn sample_ellipse(rect: Rect2, segments: usize) -> Vec<Point2> {
    let c = rect.center();
    let rx = rect.width() * 0.5;
    let ry = rect.height() * 0.5;
    let n = segments.max(16);
    (0..n)
        .map(|i| {
            let ang = (i as f64 / n as f64) * std::f64::consts::TAU;
            Point2::new(c.x + rx * ang.cos(), c.y + ry * ang.sin())
        })
        .collect()
}

/// One semicircular cloud scallop arc along an edge, described by polyline points.
pub fn generate_cloud_polyline(points: &[Point2], arc_chord_pt: f64) -> Vec<Point2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let chord = arc_chord_pt.clamp(6.0, 64.0);
    // Ensure consistent winding so outward normal always points exterior to the polygon.
    let signed = polygon_signed_area(points);
    // In Y-down coordinates: positive signed area means clockwise on screen; outward normal for edge (a->b) is (dy, -dx).
    let outward_sign = if signed >= 0.0 { 1.0 } else { -1.0 };

    let mut out = Vec::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let edge = b - a;
        let len = edge.len();
        if len < 1e-4 {
            continue;
        }
        let count = ((len / chord).round() as usize).max(1);
        let dir = edge * (1.0 / len);
        // Outward normal
        let normal = Point2::new(dir.y * outward_sign, -dir.x * outward_sign);
        let seg_len = len / count as f64;
        let bulge = seg_len * 0.42;

        for k in 0..count {
            let p0 = a + dir * (k as f64 * seg_len);
            let p1 = a + dir * ((k + 1) as f64 * seg_len);
            let mid = p0.midpoint(p1);
            // Sample semicircular arc from p0 to p1 bulging along normal
            const STEPS: usize = 6;
            for s in 0..STEPS {
                let theta = std::f64::consts::PI * (s as f64 / STEPS as f64);
                let along = (0.5 - 0.5 * theta.cos()) * seg_len;
                let out_d = theta.sin() * bulge;
                out.push(p0 + dir * along + normal * out_d);
            }
            if k + 1 == count && i + 1 == points.len() {
                out.push(p1);
            } else {
                let _ = mid;
            }
        }
    }
    out
}

/// Clips parallel hatch lines against an outer polygon minus optional cutout holes.
pub fn clip_hatch_lines(
    outer: &[Point2],
    cutouts: &[Vec<Point2>],
    angle_deg: f64,
    spacing: f64,
) -> Vec<(Point2, Point2)> {
    if outer.len() < 3 || spacing <= 1.0 {
        return Vec::new();
    }
    let rad = angle_deg.to_radians();
    let dir = Point2::new(rad.cos(), rad.sin());
    let norm = Point2::new(-rad.sin(), rad.cos());

    let mut min_n = f64::INFINITY;
    let mut max_n = f64::NEG_INFINITY;
    for &p in outer {
        let proj = p.dot(norm);
        min_n = min_n.min(proj);
        max_n = max_n.max(proj);
    }

    let mut loops: Vec<&[Point2]> = Vec::with_capacity(1 + cutouts.len());
    loops.push(outer);
    for c in cutouts {
        if c.len() >= 3 {
            loops.push(c);
        }
    }

    let mut result = Vec::new();
    let mut d = min_n + spacing * 0.5;
    while d < max_n {
        let mut hits = Vec::new();
        for poly in &loops {
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                let da = a.dot(norm) - d;
                let db = b.dot(norm) - d;
                if (da <= 0.0 && db > 0.0) || (db <= 0.0 && da > 0.0) {
                    let t = da / (da - db);
                    let pt = a.lerp(b, t);
                    hits.push(pt.dot(dir));
                }
            }
        }
        hits.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        for pair in hits.as_chunks::<2>().0 {
            let p0 = dir * pair[0] + norm * d;
            let p1 = dir * pair[1] + norm * d;
            if p0.dist(p1) > 0.5 {
                result.push((p0, p1));
            }
        }
        d += spacing;
    }
    result
}

/// Computes an outward-offset dimension line `(dim_a, dim_b)` for segment `a -> b`.
pub fn dimension_offset_segment(a: Point2, b: Point2, offset_pt: f64) -> (Point2, Point2, Point2) {
    let d = b - a;
    let len = d.len();
    let normal = if len > 1e-6 {
        Point2::new(-d.y / len, d.x / len)
    } else {
        Point2::new(0.0, -1.0)
    };
    let shift = normal * offset_pt;
    (a + shift, b + shift, normal)
}

fn seg_properly_intersects(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let cross = |p: Point2, q: Point2, r: Point2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    ((d1 > 1e-7 && d2 < -1e-7) || (d1 < -1e-7 && d2 > 1e-7))
        && ((d3 > 1e-7 && d4 < -1e-7) || (d3 < -1e-7 && d4 > 1e-7))
}

fn point_in_tri_strict(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let cross = |u: Point2, v: Point2, w: Point2| (v.x - u.x) * (w.y - u.y) - (v.y - u.y) * (w.x - u.x);
    let c1 = cross(a, b, p);
    let c2 = cross(b, c, p);
    let c3 = cross(c, a, p);
    c1 >= -1e-6 && c2 >= -1e-6 && c3 >= -1e-6
}

fn clean_ring(pts: &[Point2]) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|prev| prev.dist(p) > 1e-5) {
            out.push(p);
        }
    }
    if out.len() >= 2 && out.first().unwrap().dist(*out.last().unwrap()) <= 1e-5 {
        out.pop();
    }
    out
}

/// Triangulates an arbitrarily concave simple polygon with optional interior cutout holes
/// into clockwise screen-space triangles via hole-bridging + ear clipping.
pub fn triangulate_polygon_with_holes(outer: &[Point2], holes: &[Vec<Point2>]) -> Vec<[Point2; 3]> {
    let mut ring = clean_ring(outer);
    if ring.len() < 3 {
        return Vec::new();
    }
    if polygon_signed_area(&ring) < 0.0 {
        ring.reverse();
    }

    // Prepare holes oriented opposite to outer (negative signed area) and ordered right-to-left
    let mut clean_holes: Vec<(f64, Vec<Point2>)> = holes
        .iter()
        .filter_map(|h| {
            let mut ch = clean_ring(h);
            if ch.len() < 3 || polygon_area(&ch) < 1e-4 {
                return None;
            }
            if polygon_signed_area(&ch) > 0.0 {
                ch.reverse();
            }
            let max_x = ch.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            Some((max_x, ch))
        })
        .collect();
    clean_holes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Bridge each hole into the outer ring using a collision-free mutual vertex pair
    for (_, hole) in &clean_holes {
        let mut best: Option<(f64, usize, usize)> = None;
        for (oi, &op) in ring.iter().enumerate() {
            for (hi, &hp) in hole.iter().enumerate() {
                if op.dist(hp) < 1e-6 {
                    continue;
                }
                let blocked_outer = (0..ring.len()).any(|k| {
                    let k2 = (k + 1) % ring.len();
                    k != oi && k2 != oi && seg_properly_intersects(op, hp, ring[k], ring[k2])
                });
                if blocked_outer {
                    continue;
                }
                let blocked_hole = (0..hole.len()).any(|k| {
                    let k2 = (k + 1) % hole.len();
                    k != hi && k2 != hi && seg_properly_intersects(op, hp, hole[k], hole[k2])
                });
                if blocked_hole {
                    continue;
                }
                let d2 = (op.x - hp.x).powi(2) + (op.y - hp.y).powi(2);
                if best.is_none_or(|(bd, _, _)| d2 < bd) {
                    best = Some((d2, oi, hi));
                }
            }
        }
        if let Some((_, oi, hi)) = best {
            let mut merged = Vec::with_capacity(ring.len() + hole.len() + 2);
            merged.extend_from_slice(&ring[..=oi]);
            for step in 0..=hole.len() {
                merged.push(hole[(hi + step) % hole.len()]);
            }
            merged.push(ring[oi]);
            merged.extend_from_slice(&ring[oi + 1..]);
            ring = merged;
        }
    }

    // Ear clipping
    let mut tris = Vec::with_capacity(ring.len().saturating_sub(2));
    let mut verts: Vec<Point2> = ring;
    let mut guard = verts.len() * verts.len() + 16;

    while verts.len() > 3 && guard > 0 {
        guard -= 1;
        let n = verts.len();
        let mut clipped = false;
        for i in 0..n {
            let a = verts[(i + n - 1) % n];
            let b = verts[i];
            let c = verts[(i + 1) % n];
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if cross <= 1e-8 {
                continue;
            }
            let any_inside = (0..n).any(|k| {
                if k == i || k == (i + n - 1) % n || k == (i + 1) % n {
                    return false;
                }
                let p = verts[k];
                if p.dist(a) < 1e-6 || p.dist(b) < 1e-6 || p.dist(c) < 1e-6 {
                    return false;
                }
                point_in_tri_strict(p, a, b, c)
            });
            if !any_inside {
                tris.push([a, b, c]);
                verts.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break;
        }
    }
    if verts.len() == 3 {
        let a = verts[0];
        let b = verts[1];
        let c = verts[2];
        let area2 = ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs();
        if area2 > 1e-8 {
            tris.push([a, b, c]);
        }
    }
    tris
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_area_and_cloud_generation_work() {
        let sq = vec![
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 50.0),
            Point2::new(0.0, 50.0),
        ];
        assert!((polygon_area(&sq) - 5000.0).abs() < 1e-6);
        let cloud = generate_cloud_polyline(&sq, 18.0);
        assert!(cloud.len() > 40);
        // Cloud arcs must extend slightly outside original rectangle
        assert!(cloud.iter().any(|p| p.y < -1.0));
        assert!(cloud.iter().any(|p| p.x > 101.0));
    }

    #[test]
    fn hatch_respects_cutout_hole() {
        let outer = Rect2::from_xywh(0.0, 0.0, 100.0, 100.0).corners().to_vec();
        let hole = Rect2::from_xywh(30.0, 30.0, 40.0, 40.0).corners().to_vec();
        let lines = clip_hatch_lines(&outer, &[hole], 0.0, 10.0);
        // At y = 45, the scanline should split into two segments (0..30 and 70..100).
        let mid_lines: Vec<_> = lines
            .iter()
            .filter(|(a, _)| (a.y - 45.0).abs() < 1.0)
            .collect();
        assert_eq!(mid_lines.len(), 2);
    }

    #[test]
    fn triangulates_concave_l_shape_and_hole_with_exact_area() {
        // L-shape: 100x100 minus 50x50 corner = 7500
        let l_shape = vec![
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 50.0),
            Point2::new(50.0, 50.0),
            Point2::new(50.0, 100.0),
            Point2::new(0.0, 100.0),
        ];
        // Cutout hole: 20x20 = 400 inside the bottom-left leg
        let hole = vec![
            Point2::new(10.0, 10.0),
            Point2::new(30.0, 10.0),
            Point2::new(30.0, 30.0),
            Point2::new(10.0, 30.0),
        ];
        let tris = triangulate_polygon_with_holes(&l_shape, &[hole]);
        assert!(!tris.is_empty());
        let total_area: f64 = tris
            .iter()
            .map(|[a, b, c]| ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() * 0.5)
            .sum();
        assert!((total_area - 7100.0).abs() < 1e-4, "got {total_area}");
    }
}
