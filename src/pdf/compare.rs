use crate::geometry::math::{Point2, Rect2};
use crate::model::document::DocumentTab;
use crate::model::markup::{LineStyle, Markup, MarkupKind, MarkupStatus, MarkupStyle};
use crate::pdf::io::open_pdf_from_bytes;
use crate::pdf::renderer::{default_interpreter_settings, render_page_rgba};
use hayro::RenderSettings;
use hayro::hayro_syntax::Pdf;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct CompareReport {
    pub diff_regions_count: usize,
    pub added_pixels: usize,
    pub removed_pixels: usize,
}

/// Compares two PDF pages visually (Bluebeam Compare Documents style):
/// - Common linework is rendered in neutral gray
/// - Content present only in Document A (removed) is highlighted in Red
/// - Content present only in Document B (added) is highlighted in Blue
/// - Connected difference zones receive automatic red Cloud markups on the resulting tab
pub fn compare_pdf_pages(
    bytes_a: &[u8],
    page_a: usize,
    bytes_b: &[u8],
    page_b: usize,
    max_side_px: u32,
) -> Result<(DocumentTab, CompareReport), String> {
    let pdf_a = Pdf::new(bytes_a.to_vec()).map_err(|e| format!("PDF A inválido: {e:?}"))?;
    let (w_pt, h_pt) = pdf_a
        .pages()
        .get(page_a)
        .ok_or_else(|| "Página A fuera de rango".to_string())?
        .render_dimensions();
    let w_pt = w_pt as f64;
    let h_pt = h_pt as f64;

    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();
    let img_a = render_page_rgba(bytes_a, page_a, max_side_px, &interp, &rs)?;
    let img_b = render_page_rgba(bytes_b, page_b, max_side_px, &interp, &rs)?;

    let w = img_a.width;
    let h = img_a.height;
    if w == 0 || h == 0 || img_b.width == 0 || img_b.height == 0 {
        return Err("La página seleccionada no contiene píxeles válidos para comparar.".to_string());
    }
    let mut rgb = vec![255u8; w * h * 3];

    let cell_px = (w.max(h) / 48).max(16);
    let cols = w.div_ceil(cell_px);
    let rows = h.div_ceil(cell_px);
    let mut diff_counts = vec![0usize; cols * rows];
    let mut added_pixels = 0usize;
    let mut removed_pixels = 0usize;

    let sample_rgb = |img: &crate::pdf::renderer::RgbaImageData, x: usize, y: usize| -> (u8, u8, u8) {
        let sx = (x * img.width / w.max(1)).min(img.width.saturating_sub(1));
        let sy = (y * img.height / h.max(1)).min(img.height.saturating_sub(1));
        let off = (sy * img.width + sx) * 4;
        (
            img.premul_rgba[off],
            img.premul_rgba[off + 1],
            img.premul_rgba[off + 2],
        )
    };

    for y in 0..h {
        for x in 0..w {
            let (ra, ga, ba) = sample_rgb(&img_a, x, y);
            let (rb, gb, bb) = sample_rgb(&img_b, x, y);
            let lum_a = 0.299 * ra as f32 + 0.587 * ga as f32 + 0.114 * ba as f32;
            let lum_b = 0.299 * rb as f32 + 0.587 * gb as f32 + 0.114 * bb as f32;
            let ink_a = lum_a < 242.0;
            let ink_b = lum_b < 242.0;
            let ch_delta = ra.abs_diff(rb).max(ga.abs_diff(gb)).max(ba.abs_diff(bb));
            let dst = (y * w + x) * 3;
            match (ink_a, ink_b) {
                (true, true) if ch_delta <= 32 => {
                    rgb[dst] = 148;
                    rgb[dst + 1] = 154;
                    rgb[dst + 2] = 166;
                }
                (true, true) => {
                    // Color/shading changed between revisions -> Purple-Blue highlight
                    rgb[dst] = 124;
                    rgb[dst + 1] = 58;
                    rgb[dst + 2] = 237;
                    added_pixels += 1;
                    diff_counts[(y / cell_px) * cols + (x / cell_px)] += 1;
                }
                (true, false) => {
                    // Removed from A -> Crimson Red
                    rgb[dst] = 225;
                    rgb[dst + 1] = 45;
                    rgb[dst + 2] = 45;
                    removed_pixels += 1;
                    diff_counts[(y / cell_px) * cols + (x / cell_px)] += 1;
                }
                (false, true) => {
                    // Added in B -> Blue
                    rgb[dst] = 32;
                    rgb[dst + 1] = 102;
                    rgb[dst + 2] = 235;
                    added_pixels += 1;
                    diff_counts[(y / cell_px) * cols + (x / cell_px)] += 1;
                }
                (false, false) => {}
            }
        }
    }

    // Group adjacent cells with >3 changed pixels into bounding rectangles
    let mut visited = vec![false; cols * rows];
    let mut regions_pt = Vec::new();
    for ry in 0..rows {
        for rx in 0..cols {
            let idx = ry * cols + rx;
            if visited[idx] || diff_counts[idx] < 4 {
                continue;
            }
            visited[idx] = true;
            let mut q = VecDeque::new();
            q.push_back((rx, ry));
            let (mut min_cx, mut min_cy, mut max_cx, mut max_cy) = (rx, ry, rx, ry);
            while let Some((cx, cy)) = q.pop_front() {
                min_cx = min_cx.min(cx);
                min_cy = min_cy.min(cy);
                max_cx = max_cx.max(cx);
                max_cy = max_cy.max(cy);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < cols && (ny as usize) < rows {
                            let nidx = ny as usize * cols + nx as usize;
                            if !visited[nidx] && diff_counts[nidx] >= 4 {
                                visited[nidx] = true;
                                q.push_back((nx as usize, ny as usize));
                            }
                        }
                    }
                }
            }
            let x0 = (min_cx * cell_px) as f64 / w as f64 * w_pt;
            let y0 = (min_cy * cell_px) as f64 / h as f64 * h_pt;
            let x1 = (((max_cx + 1) * cell_px).min(w)) as f64 / w as f64 * w_pt;
            let y1 = (((max_cy + 1) * cell_px).min(h)) as f64 / h as f64 * h_pt;
            regions_pt.push(Rect2::from_min_max(Point2::new(x0, y0), Point2::new(x1, y1)).expand(6.0));
        }
    }

    // Build 1-page PDF embedding the comparison image
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
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
    let img_id = doc.add_object(img_stream);

    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                w_pt.into(),
                0.0.into(),
                0.0.into(),
                h_pt.into(),
                0.0.into(),
                0.0.into(),
            ],
        ),
        Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
        Operation::new("Q", vec![]),
    ];
    let content_bytes = Content { operations: ops }.encode().unwrap_or_default();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content_bytes));
    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! { "Im0" => img_id },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), w_pt.into(), h_pt.into()],
        "Resources" => resources_id,
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1i64,
            "Kids" => vec![page_id.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut out_bytes = Vec::new();
    doc.save_to(&mut out_bytes)
        .map_err(|e| format!("Error generando PDF comparado: {e}"))?;

    let mut tab = open_pdf_from_bytes("Comparación_Planos.pdf", None, out_bytes)?;
    let diff_count = regions_pt.len();
    for (idx, rect) in regions_pt.into_iter().enumerate() {
        let mut cloud = Markup::new(
            0,
            "Diferencia detectada",
            "Comparador RedBeam",
            MarkupStyle {
                stroke_color: [229, 57, 53, 255],
                fill_color: Some([229, 57, 53, 255]),
                fill_opacity: 0.08,
                line_style: LineStyle::Cloud,
                line_width: 2.0,
                ..Default::default()
            },
            MarkupKind::Cloud {
                points: rect.corners().to_vec(),
                is_rect: true,
            },
        );
        cloud.label = format!("Dif. #{}", idx + 1);
        cloud.status = MarkupStatus::Pending;
        cloud.comment = "Zona con cambios entre Plano A (rojo) y Plano B (azul)".to_string();
        tab.markups.push(cloud);
    }
    tab.dirty = true;

    Ok((
        tab,
        CompareReport {
            diff_regions_count: diff_count,
            added_pixels,
            removed_pixels,
        },
    ))
}
