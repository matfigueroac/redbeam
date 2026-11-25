use crate::geometry::math::Rect2;
use crate::model::document::DocumentTab;
use crate::model::markup::MarkupKind;
use crate::pdf::demo_blueprint::to_winansi_bytes;
use crate::pdf::renderer::{default_interpreter_settings, render_page_rgba};
use hayro::RenderSettings;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct RedactionReport {
    pub redactions_applied: usize,
    pub pages_sanitized: usize,
}

/// Applies all pending `MarkupKind::Redaction` marks destructively (`SCmdRedact`):
/// - Every page containing redaction boxes is rasterized at high resolution (2400 px max side),
///   every pixel inside each redaction rectangle is permanently overwritten with `fill_color`
///   (destroying all underlying text, vector paths, and raster images),
///   and the page's `/Contents` + `/Resources` are replaced with the sanitized image + optional
///   vector overlay label (e.g. `"CENSURADO"`).
/// - Pages without redactions remain 100% untouched vector PDF pages.
/// - Clears `/RedBIMFlattened` recovery metadata on the document so redacted content can never be recovered.
pub fn apply_redactions_in_tab(
    tab: &mut DocumentTab,
    fill_color: [u8; 3],
) -> Result<RedactionReport, String> {
    let mut by_page: BTreeMap<usize, Vec<(Rect2, String)>> = BTreeMap::new();
    for m in &tab.markups {
        if let MarkupKind::Redaction {
            rect,
            overlay_text,
        } = &m.kind
        {
            by_page
                .entry(m.page_index)
                .or_default()
                .push((*rect, overlay_text.clone()));
        }
    }
    if by_page.is_empty() {
        return Err("No hay zonas marcadas para redacción (censura) en este documento.".to_string());
    }

    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let pages_map = doc.get_pages();
    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });

    let mut total_applied = 0usize;
    let mut pages_sanitized = 0usize;

    for (&page_idx, boxes) in &by_page {
        let page_number = u32::try_from(page_idx).ok().and_then(|n| n.checked_add(1));
        let page_id = *page_number.and_then(|n| pages_map.get(&n))
            .ok_or_else(|| format!("Página de censura inexistente: {}", page_idx.saturating_add(1)))?;
        let page_model = tab.pages.get(page_idx)
            .ok_or_else(|| "La página de censura ya no está en el plano.".to_string())?;
        let w_pt = page_model.width_pt;
        let h_pt = page_model.height_pt;

        let rendered = render_page_rgba(&tab.raw_pdf_bytes, page_idx, 2400, &interp, &rs)?;
        let w = rendered.width;
        let h = rendered.height;
        if w == 0 || h == 0 {
            return Err("No se pudo renderizar la página de censura.".to_string());
        }
        let sx = w as f64 / w_pt.max(1.0);
        let sy = h as f64 / h_pt.max(1.0);

        // Build opaque RGB buffer from rendered page and permanently overwrite redacted pixels
        let mut rgb = vec![255u8; w * h * 3];
        for (i, px) in rendered.premul_rgba.as_chunks::<4>().0.iter().enumerate() {
            let dst = i * 3;
            rgb[dst] = px[0];
            rgb[dst + 1] = px[1];
            rgb[dst + 2] = px[2];
        }

        let [fr, fg, fb] = fill_color;
        for (rect, _) in boxes {
            let x0 = ((rect.min_x * sx).floor() as isize).clamp(0, w as isize) as usize;
            let y0 = ((rect.min_y * sy).floor() as isize).clamp(0, h as isize) as usize;
            let x1 = ((rect.max_x * sx).ceil() as isize).clamp(0, w as isize) as usize;
            let y1 = ((rect.max_y * sy).ceil() as isize).clamp(0, h as isize) as usize;
            if x0 >= x1 || y0 >= y1 || rect.width() <= 0.0 || rect.height() <= 0.0
                || ![rect.min_x, rect.min_y, rect.max_x, rect.max_y].iter().all(|v| v.is_finite()) {
                return Err(format!("Hay una zona de censura vacía o fuera de la página {}.", page_idx + 1));
            }
            for y in y0..y1 {
                let row = y * w;
                for x in x0..x1 {
                    let dst = (row + x) * 3;
                    rgb[dst] = fr;
                    rgb[dst + 1] = fg;
                    rgb[dst + 2] = fb;
                }
            }
            total_applied += 1;
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
        let img_id = doc.add_object(img_stream);

        let mut ops = vec![
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
            Operation::new("Do", vec![Object::Name(b"RD_IMG".to_vec())]),
            Operation::new("Q", vec![]),
        ];

        // Optional crisp white overlay text (e.g. "CENSURADO") centered inside each box
        let text_lum = 0.299 * fr as f64 + 0.587 * fg as f64 + 0.114 * fb as f64;
        let tc = if text_lum < 128.0 { 1.0 } else { 0.0 };
        for (rect, label) in boxes {
            let trimmed = label.trim();
            if trimmed.is_empty() {
                continue;
            }
            let fs = (rect.height() * 0.42).clamp(7.0, 18.0);
            let approx_w = trimmed.chars().count() as f64 * fs * 0.55;
            let tx = (rect.min_x + (rect.width() - approx_w) * 0.5).max(rect.min_x + 3.0);
            let ty = h_pt - (rect.min_y + rect.height() * 0.5 + fs * 0.35);
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new("rg", vec![tc.into(), tc.into(), tc.into()]));
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"RD_F1".to_vec()), fs.into()],
            ));
            ops.push(Operation::new("Td", vec![tx.into(), ty.into()]));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    to_winansi_bytes(trimmed),
                    lopdf::StringFormat::Literal,
                )],
            ));
            ops.push(Operation::new("ET", vec![]));
            ops.push(Operation::new("Q", vec![]));
        }

        let content_bytes = Content { operations: ops }.encode().map_err(|e| e.to_string())?;
        let mut cstream = Stream::new(dictionary! {}, content_bytes);
        let _ = cstream.compress();
        let content_id = doc.add_object(cstream);

        // Replace page dictionary with normalized unrotated MediaBox matching the sanitized raster
        let page_dict = doc.get_dictionary_mut(page_id).map_err(|e| e.to_string())?;
        page_dict.set("MediaBox", vec![0.0.into(), 0.0.into(), w_pt.into(), h_pt.into()]);
        page_dict.remove(b"CropBox");
        page_dict.set("Rotate", 0i64);
        page_dict.set("Contents", content_id);
        page_dict.set(
            "Resources",
            dictionary! {
                "XObject" => dictionary! { "RD_IMG" => img_id },
                "Font" => dictionary! { "RD_F1" => font_id },
            },
        );
        page_dict.remove(b"Annots");
        pages_sanitized += 1;
    }

    // Security hygiene: strip any recoverable flattened state and prune orphaned original streams
    let cat_id = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let catalog = doc.get_dictionary_mut(cat_id).map_err(|e| e.to_string())?;
    catalog.remove(b"RedBIMFlattened");
    catalog.remove(b"RedBIMState");
    doc.prune_objects();

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error guardando PDF redactado: {e}"))?;
    // Commit the PDF and its corresponding model only after every page succeeds.
    tab.raw_pdf_bytes = out.into();
    for page_idx in by_page.keys().copied() {
        let page = &mut tab.pages[page_idx];
        page.pdf_crop_box = Some([0.0, 0.0, page.width_pt, page.height_pt]);
        page.pdf_native_rotation = 0;
        page.rotation_deg = 0;
        if let Some(text) = tab.search_text_by_page.get_mut(page_idx) {
            text.clear();
        }
    }

    // Remove applied Redaction marks from live markups
    tab.markups
        .retain(|m| !matches!(m.kind, MarkupKind::Redaction { .. }));
    tab.selected_ids
        .retain(|id| tab.markups.iter().any(|m| m.id == *id));
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;

    Ok(RedactionReport {
        redactions_applied: total_applied,
        pages_sanitized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::markup::{Markup, MarkupStyle};
    use crate::pdf::{demo_blueprint::create_blank_pdf_bytes, io::open_pdf_from_bytes};

    #[test]
    fn invalid_second_page_keeps_original_document_and_pending_redactions() {
        let bytes = create_blank_pdf_bytes(300.0, 200.0, 1);
        let mut tab = open_pdf_from_bytes("prueba", None, bytes.clone()).unwrap();
        for page in [0, 1] {
            tab.markups.push(Markup::new(page, "Zona", "Autor", MarkupStyle::default(),
                MarkupKind::Redaction {
                    rect: Rect2::from_xywh(2.0, 3.0, 10.0, 10.0),
                    overlay_text: String::new(),
                }));
        }
        tab.search_text_by_page[0] = "Texto del plano".into();
        let crop = tab.pages[0].pdf_crop_box;
        assert!(apply_redactions_in_tab(&mut tab, [0, 0, 0]).is_err());
        assert_eq!(tab.raw_pdf_bytes.as_slice(), bytes.as_slice());
        assert_eq!(tab.search_text_by_page[0], "Texto del plano");
        assert_eq!(tab.pages[0].pdf_crop_box, crop);
        assert_eq!(tab.markups.len(), 2);
    }
}
