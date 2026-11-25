use crate::geometry::math::{Point2, Rect2};
use crate::pdf::renderer::{RgbaImageData, default_interpreter_settings, render_page_rgba};
use hayro::RenderSettings;

#[derive(Debug, Clone)]
pub struct VisualMatch {
    pub rect_pt: Rect2,
    pub center_pt: Point2,
    pub score: f32,
    pub rotation_deg: u16,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct VisualSearchResult {
    pub page_index: usize,
    pub template_rect_pt: Rect2,
    pub matches: Vec<VisualMatch>,
}

struct GrayImage {
    w: usize,
    h: usize,
    /// Ink intensity in `0.0..=1.0` (`0.0` = white background, `1.0` = solid black ink).
    data: Vec<f32>,
}

impl GrayImage {
    fn from_rgba(img: &RgbaImageData) -> Self {
        let n = img.width * img.height;
        let mut data = Vec::with_capacity(n);
        for px in img.premul_rgba.as_chunks::<4>().0 {
            let lum = 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
            data.push(((255.0 - lum) / 255.0).clamp(0.0, 1.0));
        }
        Self {
            w: img.width,
            h: img.height,
            data,
        }
    }

    fn downsample(&self, factor: usize) -> Self {
        if factor <= 1 {
            return Self {
                w: self.w,
                h: self.h,
                data: self.data.clone(),
            };
        }
        let nw = (self.w / factor).max(1);
        let nh = (self.h / factor).max(1);
        let mut data = vec![0.0f32; nw * nh];
        for ny in 0..nh {
            let y0 = ny * factor;
            let y1 = ((ny + 1) * factor).min(self.h);
            for nx in 0..nw {
                let x0 = nx * factor;
                let x1 = ((nx + 1) * factor).min(self.w);
                let mut sum = 0.0f32;
                let mut cnt = 0usize;
                for y in y0..y1 {
                    let row = y * self.w;
                    for x in x0..x1 {
                        sum += self.data[row + x];
                        cnt += 1;
                    }
                }
                if cnt > 0 {
                    data[ny * nw + nx] = sum / cnt as f32;
                }
            }
        }
        Self {
            w: nw,
            h: nh,
            data,
        }
    }

    fn crop(&self, x0: usize, y0: usize, cw: usize, ch: usize) -> Self {
        let mut data = Vec::with_capacity(cw * ch);
        for y in y0..(y0 + ch) {
            let row = y * self.w;
            data.extend_from_slice(&self.data[(row + x0)..(row + x0 + cw)]);
        }
        Self {
            w: cw,
            h: ch,
            data,
        }
    }

    fn rotate_90_cw(&self) -> Self {
        let nw = self.h;
        let nh = self.w;
        let mut data = vec![0.0f32; nw * nh];
        for y in 0..self.h {
            for x in 0..self.w {
                let nx = self.h - 1 - y;
                let ny = x;
                data[ny * nw + nx] = self.data[y * self.w + x];
            }
        }
        Self {
            w: nw,
            h: nh,
            data,
        }
    }
}

struct IntegralStats {
    w: usize,
    sum: Vec<f64>,
    sq_sum: Vec<f64>,
}

impl IntegralStats {
    fn build(img: &GrayImage) -> Self {
        let stride = img.w + 1;
        let mut sum = vec![0.0f64; stride * (img.h + 1)];
        let mut sq_sum = vec![0.0f64; stride * (img.h + 1)];
        for y in 0..img.h {
            let mut row_s = 0.0f64;
            let mut row_sq = 0.0f64;
            for x in 0..img.w {
                let v = img.data[y * img.w + x] as f64;
                row_s += v;
                row_sq += v * v;
                let idx = (y + 1) * stride + (x + 1);
                let above = y * stride + (x + 1);
                sum[idx] = sum[above] + row_s;
                sq_sum[idx] = sq_sum[above] + row_sq;
            }
        }
        Self {
            w: stride,
            sum,
            sq_sum,
        }
    }

    #[inline]
    fn rect_sums(&self, x: usize, y: usize, rw: usize, rh: usize) -> (f64, f64) {
        let x2 = x + rw;
        let y2 = y + rh;
        let a = y * self.w + x;
        let b = y * self.w + x2;
        let c = y2 * self.w + x;
        let d = y2 * self.w + x2;
        let s = self.sum[d] - self.sum[b] - self.sum[c] + self.sum[a];
        let sq = self.sq_sum[d] - self.sq_sum[b] - self.sq_sum[c] + self.sq_sum[a];
        (s, sq)
    }
}

struct PreparedTemplate {
    w: usize,
    h: usize,
    zero_mean: Vec<f32>,
    norm: f64,
    ink_sum: f64,
}

impl PreparedTemplate {
    fn from_gray(img: &GrayImage) -> Option<Self> {
        let n = img.data.len();
        if n < 9 {
            return None;
        }
        let ink_sum: f64 = img.data.iter().map(|&v| v as f64).sum();
        let mean = (ink_sum / n as f64) as f32;
        let mut zero_mean = Vec::with_capacity(n);
        let mut norm_sq = 0.0f64;
        for &v in &img.data {
            let d = v - mean;
            zero_mean.push(d);
            norm_sq += (d as f64) * (d as f64);
        }
        let norm = norm_sq.sqrt();
        if norm < 1e-3 {
            return None;
        }
        Some(Self {
            w: img.w,
            h: img.h,
            zero_mean,
            norm,
            ink_sum,
        })
    }

    #[inline]
    fn zncc_at(&self, img: &GrayImage, integral: &IntegralStats, x: usize, y: usize) -> f32 {
        let (ws, wsq) = integral.rect_sums(x, y, self.w, self.h);
        // Fast reject empty or wildly different-ink windows
        if ws < self.ink_sum * 0.25 || ws > self.ink_sum * 3.5 + 1.0 {
            return -1.0;
        }
        let n = (self.w * self.h) as f64;
        let var_sum = (wsq - (ws * ws) / n).max(0.0);
        if var_sum < 1e-6 {
            return -1.0;
        }
        let denom = self.norm * var_sum.sqrt();
        if denom < 1e-6 {
            return -1.0;
        }
        let mut dot = 0.0f64;
        for ty in 0..self.h {
            let irow = (y + ty) * img.w + x;
            let trow = ty * self.w;
            for tx in 0..self.w {
                dot += (img.data[irow + tx] as f64) * (self.zero_mean[trow + tx] as f64);
            }
        }
        (dot / denom) as f32
    }
}

/// Runs Bluebeam-style Visual Search on a rendered PDF page, finding every occurrence of the
/// symbol inside `template_rect_pt` using 2-stage coarse-to-fine ZNCC + NMS.
/// Returns all candidates scoring `>= min_score` (capped at 250 highest matches).
pub fn run_visual_search_on_page(
    raw_pdf_bytes: &[u8],
    page_index: usize,
    page_w_pt: f64,
    page_h_pt: f64,
    template_rect_pt: Rect2,
    min_score: f32,
    check_rotations: bool,
) -> Result<VisualSearchResult, String> {
    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();
    let rendered = render_page_rgba(raw_pdf_bytes, page_index, 1400, &interp, &rs)?;
    run_visual_search_on_image(
        &rendered,
        page_index,
        page_w_pt,
        page_h_pt,
        template_rect_pt,
        min_score,
        check_rotations,
    )
}

/// Runs Visual Search directly on a rendered RGBA page image (also testable without PDF overhead).
pub fn run_visual_search_on_image(
    rendered: &RgbaImageData,
    page_index: usize,
    page_w_pt: f64,
    page_h_pt: f64,
    template_rect_pt: Rect2,
    min_score: f32,
    check_rotations: bool,
) -> Result<VisualSearchResult, String> {
    let gray = GrayImage::from_rgba(rendered);
    if gray.w < 16 || gray.h < 16 {
        return Err("La imagen de la página es demasiado pequeña para búsqueda visual.".to_string());
    }
    let sx = gray.w as f64 / page_w_pt.max(1.0);
    let sy = gray.h as f64 / page_h_pt.max(1.0);

    let x0 = ((template_rect_pt.min_x * sx).floor() as isize).clamp(0, gray.w as isize - 1) as usize;
    let y0 = ((template_rect_pt.min_y * sy).floor() as isize).clamp(0, gray.h as isize - 1) as usize;
    let x1 = ((template_rect_pt.max_x * sx).ceil() as isize).clamp(0, gray.w as isize) as usize;
    let y1 = ((template_rect_pt.max_y * sy).ceil() as isize).clamp(0, gray.h as isize) as usize;
    let tw = x1.saturating_sub(x0);
    let th = y1.saturating_sub(y0);
    if tw < 5 || th < 5 {
        return Err("El recorte seleccionado es muy pequeño; seleccione un símbolo completo.".to_string());
    }
    if tw > gray.w / 2 || th > gray.h / 2 {
        return Err("El recorte seleccionado es demasiado grande para búsqueda de símbolos.".to_string());
    }

    let base_tpl = gray.crop(x0, y0, tw, th);
    let mut tpl_variants: Vec<(GrayImage, u16)> = vec![(base_tpl, 0)];
    if check_rotations {
        let r90 = tpl_variants[0].0.rotate_90_cw();
        let r180 = r90.rotate_90_cw();
        let r270 = r180.rotate_90_cw();
        tpl_variants.push((r90, 90));
        tpl_variants.push((r180, 180));
        tpl_variants.push((r270, 270));
    }

    let ds: usize = if tw.min(th) >= 48 {
        3
    } else if tw.min(th) >= 18 {
        2
    } else {
        1
    };

    let full_integral = IntegralStats::build(&gray);
    let coarse_img = gray.downsample(ds);
    let coarse_integral = IntegralStats::build(&coarse_img);
    let coarse_thresh = (min_score - 0.16).clamp(0.35, 0.85);

    let mut raw_hits: Vec<(usize, usize, usize, usize, f32, u16)> = Vec::new();

    for (tpl_gray, rot_deg) in &tpl_variants {
        let Some(full_tpl) = PreparedTemplate::from_gray(tpl_gray) else {
            continue;
        };
        if full_tpl.w >= gray.w || full_tpl.h >= gray.h {
            continue;
        }
        let max_fx = gray.w - full_tpl.w;
        let max_fy = gray.h - full_tpl.h;

        if ds <= 1 {
            for y in 0..=max_fy {
                for x in 0..=max_fx {
                    let sc = full_tpl.zncc_at(&gray, &full_integral, x, y);
                    if sc >= min_score {
                        raw_hits.push((x, y, full_tpl.w, full_tpl.h, sc, *rot_deg));
                    }
                }
            }
        } else {
            let c_tpl_gray = tpl_gray.downsample(ds);
            let Some(c_tpl) = PreparedTemplate::from_gray(&c_tpl_gray) else {
                continue;
            };
            if c_tpl.w >= coarse_img.w || c_tpl.h >= coarse_img.h {
                continue;
            }
            let max_cx = coarse_img.w - c_tpl.w;
            let max_cy = coarse_img.h - c_tpl.h;
            for cy in 0..=max_cy {
                for cx in 0..=max_cx {
                    let c_sc = c_tpl.zncc_at(&coarse_img, &coarse_integral, cx, cy);
                    if c_sc >= coarse_thresh {
                        // Refine at full resolution in the neighborhood of (cx*ds, cy*ds)
                        let rx0 = (cx * ds).saturating_sub(ds);
                        let ry0 = (cy * ds).saturating_sub(ds);
                        let rx1 = ((cx * ds) + ds).min(max_fx);
                        let ry1 = ((cy * ds) + ds).min(max_fy);
                        let mut best_sc = -1.0f32;
                        let mut best_xy = (rx0, ry0);
                        for y in ry0..=ry1 {
                            for x in rx0..=rx1 {
                                let sc = full_tpl.zncc_at(&gray, &full_integral, x, y);
                                if sc > best_sc {
                                    best_sc = sc;
                                    best_xy = (x, y);
                                }
                            }
                        }
                        if best_sc >= min_score {
                            raw_hits.push((
                                best_xy.0,
                                best_xy.1,
                                full_tpl.w,
                                full_tpl.h,
                                best_sc,
                                *rot_deg,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Non-Maximum Suppression sorted by descending ZNCC score
    raw_hits.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
    let min_center_dist = (tw.min(th) as f64 * 0.62).max(4.0);
    let min_center_dist_sq = min_center_dist * min_center_dist;

    let mut accepted: Vec<(f64, f64, VisualMatch)> = Vec::new();
    for (x, y, w, h, score, rot) in raw_hits {
        let cx = x as f64 + w as f64 * 0.5;
        let cy = y as f64 + h as f64 * 0.5;
        let overlaps = accepted.iter().any(|(ax, ay, _)| {
            let dx = cx - ax;
            let dy = cy - ay;
            dx * dx + dy * dy < min_center_dist_sq
        });
        if overlaps {
            continue;
        }
        let rect_pt = Rect2::from_min_max(
            Point2::new(x as f64 / sx, y as f64 / sy),
            Point2::new((x + w) as f64 / sx, (y + h) as f64 / sy),
        );
        accepted.push((
            cx,
            cy,
            VisualMatch {
                rect_pt,
                center_pt: rect_pt.center(),
                score: score.clamp(0.0, 1.0),
                rotation_deg: rot,
                selected: true,
            },
        ));
        if accepted.len() >= 250 {
            break;
        }
    }

    Ok(VisualSearchResult {
        page_index,
        template_rect_pt,
        matches: accepted.into_iter().map(|(_, _, m)| m).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_search_detects_repeated_cross_symbols_accurately() {
        // Synthesize a 240x180 white image containing 4 identical black "+" electrical symbols
        let w = 240usize;
        let h = 180usize;
        let mut rgba = vec![255u8; w * h * 4];
        let centers = [(40usize, 40usize), (120, 40), (190, 95), (70, 140)];
        let draw_symbol = |buf: &mut [u8], cx: usize, cy: usize| {
            for dy in -8isize..=8 {
                for dx in -8isize..=8 {
                    let on_ring = dx * dx + dy * dy >= 42 && dx * dx + dy * dy <= 68;
                    let on_cross = (dx.abs() <= 1 && dy.abs() <= 6) || (dy.abs() <= 1 && dx.abs() <= 6);
                    if on_ring || on_cross {
                        let px = (cx as isize + dx) as usize;
                        let py = (cy as isize + dy) as usize;
                        let off = (py * w + px) * 4;
                        buf[off] = 15;
                        buf[off + 1] = 15;
                        buf[off + 2] = 15;
                        buf[off + 3] = 255;
                    }
                }
            }
        };
        for &(cx, cy) in &centers {
            draw_symbol(&mut rgba, cx, cy);
        }
        let img = RgbaImageData {
            width: w,
            height: h,
            premul_rgba: rgba,
        };
        // Query around first symbol at (40, 40)
        let tpl = Rect2::from_xywh(28.0, 28.0, 24.0, 24.0);
        let res = run_visual_search_on_image(&img, 0, 240.0, 180.0, tpl, 0.78, false)
            .expect("visual search");
        assert_eq!(res.matches.len(), 4, "expected 4 exact symbol matches, got {:?}", res.matches);
        for &(cx, cy) in &centers {
            assert!(
                res.matches.iter().any(|m| {
                    (m.center_pt.x - cx as f64).abs() <= 2.0
                        && (m.center_pt.y - cy as f64).abs() <= 2.0
                }),
                "missing symbol at ({cx}, {cy})"
            );
        }
    }
}
