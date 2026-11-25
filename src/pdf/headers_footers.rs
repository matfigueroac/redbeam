use crate::model::document::DocumentTab;
use crate::model::scale::parse_page_range;
use crate::model::units::PT_PER_MM;
use crate::pdf::demo_blueprint::to_winansi_bytes;
use chrono::Local;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

#[derive(Debug, Clone, PartialEq)]
pub struct HeaderFooterConfig {
    pub top_left: String,
    pub top_center: String,
    pub top_right: String,
    pub bottom_left: String,
    pub bottom_center: String,
    pub bottom_right: String,
    pub font_size_pt: f64,
    pub margin_mm: f64,
    pub color: [u8; 3],
    /// Empty string = all pages; otherwise e.g. `"1-3, 5"`.
    pub page_range: String,
}

impl Default for HeaderFooterConfig {
    fn default() -> Self {
        Self {
            top_left: String::new(),
            top_center: String::new(),
            top_right: "<<Date>>".to_string(),
            bottom_left: "<<File>>".to_string(),
            bottom_center: "<<Label>>".to_string(),
            bottom_right: "Pág. <<Page>> / <<TotalPages>>".to_string(),
            font_size_pt: 9.5,
            margin_mm: 10.0,
            color: [30, 41, 59],
            page_range: String::new(),
        }
    }
}

impl HeaderFooterConfig {
    pub fn is_empty(&self) -> bool {
        self.top_left.trim().is_empty()
            && self.top_center.trim().is_empty()
            && self.top_right.trim().is_empty()
            && self.bottom_left.trim().is_empty()
            && self.bottom_center.trim().is_empty()
            && self.bottom_right.trim().is_empty()
    }
}

/// Expands Bluebeam-style Header & Footer dynamic tokens (`<<Page>>`, `<<TotalPages>>`,
/// `<<Label>>`, `<<Date>>`, `<<File>>`).
pub fn expand_hf_tokens(
    template: &str,
    page_num_1based: usize,
    total_pages: usize,
    page_label: &str,
    file_name: &str,
    date_str: &str,
) -> String {
    template
        .replace("<<Page>>", &page_num_1based.to_string())
        .replace("<<TotalPages>>", &total_pages.to_string())
        .replace("<<Label>>", page_label)
        .replace("<<File>>", file_name)
        .replace("<<Date>>", date_str)
}

fn is_hf_overlay_stream(doc: &Document, cid: ObjectId) -> bool {
    doc.get_object(cid)
        .ok()
        .and_then(|o| o.as_stream().ok())
        .is_some_and(|st| {
            if st
                .dict
                .get(b"RedBIMHeaderFooterOverlay")
                .and_then(|o| o.as_bool())
                .unwrap_or(false)
            {
                return true;
            }
            let raw = st
                .decompressed_content()
                .unwrap_or_else(|_| st.content.clone());
            raw.windows(b"/RedBIM_HeaderFooter".len())
                .any(|w| w == b"/RedBIM_HeaderFooter")
        })
}

fn resolve_page_resources(doc: &Document, page_id: ObjectId) -> Dictionary {
    let mut cur = Some(page_id);
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 32 {
            break;
        }
        let Ok(d) = doc.get_dictionary(id) else {
            break;
        };
        if let Ok(r_obj) = d.get(b"Resources") {
            match r_obj {
                Object::Dictionary(rd) => return rd.clone(),
                Object::Reference(rid) => {
                    if let Ok(rd) = doc.get_dictionary(*rid) {
                        return rd.clone();
                    }
                }
                _ => {}
            }
        }
        cur = d.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    }
    Dictionary::new()
}

/// Applies vector Headers & Footers directly to PDF page content streams using Marked Content
/// (`/RedBIM_HeaderFooter BDC ... EMC`). Strips any prior RedBIM headers/footers first so
/// repeated applications never stack or duplicate text.
pub fn apply_headers_footers_in_tab(
    tab: &mut DocumentTab,
    cfg: &HeaderFooterConfig,
) -> Result<usize, String> {
    if cfg.is_empty() {
        return Err("Ingrese al menos un texto en Encabezado o Pie de página.".to_string());
    }
    let total_pages = tab.pages.len();
    if total_pages == 0 {
        return Err("El documento no contiene páginas.".to_string());
    }

    let target_pages: std::collections::HashSet<usize> = if cfg.page_range.trim().is_empty() {
        (0..total_pages).collect()
    } else {
        let parsed = parse_page_range(&cfg.page_range, total_pages);
        if parsed.is_empty() {
            return Err("Rango de páginas inválido (ej: 1-3, 5).".to_string());
        }
        parsed.into_iter().collect()
    };

    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let q_push_id = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
    let q_pop_id = doc.add_object(Stream::new(dictionary! {}, b"\nQ\n".to_vec()));

    let date_str = Local::now().format("%d/%m/%Y").to_string();
    let file_name = tab.title.clone();
    let font_sz = cfg.font_size_pt.clamp(6.0, 32.0);
    let margin_pt = (cfg.margin_mm * PT_PER_MM).clamp(8.0, 120.0);
    let [cr, cg, cb] = cfg.color;
    let r = cr as f64 / 255.0;
    let g = cg as f64 / 255.0;
    let b = cb as f64 / 255.0;

    let pages_map = doc.get_pages();
    let mut applied = 0usize;

    for (&page_num, &page_id) in &pages_map {
        let idx = page_num.saturating_sub(1) as usize;
        if !target_pages.contains(&idx) {
            continue;
        }
        let Some(page_model) = tab.pages.get(idx) else {
            continue;
        };

        let w = page_model.width_pt;
        let h = page_model.height_pt;
        let page_lbl = page_model.display_label();

        let expand = |tpl: &str| -> String {
            expand_hf_tokens(tpl, idx + 1, total_pages, &page_lbl, &file_name, &date_str)
        };

        let slots = [
            (expand(&cfg.top_left), true, 0u8),
            (expand(&cfg.top_center), true, 1u8),
            (expand(&cfg.top_right), true, 2u8),
            (expand(&cfg.bottom_left), false, 0u8),
            (expand(&cfg.bottom_center), false, 1u8),
            (expand(&cfg.bottom_right), false, 2u8),
        ];

        let mut ops = Vec::new();
        ops.push(Operation::new(
            "BDC",
            vec![
                Object::Name(b"RedBIM_HeaderFooter".to_vec()),
                Object::Dictionary(Dictionary::new()),
            ],
        ));
        ops.push(Operation::new("q", vec![]));
        if let Some([a, bm, c, d, e, f]) = page_model.pdf_overlay_cm_matrix() {
            ops.push(Operation::new(
                "cm",
                vec![a.into(), bm.into(), c.into(), d.into(), e.into(), f.into()],
            ));
        }
        ops.push(Operation::new("rg", vec![r.into(), g.into(), b.into()]));

        for (text, is_top, align) in slots {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Approximate Helvetica glyph width for clean center/right alignment
            let approx_w = trimmed.chars().count() as f64 * font_sz * 0.52;
            let x = match align {
                0 => margin_pt,
                1 => ((w - approx_w) * 0.5).max(margin_pt),
                _ => (w - margin_pt - approx_w).max(margin_pt),
            };
            // In normalized overlay coordinates (`oy = h - vy`), top is near `h - margin_pt`
            let y = if is_top {
                (h - margin_pt - font_sz).max(8.0)
            } else {
                margin_pt.min(h - font_sz - 8.0).max(6.0)
            };

            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"RB_HF".to_vec()), font_sz.into()],
            ));
            ops.push(Operation::new("Td", vec![x.into(), y.into()]));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(
                    to_winansi_bytes(trimmed),
                    lopdf::StringFormat::Literal,
                )],
            ));
            ops.push(Operation::new("ET", vec![]));
        }

        ops.push(Operation::new("Q", vec![]));
        ops.push(Operation::new("EMC", vec![]));

        let overlay_bytes = Content { operations: ops }.encode().unwrap_or_default();
        let mut overlay_stream = Stream::new(
            dictionary! { "RedBIMHeaderFooterOverlay" => true },
            overlay_bytes,
        );
        let _ = overlay_stream.compress();
        let overlay_id = doc.add_object(overlay_stream);

        // Strip any prior RedBIM header/footer overlay on this page first
        let existing: Vec<ObjectId> = doc
            .get_page_contents(page_id)
            .into_iter()
            .filter(|&cid| !is_hf_overlay_stream(&doc, cid))
            .collect();

        let mut new_contents = Vec::with_capacity(existing.len() + 3);
        if !existing.is_empty() {
            new_contents.push(Object::Reference(q_push_id));
            new_contents.extend(existing.into_iter().map(Object::Reference));
            new_contents.push(Object::Reference(q_pop_id));
        }
        new_contents.push(Object::Reference(overlay_id));

        let mut res_dict = resolve_page_resources(&doc, page_id);
        let mut fonts = match res_dict.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => doc.get_dictionary(*id).cloned().unwrap_or_default(),
            _ => Dictionary::new(),
        };
        fonts.set("RB_HF", font_id);
        res_dict.set("Font", Object::Dictionary(fonts));

        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            page_dict.set("Contents", new_contents);
            page_dict.set("Resources", Object::Dictionary(res_dict));
        }
        applied += 1;
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error aplicando encabezados y pies: {e}"))?;
    tab.raw_pdf_bytes = out.into();
    tab.dirty = true;
    Ok(applied)
}

/// Strips all RedBIM Header & Footer overlays from every page of `tab`.
pub fn remove_headers_footers_in_tab(tab: &mut DocumentTab) -> Result<usize, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let mut removed = 0usize;
    for &page_id in doc.get_pages().values() {
        let existing = doc.get_page_contents(page_id);
        let before = existing.len();
        let kept: Vec<Object> = existing
            .into_iter()
            .filter(|&cid| !is_hf_overlay_stream(&doc, cid))
            .map(Object::Reference)
            .collect();
        if kept.len() < before {
            removed += before - kept.len();
            if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                page_dict.set("Contents", kept);
            }
        }
    }
    if removed == 0 {
        return Err("No se encontraron encabezados/pies de RedBeam para quitar.".to_string());
    }
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error quitando encabezados/pies: {e}"))?;
    tab.raw_pdf_bytes = out.into();
    tab.dirty = true;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_tokens_expand_cleanly() {
        let s = expand_hf_tokens(
            "<<File>> | <<Label>> | Pág. <<Page>> de <<TotalPages>> (<<Date>>)",
            2,
            10,
            "ARQ-02 · Cortes",
            "Edificio.pdf",
            "23/11/2026",
        );
        assert_eq!(
            s,
            "Edificio.pdf | ARQ-02 · Cortes | Pág. 2 de 10 (23/11/2026)"
        );
    }
}
