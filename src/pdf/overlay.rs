use crate::geometry::math::Point2;
use crate::model::document::DocumentTab;
use crate::pdf::io::open_pdf_from_bytes;
use crate::pdf::renderer::{RgbaImageData, default_interpreter_settings, render_page_rgba};
use hayro::RenderSettings;
use hayro::hayro_syntax::Pdf;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

/// 2D affine transform mapping a point `(x, y)` in Document A coordinates to `(u, v)` in Document B:
/// `u = a*x + b*y + tx`, `v = c*x + d*y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2D {
    pub a: f64,
    pub b: f64,
    pub tx: f64,
    pub c: f64,
    pub d: f64,
    pub ty: f64,
}

impl Default for Affine2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Affine2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: 1.0,
            ty: 0.0,
        }
    }

    pub fn translation(dx: f64, dy: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: dx,
            c: 0.0,
            d: 1.0,
            ty: dy,
        }
    }

    pub fn apply(&self, p: Point2) -> Point2 {
        Point2::new(
            self.a * p.x + self.b * p.y + self.tx,
            self.c * p.x + self.d * p.y + self.ty,
        )
    }

    /// Builds an affine transform mapping A -> B from manual offset (`dx, dy`),
    /// rotation around `center` (degrees), and uniform scale factor.
    pub fn from_trs(dx: f64, dy: f64, angle_deg: f64, scale: f64, center: Point2) -> Self {
        let s = if scale.abs() < 1e-4 { 1.0 } else { 1.0 / scale };
        let rad = (-angle_deg).to_radians();
        let cos = rad.cos() * s;
        let sin = rad.sin() * s;
        let a = cos;
        let b = -sin;
        let c = sin;
        let d = cos;
        let tx = center.x - (a * (center.x + dx) + b * (center.y + dy));
        let ty = center.y - (c * (center.x + dx) + d * (center.y + dy));
        Self {
            a,
            b,
            tx,
            c,
            d,
            ty,
        }
    }

    /// Exact 6-DOF 2D affine solver from 3 corresponding point pairs (Bluebeam 3-point alignment).
    /// Solves `M * pts_a[i] = pts_b[i]` for `i = 0..3`. Returns `None` if `pts_a` are collinear.
    pub fn from_three_point_pairs(pts_a: [Point2; 3], pts_b: [Point2; 3]) -> Option<Self> {
        let [p0, p1, p2] = pts_a;
        let [q0, q1, q2] = pts_b;
        let det = p0.x * (p1.y - p2.y) - p0.y * (p1.x - p2.x) + (p1.x * p2.y - p2.x * p1.y);
        if det.abs() < 1e-6 {
            return None;
        }
        let solve = |v0: f64, v1: f64, v2: f64| -> (f64, f64, f64) {
            let da = v0 * (p1.y - p2.y) - p0.y * (v1 - v2) + (v1 * p2.y - v2 * p1.y);
            let db = p0.x * (v1 - v2) - v0 * (p1.x - p2.x) + (p1.x * v2 - p2.x * v1);
            let dt = p0.x * (p1.y * v2 - p2.y * v1)
                - p0.y * (p1.x * v2 - p2.x * v1)
                + v0 * (p1.x * p2.y - p2.x * p1.y);
            (da / det, db / det, dt / det)
        };
        let (a, b, tx) = solve(q0.x, q1.x, q2.x);
        let (c, d, ty) = solve(q0.y, q1.y, q2.y);
        Some(Self {
            a,
            b,
            tx,
            c,
            d,
            ty,
        })
    }
}

#[derive(Debug, Clone)]
pub enum OverlayAlignmentMode {
    /// Direct 1:1 overlay without alignment adjustment.
    None,
    /// Coarse-to-fine normalized cross-correlation ink shift alignment.
    AutoTranslate,
    /// Exact 3-point affine alignment (handles translation, rotation, and scale differences).
    ThreePoint {
        pts_a: [Point2; 3],
        pts_b: [Point2; 3],
    },
    /// Manual shift (pt), rotation (degrees), and scale factor.
    Manual {
        dx_pt: f64,
        dy_pt: f64,
        angle_deg: f64,
        scale: f64,
    },
}

#[derive(Debug, Clone)]
pub struct OverlayConfig {
    pub color_a: [u8; 3],
    pub color_b: [u8; 3],
    pub alignment: OverlayAlignmentMode,
    pub max_side_px: u32,
    /// Pairs of `(page_idx_a, page_idx_b)` to overlay; if empty, overlays all common pages `0..min(na, nb)`.
    pub page_pairs: Vec<(usize, usize)>,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            color_a: [220, 38, 38],  // Red (Rev A)
            color_b: [22, 163, 74],  // Green (Rev B)
            alignment: OverlayAlignmentMode::AutoTranslate,
            max_side_px: 1800,
            page_pairs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OverlayReport {
    pub pages_overlaid: usize,
    pub applied_shift_pt: (f64, f64),
    pub overlap_ratio: f64,
}

/// Estimates the optimal translation `(dx_px, dy_px)` from A's pixel coordinates into B's pixel
/// coordinates via coarse-to-fine normalized ink correlation.
pub fn estimate_auto_shift_px(img_a: &RgbaImageData, img_b: &RgbaImageData) -> (f64, f64) {
    let w = img_a.width;
    let h = img_a.height;
    if w < 8 || h < 8 || img_b.width < 8 || img_b.height < 8 {
        return (0.0, 0.0);
    }

    // Coarse grid (~128px max side)
    let target = 128usize;
    let step = (w.max(h) / target).max(1);
    let cw = (w / step).max(1);
    let ch = (h / step).max(1);

    let build_coarse = |img: &RgbaImageData| -> Vec<f32> {
        let mut grid = vec![0.0f32; cw * ch];
        for cy in 0..ch {
            let y0 = cy * step;
            let y1 = ((cy + 1) * step).min(img.height);
            for cx in 0..cw {
                let x0 = cx * step;
                let x1 = ((cx + 1) * step).min(img.width);
                let mut sum = 0.0f32;
                let mut cnt = 0usize;
                for y in y0..y1 {
                    let row = y * img.width;
                    for x in x0..x1 {
                        let off = (row + x) * 4;
                        let r = img.premul_rgba[off] as f32;
                        let g = img.premul_rgba[off + 1] as f32;
                        let b = img.premul_rgba[off + 2] as f32;
                        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                        sum += ((255.0 - lum) / 255.0).max(0.0);
                        cnt += 1;
                    }
                }
                if cnt > 0 {
                    grid[cy * cw + cx] = sum / cnt as f32;
                }
            }
        }
        grid
    };

    let ga = build_coarse(img_a);
    let gb = build_coarse(img_b);
    let max_dx = (cw / 4).clamp(4, 36) as i32;
    let max_dy = (ch / 4).clamp(4, 36) as i32;

    let mut best_score = -1.0f64;
    let mut best_cdx = 0i32;
    let mut best_cdy = 0i32;

    for dy in -max_dy..=max_dy {
        for dx in -max_dx..=max_dx {
            let mut dot = 0.0f64;
            let mut na = 0.0f64;
            let mut nb = 0.0f64;
            let y_start = 0.max(-dy);
            let y_end = (ch as i32).min(ch as i32 - dy);
            let x_start = 0.max(-dx);
            let x_end = (cw as i32).min(cw as i32 - dx);
            for y in y_start..y_end {
                let ra = y as usize * cw;
                let rb = (y + dy) as usize * cw;
                for x in x_start..x_end {
                    let va = ga[ra + x as usize] as f64;
                    let vb = gb[rb + (x + dx) as usize] as f64;
                    dot += va * vb;
                    na += va * va;
                    nb += vb * vb;
                }
            }
            let denom = (na * nb).sqrt();
            if denom > 1e-6 {
                let score = dot / denom;
                if score > best_score {
                    best_score = score;
                    best_cdx = dx;
                    best_cdy = dy;
                }
            }
        }
    }

    // Fine pixel refinement around (best_cdx * step, best_cdy * step)
    let base_dx = best_cdx * step as i32;
    let base_dy = best_cdy * step as i32;
    let radius = (step as i32).clamp(2, 10);
    let fine_stride = (w.max(h) / 360).max(1);

    let mut fine_best_score = -1.0f64;
    let mut fine_dx = base_dx;
    let mut fine_dy = base_dy;

    let ink_at = |img: &RgbaImageData, x: usize, y: usize| -> f64 {
        let off = (y * img.width + x) * 4;
        let r = img.premul_rgba[off] as f64;
        let g = img.premul_rgba[off + 1] as f64;
        let b = img.premul_rgba[off + 2] as f64;
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        ((255.0 - lum) / 255.0).max(0.0)
    };

    for dy in (base_dy - radius)..=(base_dy + radius) {
        for dx in (base_dx - radius)..=(base_dx + radius) {
            let mut dot = 0.0f64;
            let mut na = 0.0f64;
            let mut nb = 0.0f64;
            let mut y = 0usize;
            while y < h {
                let by = y as i32 + dy;
                if by >= 0 && (by as usize) < img_b.height {
                    let mut x = 0usize;
                    while x < w {
                        let bx = x as i32 + dx;
                        if bx >= 0 && (bx as usize) < img_b.width {
                            let va = ink_at(img_a, x, y);
                            let vb = ink_at(img_b, bx as usize, by as usize);
                            dot += va * vb;
                            na += va * va;
                            nb += vb * vb;
                        }
                        x += fine_stride;
                    }
                }
                y += fine_stride;
            }
            let denom = (na * nb).sqrt();
            if denom > 1e-6 {
                let score = dot / denom;
                if score > fine_best_score {
                    fine_best_score = score;
                    fine_dx = dx;
                    fine_dy = dy;
                }
            }
        }
    }

    (fine_dx as f64, fine_dy as f64)
}

fn sample_bilinear_ink(img: &RgbaImageData, u: f64, v: f64) -> f64 {
    if u < 0.0 || v < 0.0 || u >= (img.width as f64 - 1.0) || v >= (img.height as f64 - 1.0) {
        return 0.0;
    }
    let x0 = u.floor() as usize;
    let y0 = v.floor() as usize;
    let x1 = (x0 + 1).min(img.width - 1);
    let y1 = (y0 + 1).min(img.height - 1);
    let fx = u - x0 as f64;
    let fy = v - y0 as f64;

    let ink = |x: usize, y: usize| -> f64 {
        let off = (y * img.width + x) * 4;
        let r = img.premul_rgba[off] as f64;
        let g = img.premul_rgba[off + 1] as f64;
        let b = img.premul_rgba[off + 2] as f64;
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        ((255.0 - lum) / 255.0).clamp(0.0, 1.0)
    };

    let i00 = ink(x0, y0);
    let i10 = ink(x1, y0);
    let i01 = ink(x0, y1);
    let i11 = ink(x1, y1);
    let top = i00 * (1.0 - fx) + i10 * fx;
    let bot = i01 * (1.0 - fx) + i11 * fx;
    top * (1.0 - fy) + bot * fy
}

/// Creates a multi-page Bluebeam-style Overlay PDF DocumentTab combining Document A and Document B
/// using per-revision tint colors, photographic Multiply blending, and Auto / 3-Point / Manual alignment.
pub fn overlay_pdf_pages(
    bytes_a: &[u8],
    bytes_b: &[u8],
    config: &OverlayConfig,
) -> Result<(DocumentTab, OverlayReport), String> {
    let pdf_a = Pdf::new(bytes_a.to_vec()).map_err(|e| format!("PDF A inválido: {e:?}"))?;
    let pdf_b = Pdf::new(bytes_b.to_vec()).map_err(|e| format!("PDF B inválido: {e:?}"))?;
    let count_a = pdf_a.pages().len();
    let count_b = pdf_b.pages().len();
    if count_a == 0 || count_b == 0 {
        return Err("Uno de los PDFs no contiene páginas.".to_string());
    }

    let pairs: Vec<(usize, usize)> = if config.page_pairs.is_empty() {
        let n = count_a.min(count_b);
        (0..n).map(|i| (i, i)).collect()
    } else {
        config
            .page_pairs
            .iter()
            .copied()
            .filter(|&(pa, pb)| pa < count_a && pb < count_b)
            .collect()
    };
    if pairs.is_empty() {
        return Err("No hay pares de páginas válidos para superponer.".to_string());
    }

    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();

    let mut out_doc = Document::with_version("1.7");
    let pages_id = out_doc.new_object_id();
    let mut page_ids = Vec::with_capacity(pairs.len());
    let mut last_shift_pt = (0.0, 0.0);
    let mut total_ink_union = 0usize;
    let mut total_ink_inter = 0usize;

    for &(pa, pb) in &pairs {
        let (w_a_pt, h_a_pt) = pdf_a.pages()[pa].render_dimensions();
        let (w_b_pt, h_b_pt) = pdf_b.pages()[pb].render_dimensions();
        let w_a_pt = w_a_pt as f64;
        let h_a_pt = h_a_pt as f64;
        let w_b_pt = w_b_pt as f64;
        let h_b_pt = h_b_pt as f64;

        let img_a = render_page_rgba(bytes_a, pa, config.max_side_px, &interp, &rs)?;
        let img_b = render_page_rgba(bytes_b, pb, config.max_side_px, &interp, &rs)?;
        let w = img_a.width;
        let h = img_a.height;
        if w == 0 || h == 0 || img_b.width == 0 || img_b.height == 0 {
            continue;
        }

        // Build pixel-space affine map from A pixel (px, py) -> B pixel (u, v)
        let sx_a = w_a_pt / w as f64;
        let sy_a = h_a_pt / h as f64;
        let inv_sx_b = img_b.width as f64 / w_b_pt.max(1.0);
        let inv_sy_b = img_b.height as f64 / h_b_pt.max(1.0);

        let map_pt_a_to_b: Affine2D = match &config.alignment {
            OverlayAlignmentMode::None => Affine2D::identity(),
            OverlayAlignmentMode::AutoTranslate => {
                let (dx_px, dy_px) = estimate_auto_shift_px(&img_a, &img_b);
                let dx_pt = dx_px * sx_a;
                let dy_pt = dy_px * sy_a;
                last_shift_pt = (dx_pt, dy_pt);
                Affine2D::translation(dx_pt, dy_pt)
            }
            OverlayAlignmentMode::ThreePoint { pts_a, pts_b } => {
                let xf = Affine2D::from_three_point_pairs(*pts_a, *pts_b).ok_or_else(|| {
                    "Los 3 puntos de alineación en el plano A son colineales; elija 3 esquinas no alineadas.".to_string()
                })?;
                last_shift_pt = (xf.tx, xf.ty);
                xf
            }
            OverlayAlignmentMode::Manual {
                dx_pt,
                dy_pt,
                angle_deg,
                scale,
            } => {
                last_shift_pt = (*dx_pt, *dy_pt);
                Affine2D::from_trs(
                    *dx_pt,
                    *dy_pt,
                    *angle_deg,
                    *scale,
                    Point2::new(w_a_pt * 0.5, h_a_pt * 0.5),
                )
            }
        };

        let mut rgb = vec![255u8; w * h * 3];
        let [ra_t, ga_t, ba_t] = config.color_a;
        let [rb_t, gb_t, bb_t] = config.color_b;

        for y in 0..h {
            let py_pt = (y as f64 + 0.5) * sy_a;
            let row_off = y * w;
            for x in 0..w {
                let px_pt = (x as f64 + 0.5) * sx_a;
                let off_a = (row_off + x) * 4;
                let lum_a = 0.299 * img_a.premul_rgba[off_a] as f64
                    + 0.587 * img_a.premul_rgba[off_a + 1] as f64
                    + 0.114 * img_a.premul_rgba[off_a + 2] as f64;
                let ink_a = ((255.0 - lum_a) / 255.0).clamp(0.0, 1.0);

                let pt_b = map_pt_a_to_b.apply(Point2::new(px_pt, py_pt));
                let u = pt_b.x * inv_sx_b - 0.5;
                let v = pt_b.y * inv_sy_b - 0.5;
                let ink_b = sample_bilinear_ink(&img_b, u, v);

                let has_a = ink_a > 0.08;
                let has_b = ink_b > 0.08;
                if has_a || has_b {
                    total_ink_union += 1;
                    if has_a && has_b {
                        total_ink_inter += 1;
                    }
                }

                // Photographic Multiply compositing of tinted A and tinted B layers
                let ca_r = 255.0 - ink_a * (255.0 - ra_t as f64);
                let ca_g = 255.0 - ink_a * (255.0 - ga_t as f64);
                let ca_b = 255.0 - ink_a * (255.0 - ba_t as f64);

                let cb_r = 255.0 - ink_b * (255.0 - rb_t as f64);
                let cb_g = 255.0 - ink_b * (255.0 - gb_t as f64);
                let cb_b = 255.0 - ink_b * (255.0 - bb_t as f64);

                let dst = (row_off + x) * 3;
                rgb[dst] = ((ca_r * cb_r) / 255.0).round().clamp(0.0, 255.0) as u8;
                rgb[dst + 1] = ((ca_g * cb_g) / 255.0).round().clamp(0.0, 255.0) as u8;
                rgb[dst + 2] = ((ca_b * cb_b) / 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }

        let mut img_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => (w as i64),
                "Height" => (h as i64),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8i64,
            },
            rgb,
        );
        let _ = img_stream.compress();
        let img_id = out_doc.add_object(img_stream);

        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    w_a_pt.into(),
                    0.0.into(),
                    0.0.into(),
                    h_a_pt.into(),
                    0.0.into(),
                    0.0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"OV0".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        let content_bytes = Content { operations: ops }.encode().unwrap_or_default();
        let mut cstream = Stream::new(dictionary! {}, content_bytes);
        let _ = cstream.compress();
        let content_id = out_doc.add_object(cstream);

        let page_id = out_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), w_a_pt.into(), h_a_pt.into()],
            "Resources" => dictionary! {
                "XObject" => dictionary! { "OV0" => img_id },
            },
            "Contents" => content_id,
        });
        page_ids.push(page_id);
    }

    if page_ids.is_empty() {
        return Err("No se pudo generar ninguna página superpuesta.".to_string());
    }

    let kids: Vec<Object> = page_ids.iter().map(|&id| Object::Reference(id)).collect();
    out_doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => (page_ids.len() as i64),
        }),
    );
    let catalog_id = out_doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    out_doc.trailer.set("Root", catalog_id);

    let mut out_bytes = Vec::new();
    out_doc
        .save_to(&mut out_bytes)
        .map_err(|e| format!("Error guardando PDF Overlay: {e}"))?;

    let mut tab = open_pdf_from_bytes("Overlay_Planos_RedBeam.pdf", None, out_bytes)?;
    for (i, &(pa, pb)) in pairs.iter().enumerate() {
        if let Some(pm) = tab.pages.get_mut(i) {
            pm.label = Some(format!("Overlay Pág. {} vs {}", pa + 1, pb + 1));
        }
    }
    tab.dirty = true;

    let overlap_ratio = if total_ink_union > 0 {
        total_ink_inter as f64 / total_ink_union as f64
    } else {
        1.0
    };
    Ok((
        tab,
        OverlayReport {
            pages_overlaid: page_ids.len(),
            applied_shift_pt: last_shift_pt,
            overlap_ratio,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_point_affine_recovers_rotation_scale_and_translation() {
        // Transform: scale 1.25, rotate 90 deg CCW, shift (+30, -15)
        let gt = |p: Point2| Point2::new(-1.25 * p.y + 30.0, 1.25 * p.x - 15.0);
        let a = [
            Point2::new(10.0, 20.0),
            Point2::new(110.0, 20.0),
            Point2::new(10.0, 90.0),
        ];
        let b = [gt(a[0]), gt(a[1]), gt(a[2])];
        let xf = Affine2D::from_three_point_pairs(a, b).expect("non-collinear");
        let test_pt = Point2::new(47.5, 63.2);
        let expected = gt(test_pt);
        let got = xf.apply(test_pt);
        assert!((got.x - expected.x).abs() < 1e-6);
        assert!((got.y - expected.y).abs() < 1e-6);
    }

    #[test]
    fn three_point_affine_rejects_collinear_points() {
        let a = [
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(20.0, 20.0),
        ];
        assert!(Affine2D::from_three_point_pairs(a, a).is_none());
    }
}
