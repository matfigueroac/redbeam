use crate::geometry::math::{Point2, Rect2};
use crate::model::document::{
    BookmarkNode, DocumentTab, PageModel, RedBimEmbeddedState, SpaceRegion,
};
use crate::model::markup::{
    HyperlinkTarget, LineEnd, LineStyle, Markup, MarkupKind, MarkupStyle, NoteIcon,
};
use crate::model::scale::{PageScale, ViewportScale};
use crate::model::units::LengthUnit;
use crate::pdf::demo_blueprint::{create_demo_markups, generate_demo_chile_pdf};
use hayro::hayro_syntax::Pdf;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Encodes text as UTF-16BE with BOM (`FE FF`) per ISO 32000-1 PDF text strings.
fn pdf_utf16_string(s: &str) -> Object {
    let mut bytes = Vec::with_capacity(2 + s.len() * 2);
    bytes.push(0xFE);
    bytes.push(0xFF);
    for u in s.encode_utf16() {
        bytes.push((u >> 8) as u8);
        bytes.push((u & 0xFF) as u8);
    }
    Object::string_literal(bytes)
}

/// Decodes PDF string literal (UTF-16BE with BOM, UTF-8 with BOM, or PDFDocEncoding/WinAnsi) to Rust String.
fn decode_pdf_string(obj: &Object) -> String {
    let Ok(bytes) = obj.as_str() else {
        return String::new();
    };
    if bytes.starts_with(&[0xFE, 0xFF]) && bytes.len() >= 2 {
        let u16s: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    bytes.iter().map(|&b| b as char).collect()
}

pub fn open_demo_chile_document() -> Result<DocumentTab, String> {
    let bytes = generate_demo_chile_pdf();
    let mut tab = open_pdf_from_bytes("PLANOS_CHILE_DEMO.pdf", None, bytes)?;
    use crate::model::units::PT_PER_MM;
    if let Some(p0) = tab.pages.get_mut(0) {
        p0.scale = PageScale::from_metric_ratio(100.0);
        p0.label = Some("ARQ-01 · Planta Cubierta".to_string());
    }
    if let Some(p1) = tab.pages.get_mut(1) {
        p1.scale = PageScale::from_metric_ratio(50.0);
        p1.label = Some("ARQ-02 · Estructuras".to_string());
        p1.viewports.push(ViewportScale::new(
            "Detalle A (1:20)",
            Rect2::from_xywh(
                480.0 * PT_PER_MM,
                115.0 * PT_PER_MM,
                260.0 * PT_PER_MM,
                260.0 * PT_PER_MM,
            ),
            PageScale::from_metric_ratio(20.0),
        ));
    }
    if let Some(p2) = tab.pages.get_mut(2) {
        p2.scale = PageScale::from_metric_ratio(25.0);
        p2.label = Some("ARQ-03 · Detalles".to_string());
    }
    // Starter Spaces on Page 0 so users immediately experience Bluebeam-style Spaces
    let mut sp_norte = SpaceRegion::new(
        0,
        "Sector Norte · Nave",
        Rect2::from_xywh(
            45.0 * PT_PER_MM,
            110.0 * PT_PER_MM,
            595.0 * PT_PER_MM,
            130.0 * PT_PER_MM,
        )
        .corners()
        .to_vec(),
    );
    sp_norte.fill_color = [56, 189, 248, 255];
    let mut sp_sur = SpaceRegion::new(
        0,
        "Sector Sur · Oficinas",
        Rect2::from_xywh(
            45.0 * PT_PER_MM,
            245.0 * PT_PER_MM,
            595.0 * PT_PER_MM,
            145.0 * PT_PER_MM,
        )
        .corners()
        .to_vec(),
    );
    sp_sur.fill_color = [52, 211, 153, 255];
    tab.spaces = vec![sp_norte, sp_sur];
    tab.markups = create_demo_markups();
    tab.dirty = false;
    Ok(tab)
}

pub fn open_pdf_file(path: &Path) -> Result<DocumentTab, String> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("No se encontró el PDF: {}", path.display()),
        std::io::ErrorKind::PermissionDenied => format!("Sin permiso para leer: {}", path.display()),
        _ => format!("No se pudo leer {}: {e}", path.display()),
    })?;
    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Documento.pdf")
        .to_string();
    open_pdf_from_bytes(title, Some(path.to_path_buf()), bytes)
}

pub fn open_pdf_from_bytes(
    title: impl Into<String>,
    file_path: Option<std::path::PathBuf>,
    raw_bytes: impl Into<Arc<Vec<u8>>>,
) -> Result<DocumentTab, String> {
    let raw_bytes = raw_bytes.into();
    let hayro_pdf = Pdf::new(Arc::clone(&raw_bytes)).map_err(|e| format!("Error al abrir PDF: {e:?}"))?;
    let mut pages = Vec::new();
    for (idx, hp) in hayro_pdf.pages().iter().enumerate() {
        let (w, h) = hp.render_dimensions();
        pages.push(PageModel::new(idx, w as f64, h as f64));
    }
    drop(hayro_pdf);
    if pages.is_empty() {
        return Err("El archivo PDF no contiene páginas.".to_string());
    }

    let doc = Document::load_mem(&raw_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let page_map = doc.get_pages(); // BTreeMap<u32 (1-based), ObjectId>
    let mut page_id_to_idx: HashMap<ObjectId, usize> = HashMap::new();
    for (&num, &oid) in &page_map {
        let idx = num.saturating_sub(1) as usize;
        page_id_to_idx.insert(oid, idx);
        if let Some(pm) = pages.get_mut(idx)
            && let Some((box_arr, rot)) = read_page_box_and_rotation(&doc, oid)
        {
            pm.pdf_crop_box = Some(box_arr);
            pm.pdf_native_rotation = rot;
            pm.rotation_deg = rot;
        }
    }

    // Extract text per page lazily in background worker so opening huge PDFs is instant
    let search_text_by_page = vec![String::new(); pages.len()];

    // Read embedded lossless RedBIMState if present
    if let Some(state) = read_embedded_state(&doc) {
        let mut tab = DocumentTab::new_with_pages(title, file_path, raw_bytes, pages);
        for (i, saved_p) in state.pages.into_iter().enumerate() {
            if let Some(p) = tab.pages.get_mut(i) {
                p.scale = saved_p.scale;
                p.viewports = saved_p.viewports;
                p.rotation_deg = saved_p.rotation_deg;
                p.label = saved_p.label;
            }
        }
        tab.markups = state.markups;
        if !state.layers.is_empty() {
            tab.layers = state.layers;
        }
        if !state.bookmarks.is_empty() {
            tab.bookmarks = state.bookmarks;
        } else {
            tab.bookmarks = read_pdf_outlines(&doc, &page_id_to_idx);
        }
        tab.spaces = state.spaces;
        if !state.custom_columns.is_empty() {
            tab.custom_columns = state.custom_columns;
        }
        tab.search_text_by_page = search_text_by_page;
        return Ok(tab);
    }

    // Otherwise import standard PDF /Annots, /VP viewports & /Outlines
    let mut imported_markups = Vec::new();
    for (&num, &page_id) in &page_map {
        let idx = num.saturating_sub(1) as usize;
        let Some(page_model) = pages.get_mut(idx) else {
            continue;
        };
        let page_h = page_model.height_pt;
        if let Ok(mut viewports) = read_page_viewports(&doc, page_id, page_model) {
            let mut full_page = viewports
                .iter()
                .enumerate()
                .filter(|(_, vp)| vp.scale.is_set && covers_page(vp.rect, page_model));
            if let Some((index, _)) = full_page.next()
                && full_page.next().is_none()
            {
                page_model.scale = viewports.remove(index).scale;
            }
            page_model.viewports = viewports;
        }
        if let Ok(annots) = doc.get_page_annotations(page_id) {
            for dict in annots {
                if let Some(m) = import_pdf_annotation(&doc, dict, idx, page_h) {
                    imported_markups.push(m);
                }
            }
        }
    }

    let bookmarks = read_pdf_outlines(&doc, &page_id_to_idx);
    let mut tab = DocumentTab::new_with_pages(title, file_path, raw_bytes, pages);
    for m in &imported_markups {
        tab.ensure_layer(&m.layer);
    }
    tab.markups = imported_markups;
    tab.bookmarks = bookmarks;
    tab.search_text_by_page = search_text_by_page;
    Ok(tab)
}

fn read_page_box_and_rotation(doc: &Document, page_id: ObjectId) -> Option<([f64; 4], u16)> {
    let mat = materialize_page_dict(doc, page_id)?;
    let rot = mat
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360) as u16;
    let read_box = |key: &[u8]| -> Option<[f64; 4]> {
        let obj = mat.get(key).ok()?;
        let arr = match obj {
            Object::Array(a) => a,
            Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
            _ => return None,
        };
        if arr.len() < 4 {
            return None;
        }
        let x0 = read_number(&arr[0])?;
        let y0 = read_number(&arr[1])?;
        let x1 = read_number(&arr[2])?;
        let y1 = read_number(&arr[3])?;
        Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
    };
    let crop = read_box(b"CropBox").or_else(|| read_box(b"MediaBox"))?;
    Some((crop, rot))
}

fn read_embedded_state(doc: &Document) -> Option<RedBimEmbeddedState> {
    let catalog = doc.catalog().ok()?;
    let obj = catalog.get(b"RedBIMState").ok()?;
    let stream = match obj {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_stream().ok()?,
        Object::Stream(s) => s,
        _ => return None,
    };
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    serde_json::from_slice::<RedBimEmbeddedState>(&bytes).ok()
}

fn read_pdf_outlines(
    doc: &Document,
    page_id_to_idx: &HashMap<ObjectId, usize>,
) -> Vec<BookmarkNode> {
    let Ok(catalog) = doc.catalog() else {
        return Vec::new();
    };
    let Ok(outlines_dict) = doc.get_dict_in_dict(catalog, b"Outlines") else {
        return Vec::new();
    };
    let Ok(first_obj) = outlines_dict.get(b"First") else {
        return Vec::new();
    };
    walk_outline_siblings(doc, first_obj, page_id_to_idx, 0)
}

fn walk_outline_siblings(
    doc: &Document,
    start_obj: &Object,
    page_id_to_idx: &HashMap<ObjectId, usize>,
    depth: usize,
) -> Vec<BookmarkNode> {
    if depth > 12 {
        return Vec::new();
    }
    let mut nodes = Vec::new();
    let mut current_ref = start_obj.as_reference().ok();
    let mut guard = 0usize;
    while let Some(oid) = current_ref {
        guard += 1;
        if guard > 500 {
            break;
        }
        let Ok(dict) = doc.get_dictionary(oid) else {
            break;
        };
        let title = dict
            .get(b"Title")
            .map(decode_pdf_string)
            .unwrap_or_else(|_| "Marcador".to_string());
        let page_index = resolve_outline_page(doc, dict, page_id_to_idx);
        let children = dict
            .get(b"First")
            .ok()
            .map(|f| walk_outline_siblings(doc, f, page_id_to_idx, depth + 1))
            .unwrap_or_default();
        nodes.push(BookmarkNode {
            title,
            page_index,
            children,
        });
        current_ref = dict.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }
    nodes
}

fn resolve_outline_page(
    doc: &Document,
    dict: &Dictionary,
    page_id_to_idx: &HashMap<ObjectId, usize>,
) -> Option<usize> {
    let dest = dict.get(b"Dest").ok().or_else(|| {
        doc.get_dict_in_dict(dict, b"A")
            .ok()
            .and_then(|a| a.get(b"D").ok())
    })?;
    let arr = match dest {
        Object::Array(a) => a,
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
        _ => return None,
    };
    let page_ref = arr.first()?.as_reference().ok()?;
    page_id_to_idx.get(&page_ref).copied()
}

fn read_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Real(r) => Some(*r as f64),
        Object::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn read_rect_pdf(dict: &Dictionary, page_h: f64) -> Option<Rect2> {
    let arr = dict.get(b"Rect").ok()?.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let x1 = read_number(&arr[0])?;
    let y1 = read_number(&arr[1])?;
    let x2 = read_number(&arr[2])?;
    let y2 = read_number(&arr[3])?;
    Some(Rect2::from_min_max(
        Point2::new(x1, page_h - y2),
        Point2::new(x2, page_h - y1),
    ))
}

fn read_color_rgb(dict: &Dictionary, key: &[u8]) -> Option<[u8; 4]> {
    let arr = dict.get(key).ok()?.as_array().ok()?;
    if arr.len() >= 3 {
        let r = (read_number(&arr[0])?.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (read_number(&arr[1])?.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (read_number(&arr[2])?.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some([r, g, b, 255])
    } else {
        None
    }
}

fn read_page_viewports(
    doc: &Document,
    page_id: ObjectId,
    page_model: &PageModel,
) -> Result<Vec<ViewportScale>, String> {
    let page = doc.get_dictionary(page_id).map_err(|e| e.to_string())?;
    let Ok(vp_obj) = page.get(b"VP") else {
        return Ok(Vec::new());
    };
    let arr = match vp_obj {
        Object::Array(a) => a.clone(),
        Object::Reference(id) => doc
            .get_object(*id)
            .map_err(|e| e.to_string())?
            .as_array()
            .map_err(|e| e.to_string())?
            .clone(),
        _ => return Ok(Vec::new()),
    };

    let mut viewports = Vec::new();
    for entry in arr {
        let dict = match entry {
            Object::Dictionary(d) => d,
            Object::Reference(id) => match doc.get_dictionary(id) {
                Ok(d) => d.clone(),
                Err(_) => continue,
            },
            _ => continue,
        };
        let Some(bbox) = dict.get(b"BBox").ok().and_then(|o| o.as_array().ok()) else {
            continue;
        };
        if bbox.len() < 4 {
            continue;
        }
        let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
            read_number(&bbox[0]),
            read_number(&bbox[1]),
            read_number(&bbox[2]),
            read_number(&bbox[3]),
        ) else {
            continue;
        };
        let a = page_model.pdf_to_page_pt(x1, y1);
        let b = page_model.pdf_to_page_pt(x2, y2);
        let rect = Rect2::from_min_max(
            Point2::new(a.x.min(b.x), a.y.min(b.y)),
            Point2::new(a.x.max(b.x), a.y.max(b.y)),
        );
        let name = dict
            .get(b"Name")
            .map(decode_pdf_string)
            .unwrap_or_else(|_| "Viewport".to_string());
        let mut scale = PageScale::default();
        if let Ok(m_dict) = doc.get_dict_in_dict(&dict, b"Measure") {
            let mut parsed = false;
            // Prefer exact ISO 32000-1 /X [ << /C ... /U ... >> ] number format
            if let Ok(x_obj) = m_dict.get(b"X") {
                let x_arr = match x_obj {
                    Object::Array(a) => Some(a.as_slice()),
                    Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_array().ok()).map(|v| v.as_slice()),
                    _ => None,
                };
                if let Some(first_nf) = x_arr.and_then(|a| a.first()) {
                    let nf_dict = match first_nf {
                        Object::Dictionary(d) => Some(d),
                        Object::Reference(id) => doc.get_dictionary(*id).ok(),
                        _ => None,
                    };
                    if let Some(nf) = nf_dict
                        && let Some(c_val) = nf.get(b"C").ok().and_then(read_number)
                        && c_val.is_finite()
                        && c_val > 0.0
                        && let Ok(unit_obj) = nf.get(b"U")
                        && let Some(unit) = parse_measure_unit(&decode_pdf_string(unit_obj))
                    {
                        // c_val is real display units per 1 PDF point
                        scale = PageScale::from_calibration(1.0, c_val, unit);
                        parsed = true;
                    }
                }
            }
            if !parsed {
                let ratio_str = m_dict.get(b"R").map(decode_pdf_string).unwrap_or_default();
                if let Some(rest) = ratio_str.strip_prefix("1:")
                    && let Ok(ratio) = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .parse::<f64>()
                    && ratio.is_finite()
                    && ratio > 0.0
                {
                    scale = PageScale::from_metric_ratio(ratio);
                }
            }
        }
        viewports.push(ViewportScale::new(name, rect, scale));
    }
    Ok(viewports)
}

fn covers_page(rect: Rect2, page: &PageModel) -> bool {
    const TOLERANCE_PT: f64 = 0.1;
    rect.min().x.abs() <= TOLERANCE_PT
        && rect.min().y.abs() <= TOLERANCE_PT
        && (rect.max().x - page.width_pt).abs() <= TOLERANCE_PT
        && (rect.max().y - page.height_pt).abs() <= TOLERANCE_PT
}

pub(crate) fn apply_text_page_scales(pages: &mut [PageModel], text_by_page: &[String]) {
    for (page, text) in pages.iter_mut().zip(text_by_page) {
        if !page.scale.is_set
            && !page
                .viewports
                .iter()
                .any(|vp| vp.scale.is_set && covers_page(vp.rect, page))
            && let Some(ratio) = detect_metric_scale(text)
        {
            page.scale = PageScale::from_metric_ratio(ratio);
        }
    }
}

fn detect_metric_scale(text: &str) -> Option<f64> {
    let text = text.to_lowercase();
    let mut detected = None;
    let mut has_sheet_scale = false;
    for (index, _) in text.match_indices("escala") {
        let before = &text[..index];
        let after = &text[index + "escala".len()..];
        if before.ends_with(char::is_alphanumeric) || after.starts_with(char::is_alphanumeric) {
            continue;
        }
        let label_value = after.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
        let previous_word = before.split_whitespace().next_back();
        let next_word = label_value.split(|c: char| !c.is_alphabetic()).next();
        if previous_word == Some("sin")
            || matches!(next_word, Some("variable" | "indicada" | "varias"))
            || label_value.starts_with("s/e")
        {
            return None;
        }
        let value = label_value.strip_prefix("principal").unwrap_or(label_value);
        let value = value.trim_start_matches(|c: char| c.is_whitespace() || c == ':');
        let Some((ratio, rest)) = metric_ratio_prefix(value) else {
            continue;
        };
        let rest = rest.trim_start();
        let alternative = rest
            .strip_prefix(['/', ';', ','])
            .or_else(|| rest.strip_prefix("y "))
            .or_else(|| rest.strip_prefix("o "));
        if detected.is_some_and(|previous| previous != ratio)
            || alternative
                .is_some_and(|tail| tail.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        {
            return None;
        }
        detected = Some(ratio);
        let line_prefix = before.rsplit(['\n', '\r']).next().unwrap_or_default();
        has_sheet_scale |= !line_prefix
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word == "detalle");
    }
    detected.filter(|_| has_sheet_scale)
}

fn metric_ratio_prefix(value: &str) -> Option<(f64, &str)> {
    let denominator = value
        .strip_prefix('1')?
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let whole_len = denominator.bytes().take_while(u8::is_ascii_digit).count();
    if whole_len == 0 {
        return None;
    }
    let mut end = whole_len;
    if let Some(fraction) = denominator[whole_len..].strip_prefix(['.', ',']) {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        // Three digits can indicate either a decimal or a thousands group.
        if digits == 3 {
            return None;
        }
        if digits > 0 {
            end += 1 + digits;
        }
    }
    let rest = &denominator[end..];
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == ':')
        || rest
            .trim_start_matches([' ', '\t', '\u{a0}', '\u{202f}'])
            .starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let ratio = denominator[..end].replace(',', ".").parse::<f64>().ok()?;
    (ratio.is_finite() && ratio >= 1e-6).then_some((ratio, rest))
}

fn parse_measure_unit(raw: &str) -> Option<LengthUnit> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mm" => Some(LengthUnit::Millimeter),
        "cm" => Some(LengthUnit::Centimeter),
        "m" => Some(LengthUnit::Meter),
        "km" => Some(LengthUnit::Kilometer),
        "in" | "\"" => Some(LengthUnit::Inch),
        "ft" | "'" => Some(LengthUnit::Foot),
        "ft-in" | "ft'-in\"" => Some(LengthUnit::FootInch),
        "yd" => Some(LengthUnit::Yard),
        _ => None,
    }
}

fn import_pdf_annotation(
    _doc: &Document,
    dict: &Dictionary,
    page_index: usize,
    page_h: f64,
) -> Option<Markup> {
    let subtype = dict.get(b"Subtype").ok()?.as_name().ok()?;
    let rect = read_rect_pdf(dict, page_h)?;
    let contents = dict.get(b"Contents").map(decode_pdf_string).unwrap_or_default();
    let subj = dict
        .get(b"Subj")
        .map(decode_pdf_string)
        .unwrap_or_else(|_| String::from_utf8_lossy(subtype).to_string());
    let author = dict
        .get(b"T")
        .map(decode_pdf_string)
        .unwrap_or_else(|_| "PDF".to_string());

    let mut style = MarkupStyle::default();
    if let Some(c) = read_color_rgb(dict, b"C") {
        style.stroke_color = c;
        style.text_color = c;
    }
    if let Some(ic) = read_color_rgb(dict, b"IC") {
        style.fill_color = Some(ic);
    }
    if let Some(ca) = dict.get(b"CA").ok().and_then(read_number) {
        style.opacity = ca.clamp(0.05, 1.0) as f32;
    }

    let kind = match subtype {
        b"Text" => MarkupKind::Note {
            pos: rect.center(),
        },
        b"FreeText" => {
            if let Ok(cl) = dict.get(b"CL").and_then(|o| o.as_array())
                && cl.len() >= 4
            {
                let tip = Point2::new(
                    read_number(&cl[0]).unwrap_or(rect.min_x),
                    page_h - read_number(&cl[1]).unwrap_or(page_h - rect.min_y),
                );
                let knee = if cl.len() >= 6 {
                    Point2::new(
                        read_number(&cl[2]).unwrap_or(tip.x + 20.0),
                        page_h - read_number(&cl[3]).unwrap_or(page_h - tip.y),
                    )
                } else {
                    tip.midpoint(rect.center())
                };
                MarkupKind::Callout {
                    arrow_tip: tip,
                    knee,
                    box_rect: rect,
                    text: contents.clone(),
                }
            } else {
                MarkupKind::TextBox {
                    rect,
                    text: contents.clone(),
                }
            }
        }
        b"Line" => {
            let l_arr = dict.get(b"L").ok().and_then(|o| o.as_array().ok());
            let (p1, p2) = if let Some(a) = l_arr
                && a.len() >= 4
            {
                (
                    Point2::new(
                        read_number(&a[0]).unwrap_or(rect.min_x),
                        page_h - read_number(&a[1]).unwrap_or(page_h - rect.min_y),
                    ),
                    Point2::new(
                        read_number(&a[2]).unwrap_or(rect.max_x),
                        page_h - read_number(&a[3]).unwrap_or(page_h - rect.max_y),
                    ),
                )
            } else {
                (rect.min(), rect.max())
            };
            let has_arrow = dict
                .get(b"LE")
                .ok()
                .and_then(|o| o.as_array().ok())
                .is_some_and(|arr| {
                    arr.iter()
                        .any(|x| x.as_name().is_ok_and(|n| n != b"None"))
                });
            if has_arrow {
                style.end_end = LineEnd::ClosedArrow;
                MarkupKind::Arrow { start: p1, end: p2 }
            } else {
                MarkupKind::Line { start: p1, end: p2 }
            }
        }
        b"PolyLine" | b"Polygon" => {
            let verts = dict.get(b"Vertices").ok().and_then(|o| o.as_array().ok())?;
            let mut pts = Vec::new();
            for pair in verts.as_chunks::<2>().0 {
                if let (Some(x), Some(y)) = (read_number(&pair[0]), read_number(&pair[1])) {
                    pts.push(Point2::new(x, page_h - y));
                }
            }
            if pts.len() < 2 {
                return None;
            }
            let is_cloud = dict
                .get(b"BE")
                .ok()
                .and_then(|o| o.as_dict().ok())
                .and_then(|be| be.get(b"S").ok())
                .and_then(|s| s.as_name().ok())
                == Some(b"C");
            if is_cloud {
                style.line_style = LineStyle::Cloud;
                MarkupKind::Cloud {
                    points: pts,
                    is_rect: false,
                }
            } else if subtype == b"Polygon" {
                MarkupKind::Polygon { points: pts }
            } else {
                MarkupKind::Polyline { points: pts }
            }
        }
        b"Square" => MarkupKind::Rectangle { rect },
        b"Circle" => MarkupKind::Ellipse { rect },
        b"Ink" => {
            let ink_list = dict.get(b"InkList").ok().and_then(|o| o.as_array().ok())?;
            let mut strokes = Vec::new();
            for s_obj in ink_list {
                if let Ok(coords) = s_obj.as_array() {
                    let mut pts = Vec::new();
                    for pair in coords.as_chunks::<2>().0 {
                        if let (Some(x), Some(y)) = (read_number(&pair[0]), read_number(&pair[1]))
                        {
                            pts.push(Point2::new(x, page_h - y));
                        }
                    }
                    if !pts.is_empty() {
                        strokes.push(pts);
                    }
                }
            }
            MarkupKind::Ink { strokes }
        }
        b"Highlight" => MarkupKind::Highlight {
            strokes: vec![vec![
                Point2::new(rect.min_x, rect.center().y),
                Point2::new(rect.max_x, rect.center().y),
            ]],
        },
        b"Stamp" => MarkupKind::Stamp {
            rect,
            title: if contents.is_empty() {
                subj.clone()
            } else {
                contents.clone()
            },
            subtitle: String::new(),
            show_date: false,
            show_author: true,
        },
        _ => return None,
    };

    let mut m = Markup::new(page_index, subj, author, style, kind);
    m.comment = contents;
    Some(m)
}

/// Returns a copy of the PDF bytes with page `/Annots` cleared so `hayro` renders only
/// base sheet content while RedBIM draws live vector annotations on top.
pub fn strip_annots_for_render(raw_bytes: &[u8]) -> Vec<u8> {
    let Ok(mut doc) = Document::load_mem(raw_bytes) else {
        return raw_bytes.to_vec();
    };
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for pid in page_ids {
        if let Ok(page) = doc.get_dictionary_mut(pid) {
            page.remove(b"Annots");
        }
    }
    let mut out = Vec::new();
    if doc.save_to(&mut out).is_ok() && !out.is_empty() {
        out
    } else {
        raw_bytes.to_vec()
    }
}

pub use crate::storage::atomic_write_file;

pub fn save_pdf_file(tab: &DocumentTab, path: &Path) -> Result<Vec<u8>, String> {
    let bytes = save_pdf_to_bytes(tab)?;
    atomic_write_file(path, &bytes)?;
    Ok(bytes)
}

pub fn save_pdf_to_bytes(tab: &DocumentTab) -> Result<Vec<u8>, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error al procesar PDF: {e}"))?;
    let pages = doc.get_pages();
    let ap_res = crate::pdf::flatten::PdfOverlayResources::create(&mut doc);
    let markups_by_page = tab.markups_by_page();

    for (&page_num, &page_id) in &pages {
        let idx = page_num.saturating_sub(1) as usize;
        let Some(page_model) = tab.pages.get(idx) else {
            continue;
        };
        let page_h = page_model.height_pt;

        // Preserve existing non-markup annotations (e.g. interactive form /Widget fields)
        let mut annot_refs = Vec::new();
        if let Ok(existing_dict) = doc.get_dictionary(page_id)
            && let Ok(ann_obj) = existing_dict.get(b"Annots")
        {
            let existing_arr = match ann_obj {
                Object::Array(a) => Some(a.clone()),
                Object::Reference(id) => doc
                    .get_object(*id)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .cloned(),
                _ => None,
            };
            if let Some(arr) = existing_arr {
                for item in arr {
                    if let Ok(rid) = item.as_reference()
                        && let Ok(d) = doc.get_dictionary(rid)
                        && let Ok(st) = d.get(b"Subtype").and_then(|o| o.as_name())
                        && matches!(st, b"Widget" | b"Popup" | b"Screen" | b"3D")
                    {
                        annot_refs.push(Object::Reference(rid));
                    }
                }
            }
        }

        // Write ISO 32000-1 /Annots + Form XObject /AP appearance streams for all markups on this page
        for &m in &markups_by_page[idx] {
            let mut annot_dict = markup_to_pdf_annot(m, page_model, page_h, &pages);
            let ap_id = crate::pdf::flatten::build_markup_appearance_stream(
                &mut doc, &ap_res, m, page_model, page_h,
            );
            annot_dict.set("AP", dictionary! { "N" => ap_id });
            let aid = doc.add_object(annot_dict);
            annot_refs.push(Object::Reference(aid));
        }

        // Write PDF /VP Viewports + Page-level scale (always emit page scale first if set)
        let mut vp_array = Vec::new();
        if page_model.scale.is_set {
            let measure = build_measure_dict(&page_model.scale);
            let m_id = doc.add_object(measure);
            let [bx0, by0, bx1, by1] = page_model
                .page_rect_to_pdf_rect(Rect2::from_xywh(0.0, 0.0, page_model.width_pt, page_model.height_pt));
            let vp_dict = dictionary! {
                "Type" => "Viewport",
                "Name" => pdf_utf16_string(&page_model.display_label()),
                "BBox" => vec![bx0.into(), by0.into(), bx1.into(), by1.into()],
                "Measure" => m_id,
            };
            vp_array.push(Object::Dictionary(vp_dict));
        }
        for vp in &page_model.viewports {
            let measure = build_measure_dict(&vp.scale);
            let m_id = doc.add_object(measure);
            let [vx0, vy0, vx1, vy1] = page_model.page_rect_to_pdf_rect(vp.rect);
            let vp_dict = dictionary! {
                "Type" => "Viewport",
                "Name" => pdf_utf16_string(&vp.name),
                "BBox" => vec![vx0.into(), vy0.into(), vx1.into(), vy1.into()],
                "Measure" => m_id,
            };
            vp_array.push(Object::Dictionary(vp_dict));
        }

        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            if annot_refs.is_empty() {
                page_dict.remove(b"Annots");
            } else {
                page_dict.set("Annots", annot_refs);
            }
            if vp_array.is_empty() {
                page_dict.remove(b"VP");
            } else {
                page_dict.set("VP", vp_array);
            }
        }
    }

    // Store compressed lossless /RedBIMState stream in Catalog
    let state = tab.export_embedded_state();
    let json = serde_json::to_vec(&state).map_err(|e| format!("Error serializando estado: {e}"))?;
    let mut stream = Stream::new(dictionary! {}, json);
    let _ = stream.compress();
    let state_id = doc.add_object(stream);
    if let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(|o| o.as_reference())
        && let Ok(cat) = doc.get_dictionary_mut(catalog_id)
    {
        cat.set("RedBIMState", state_id);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error al escribir PDF: {e}"))?;
    Ok(out)
}

fn build_measure_dict(scale: &PageScale) -> Dictionary {
    let unit_str = scale.display_unit.suffix();
    let area_str = scale.area_unit.suffix();
    let c_factor_x = scale.pdf_measure_c_factor();
    let c_factor_y = scale.display_unit.from_meters(scale.pt_to_meters_y());
    // ISO 32000-1 Table 261: /A conversion factor converts squared /X units to /A units
    let x_unit_sq_m = scale.display_unit.meters_per_unit().powi(2);
    let area_c = x_unit_sq_m / scale.area_unit.sq_meters_per_unit().max(1e-18);

    let x_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => pdf_utf16_string(unit_str),
        "C" => c_factor_x,
        "D" => 100i64,
    })];
    let d_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => pdf_utf16_string(unit_str),
        "C" => 1.0,
        "D" => 100i64,
    })];
    let a_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => pdf_utf16_string(area_str),
        "C" => area_c,
        "D" => 100i64,
    })];
    let ratio = if scale.paper_unit.is_metric() && scale.real_unit.is_metric() {
        let r = scale.ratio_x();
        if (r - r.round()).abs() < 0.01 {
            format!("1:{:.0}", r.round())
        } else {
            format!("1:{r:.2}")
        }
    } else {
        scale.label()
    };
    let mut dict = dictionary! {
        "Type" => "Measure",
        "Subtype" => "RL",
        "R" => pdf_utf16_string(&ratio),
        "X" => x_arr,
        "D" => d_arr,
        "A" => a_arr,
    };
    if scale.separate_xy {
        dict.set(
            "Y",
            vec![Object::Dictionary(dictionary! {
                "Type" => "NumberFormat",
                "U" => pdf_utf16_string(unit_str),
                "C" => c_factor_y,
                "D" => 100i64,
            })],
        );
    }
    dict
}

fn color_arr(c: [u8; 4]) -> Vec<Object> {
    vec![
        (c[0] as f64 / 255.0).into(),
        (c[1] as f64 / 255.0).into(),
        (c[2] as f64 / 255.0).into(),
    ]
}

fn markup_to_pdf_annot(
    m: &Markup,
    page: &PageModel,
    page_h: f64,
    pages_map: &std::collections::BTreeMap<u32, ObjectId>,
) -> Dictionary {
    let b = crate::pdf::flatten::markup_visual_bounds(m);
    let [rx0, ry0, rx1, ry1] = page.page_rect_to_pdf_rect(b);
    let rect = vec![rx0.into(), ry0.into(), rx1.into(), ry1.into()];
    let metrics = m.compute_metrics(&page.scale, &page.viewports);
    let content_text = if m.comment.is_empty() {
        metrics.primary_display.clone().unwrap_or_default()
    } else {
        m.comment.clone()
    };

    let mut d = dictionary! {
        "Type" => "Annot",
        "Rect" => rect,
        "NM" => pdf_utf16_string(&m.id.to_string()),
        "Subj" => pdf_utf16_string(&m.subject),
        "T" => pdf_utf16_string(&m.author),
        "Contents" => pdf_utf16_string(&content_text),
        "CA" => (m.style.opacity as f64),
        "C" => color_arr(m.style.stroke_color),
        "F" => 4i64,
    };
    if let Some(fill) = m.style.fill_color {
        d.set("IC", color_arr(fill));
    }

    match &m.kind {
        MarkupKind::Note { .. } => {
            d.set("Subtype", "Text");
            let icon = match m.style.note_icon {
                NoteIcon::Comment => "Comment",
                NoteIcon::Help => "Help",
                NoteIcon::Key => "Key",
                NoteIcon::Insert => "Insert",
                NoteIcon::NewParagraph => "NewParagraph",
                NoteIcon::Paragraph => "Paragraph",
                _ => "Note",
            };
            d.set("Name", Object::Name(icon.as_bytes().to_vec()));
        }
        MarkupKind::TextBox { text, .. } | MarkupKind::Typewriter { text, .. } => {
            d.set("Subtype", "FreeText");
            d.set("Contents", pdf_utf16_string(text));
            d.set(
                "DA",
                Object::string_literal(format!(
                    "/Helv {} Tf {} {} {} rg",
                    m.style.font_size,
                    m.style.text_color[0] as f64 / 255.0,
                    m.style.text_color[1] as f64 / 255.0,
                    m.style.text_color[2] as f64 / 255.0
                )),
            );
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            text,
        } => {
            d.set("Subtype", "FreeText");
            d.set("IT", "FreeTextCallout");
            d.set("Contents", pdf_utf16_string(text));
            let (knee, anchor) = crate::geometry::hit_test::callout_landing(*box_rect, *knee);
            d.set(
                "CL",
                vec![
                    arrow_tip.x.into(),
                    (page_h - arrow_tip.y).into(),
                    knee.x.into(),
                    (page_h - knee.y).into(),
                    anchor.x.into(),
                    (page_h - anchor.y).into(),
                ],
            );
            d.set("LE", Object::Name(m.style.end_end.pdf_name().as_bytes().to_vec()));
        }
        MarkupKind::Cloud { points, .. } | MarkupKind::CloudPlus { cloud_points: points, .. } => {
            d.set("Subtype", "Polygon");
            d.set("BE", dictionary! { "S" => "C", "I" => 1.0 });
            let mut verts = Vec::with_capacity(points.len() * 2);
            for p in points {
                verts.push(p.x.into());
                verts.push((page_h - p.y).into());
            }
            d.set("Vertices", verts);
        }
        MarkupKind::Line { start, end }
        | MarkupKind::Arrow { start, end }
        | MarkupKind::MeasureLength { start, end, .. }
        | MarkupKind::MeasureDiameter {
            p1: start, p2: end, ..
        } => {
            d.set("Subtype", "Line");
            d.set(
                "L",
                vec![
                    start.x.into(),
                    (page_h - start.y).into(),
                    end.x.into(),
                    (page_h - end.y).into(),
                ],
            );
            let start_le = if matches!(m.kind, MarkupKind::MeasureLength { .. }) {
                LineEnd::Slash
            } else {
                m.style.start_end
            };
            let end_le = if matches!(m.kind, MarkupKind::Arrow { .. }) {
                LineEnd::ClosedArrow
            } else if matches!(m.kind, MarkupKind::MeasureLength { .. }) {
                LineEnd::Slash
            } else {
                m.style.end_end
            };
            d.set(
                "LE",
                vec![
                    Object::Name(start_le.pdf_name().as_bytes().to_vec()),
                    Object::Name(end_le.pdf_name().as_bytes().to_vec()),
                ],
            );
        }
        MarkupKind::Polyline { points }
        | MarkupKind::MeasurePolylength { points, .. }
        | MarkupKind::MeasurePerimeter { points, .. } => {
            d.set("Subtype", "PolyLine");
            let mut verts = Vec::with_capacity(points.len() * 2);
            for p in points {
                verts.push(p.x.into());
                verts.push((page_h - p.y).into());
            }
            d.set("Vertices", verts);
        }
        MarkupKind::Polygon { points }
        | MarkupKind::MeasureArea { points, .. }
        | MarkupKind::MeasureVolume { points, .. }
        | MarkupKind::SketchPoly { points, .. } => {
            d.set("Subtype", "Polygon");
            let mut verts = Vec::with_capacity(points.len() * 2);
            for p in points {
                verts.push(p.x.into());
                verts.push((page_h - p.y).into());
            }
            d.set("Vertices", verts);
        }
        MarkupKind::Rectangle { .. } | MarkupKind::SketchRect { .. } => {
            d.set("Subtype", "Square");
        }
        MarkupKind::Ellipse { .. } => {
            d.set("Subtype", "Circle");
        }
        MarkupKind::Ink { strokes } => {
            d.set("Subtype", "Ink");
            let list: Vec<Object> = strokes
                .iter()
                .map(|s| {
                    let mut arr = Vec::with_capacity(s.len() * 2);
                    for p in s {
                        arr.push(p.x.into());
                        arr.push((page_h - p.y).into());
                    }
                    Object::Array(arr)
                })
                .collect();
            d.set("InkList", list);
        }
        MarkupKind::Highlight { .. } => {
            d.set("Subtype", "Highlight");
        }
        MarkupKind::Stamp { title, .. } => {
            d.set("Subtype", "Stamp");
            d.set("Name", Object::Name(b"Draft".to_vec()));
            d.set("Contents", pdf_utf16_string(title));
        }
        MarkupKind::Hyperlink {
            target,
            description,
            ..
        } => {
            d.set("Subtype", "Link");
            d.set("Contents", pdf_utf16_string(description));
            match target {
                HyperlinkTarget::Page { page_index } => {
                    if let Some(&target_id) = pages_map.get(&(*page_index as u32 + 1)) {
                        d.set(
                            "Dest",
                            vec![Object::Reference(target_id), Object::Name(b"Fit".to_vec())],
                        );
                    }
                }
                HyperlinkTarget::Url(url) => {
                    d.set(
                        "A",
                        dictionary! {
                            "Type" => "Action",
                            "S" => "URI",
                            "URI" => Object::string_literal(url.as_bytes()),
                        },
                    );
                }
            }
        }
        MarkupKind::Redaction { overlay_text, .. } => {
            d.set("Subtype", "Redact");
            d.set("IC", color_arr([0, 0, 0, 255]));
            d.set("OverlayText", pdf_utf16_string(overlay_text));
        }
        _ => {
            d.set("Subtype", "Square");
        }
    }
    d
}

/// Rotates a page in `tab.raw_pdf_bytes` by +90 or -90 degrees and updates `PageModel`.
pub(crate) struct RotatedPdfPage {
    bytes: Arc<Vec<u8>>,
    size: (f64, f64),
    rotation: u16,
}

pub(crate) fn rotate_pdf_page(
    raw: &[u8],
    page: &PageModel,
    page_index: usize,
    delta_deg: i32,
) -> Result<RotatedPdfPage, String> {
    let mut doc = Document::load_mem(raw).map_err(|e| e.to_string())?;
    let page_id = doc
        .page_iter()
        .nth(page_index)
        .ok_or("Página no encontrada")?;
    let current_rotation = materialize_page_dict(&doc, page_id)
        .and_then(|dict| dict.get(b"Rotate").ok()?.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360) as u16;
    let rotation = (i64::from(current_rotation) + i64::from(delta_deg)).rem_euclid(360) as u16;
    doc.get_dictionary_mut(page_id)
        .map_err(|e| e.to_string())?
        .set("Rotate", i64::from(rotation));

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    let bytes = Arc::new(out);
    // A quarter turn swaps the already measured render dimensions, including crop and UserUnit.
    let size = if current_rotation == page.pdf_native_rotation
        && current_rotation.is_multiple_of(90)
        && rotation.is_multiple_of(90)
        && page.width_pt > 10.0
        && page.height_pt > 10.0
    {
        if (current_rotation % 180) == (rotation % 180) {
            (page.width_pt, page.height_pt)
        } else {
            (page.height_pt, page.width_pt)
        }
    } else {
        let pdf = Pdf::new(Arc::clone(&bytes)).map_err(|e| format!("{e:?}"))?;
        let (w, h) = pdf
            .pages()
            .get(page_index)
            .ok_or("Página no encontrada")?
            .render_dimensions();
        (f64::from(w), f64::from(h))
    };
    Ok(RotatedPdfPage {
        bytes,
        size,
        rotation,
    })
}

pub(crate) fn apply_rotated_page(
    tab: &mut DocumentTab,
    page_index: usize,
    rotated: RotatedPdfPage,
) {
    let page = &mut tab.pages[page_index];
    (page.width_pt, page.height_pt) = rotated.size;
    page.rotation_deg = rotated.rotation;
    page.pdf_native_rotation = rotated.rotation;
    tab.raw_pdf_bytes = rotated.bytes;
    tab.camera.fit_page = true;
    tab.secondary_camera.fit_page = true;
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
}

pub fn rotate_page_in_doc(
    tab: &mut DocumentTab,
    page_index: usize,
    delta_deg: i32,
) -> Result<(), String> {
    let page = tab.pages.get(page_index).ok_or("Página no encontrada")?;
    let rotated = rotate_pdf_page(&tab.raw_pdf_bytes, page, page_index, delta_deg)?;
    apply_rotated_page(tab, page_index, rotated);
    Ok(())
}

/// Inserts a blank page after `after_index` (or at end) matching `width_pt × height_pt`.
pub fn insert_blank_page_in_doc(
    tab: &mut DocumentTab,
    insert_at: usize,
    width_pt: f64,
    height_pt: f64,
) -> Result<(), String> {
    let mut doc = Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| e.to_string())?;
    let catalog_id = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let pages_id = doc.get_dictionary(catalog_id).and_then(|d| d.get(b"Pages")).and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;

    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let new_page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), width_pt.into(), height_pt.into()],
        "Resources" => dictionary! {},
        "Contents" => content_id,
    });

    // Materialize inherited attributes before flattening nested /Pages trees
    let mut existing_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for &pid in &existing_ids {
        if let Some(mat) = materialize_page_dict(&doc, pid) {
            doc.objects.insert(pid, Object::Dictionary(mat));
        }
    }
    let idx = insert_at.min(existing_ids.len());
    existing_ids.insert(idx, new_page_id);
    let kids: Vec<Object> = existing_ids.iter().map(|&id| Object::Reference(id)).collect();
    for &pid in &existing_ids {
        if let Ok(d) = doc.get_dictionary_mut(pid) {
            d.set("Parent", pages_id);
        }
    }
    if let Ok(pages_dict) = doc.get_dictionary_mut(pages_id) {
        pages_dict.set("Kids", kids);
        pages_dict.set("Count", existing_ids.len() as i64);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    tab.raw_pdf_bytes = out.into();

    let mut new_pm = PageModel::new(idx, width_pt, height_pt);
    if let Some(prev) = tab.pages.get(idx.saturating_sub(1)) {
        new_pm.scale.display_unit = prev.scale.display_unit;
        new_pm.scale.area_unit = prev.scale.area_unit;
        new_pm.scale.volume_unit = prev.scale.volume_unit;
        new_pm.scale.precision = prev.scale.precision;
    }
    tab.pages.insert(idx, new_pm);
    for (i, p) in tab.pages.iter_mut().enumerate() {
        p.page_index = i;
    }
    for m in &mut tab.markups {
        if m.page_index >= idx {
            m.page_index += 1;
        }
        if let MarkupKind::Hyperlink {
            target: HyperlinkTarget::Page { page_index },
            ..
        } = &mut m.kind
            && *page_index >= idx
        {
            *page_index += 1;
        }
    }
    for sp in &mut tab.spaces {
        if sp.page_index >= idx {
            sp.page_index += 1;
        }
    }
    remap_bookmarks_on_insert(&mut tab.bookmarks, idx);
    if tab.search_text_by_page.len() < idx {
        tab.search_text_by_page.resize(idx, String::new());
    }
    tab.search_text_by_page.insert(idx, String::new());
    if tab.camera.page_index >= idx {
        tab.camera.page_index += 1;
    }
    if tab.secondary_camera.page_index >= idx {
        tab.secondary_camera.page_index += 1;
    }
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.camera.fit_page = true;
    tab.secondary_camera.fit_page = true;
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
    Ok(())
}

/// Deletes page `page_index` (must keep at least 1 page).
pub(crate) fn delete_pdf_page(
    raw: &[u8],
    page_count: usize,
    page_index: usize,
) -> Result<Arc<Vec<u8>>, String> {
    if page_count <= 1 {
        return Err("No se puede eliminar la única página del documento.".to_string());
    }
    if page_index >= page_count {
        return Err("Página inválida".to_string());
    }
    let mut doc = Document::load_mem(raw).map_err(|e| e.to_string())?;
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let pages_id = doc
        .get_dictionary(catalog_id)
        .and_then(|d| d.get(b"Pages"))
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;

    let mut existing_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if page_index >= existing_ids.len() {
        return Err("Página inválida".to_string());
    }
    existing_ids.remove(page_index);
    for &pid in &existing_ids {
        if let Some(mat) = materialize_page_dict(&doc, pid) {
            doc.objects.insert(pid, Object::Dictionary(mat));
        }
    }
    let kids: Vec<Object> = existing_ids
        .iter()
        .map(|&id| Object::Reference(id))
        .collect();
    for &pid in &existing_ids {
        if let Ok(d) = doc.get_dictionary_mut(pid) {
            d.set("Parent", pages_id);
        }
    }
    if let Ok(pages_dict) = doc.get_dictionary_mut(pages_id) {
        pages_dict.set("Kids", kids);
        pages_dict.set("Count", existing_ids.len() as i64);
    }

    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(Arc::new(out))
}

pub(crate) fn apply_deleted_page(tab: &mut DocumentTab, page_index: usize, bytes: Arc<Vec<u8>>) {
    tab.raw_pdf_bytes = bytes;
    tab.pages.remove(page_index);
    for (i, p) in tab.pages.iter_mut().enumerate() {
        p.page_index = i;
    }
    tab.markups.retain(|m| m.page_index != page_index);
    for m in &mut tab.markups {
        if m.page_index > page_index {
            m.page_index -= 1;
        }
        if let MarkupKind::Hyperlink {
            target: HyperlinkTarget::Page { page_index: dest },
            ..
        } = &mut m.kind
            && *dest > page_index
        {
            *dest -= 1;
        } else if let MarkupKind::Hyperlink {
            target: HyperlinkTarget::Page { page_index: dest },
            ..
        } = &mut m.kind
            && *dest == page_index
        {
            *dest = page_index.min(tab.pages.len().saturating_sub(1));
        }
    }
    tab.spaces.retain(|s| s.page_index != page_index);
    for sp in &mut tab.spaces {
        if sp.page_index > page_index {
            sp.page_index -= 1;
        }
    }
    remap_bookmarks_on_delete(&mut tab.bookmarks, page_index);
    if page_index < tab.search_text_by_page.len() {
        tab.search_text_by_page.remove(page_index);
    }
    let max_idx = tab.pages.len().saturating_sub(1);
    tab.camera.page_index = tab.camera.page_index.min(max_idx);
    tab.secondary_camera.page_index = tab.secondary_camera.page_index.min(max_idx);
    if !tab.selected_ids.is_empty() {
        let remaining: HashSet<_> = tab.markups.iter().map(|m| m.id).collect();
        tab.selected_ids.retain(|id| remaining.contains(id));
    }
    tab.camera.fit_page = true;
    tab.secondary_camera.fit_page = true;
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
}

pub fn delete_page_in_doc(tab: &mut DocumentTab, page_index: usize) -> Result<(), String> {
    let bytes = delete_pdf_page(&tab.raw_pdf_bytes, tab.pages.len(), page_index)?;
    apply_deleted_page(tab, page_index, bytes);
    Ok(())
}

fn remap_bookmarks_on_insert(nodes: &mut [BookmarkNode], idx: usize) {
    for n in nodes {
        if let Some(p) = n.page_index
            && p >= idx
        {
            n.page_index = Some(p + 1);
        }
        remap_bookmarks_on_insert(&mut n.children, idx);
    }
}

fn remap_bookmarks_on_delete(nodes: &mut [BookmarkNode], page_index: usize) {
    for n in nodes {
        if let Some(p) = n.page_index {
            if p == page_index {
                n.page_index = None;
            } else if p > page_index {
                n.page_index = Some(p - 1);
            }
        }
        remap_bookmarks_on_delete(&mut n.children, page_index);
    }
}

/// Materializes inherited page tree attributes (`MediaBox`, `CropBox`, `Rotate`, `Resources`)
/// onto a standalone page dictionary before renumbering/merging.
fn materialize_page_dict(doc: &Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut page_dict = doc.get_dictionary(page_id).ok()?.clone();
    let mut cur_parent = page_dict
        .get(b"Parent")
        .ok()
        .and_then(|o| o.as_reference().ok());
    let mut guard = 0;
    while let Some(pid) = cur_parent {
        guard += 1;
        if guard > 32 {
            break;
        }
        let Ok(pdict) = doc.get_dictionary(pid) else {
            break;
        };
        for key in [&b"MediaBox"[..], &b"CropBox"[..], &b"Rotate"[..], &b"Resources"[..]] {
            if !page_dict.has(key)
                && let Ok(val) = pdict.get(key)
            {
                page_dict.set(key, val.clone());
            }
        }
        cur_parent = pdict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    }
    Some(page_dict)
}

/// Combines multiple PDF documents (supplied as raw byte slices) into one unified PDF.
pub fn combine_pdf_bytes(docs_bytes: &[&[u8]]) -> Result<Vec<u8>, String> {
    combine_pdf_documents(docs_bytes.iter().enumerate().map(|(i, raw)| {
        Document::load_mem(raw).map_err(|e| format!("Error leyendo documento #{}: {e}", i + 1))
    }))
}

fn combine_pdf_documents(
    sources: impl IntoIterator<Item = Result<Document, String>>,
) -> Result<Vec<u8>, String> {
    let mut sources = sources.into_iter().peekable();
    if sources.peek().is_none() {
        return Err("No hay documentos para combinar.".to_string());
    }
    let mut merged = Document::with_version("1.7");
    let pages_id = merged.new_object_id();
    let mut max_id = pages_id.0 + 1;
    let mut all_page_ids = Vec::new();

    for source in sources {
        let mut src = source?;
        let page_ids: Vec<ObjectId> = src.get_pages().values().copied().collect();
        for &pid in &page_ids {
            if let Some(mat) = materialize_page_dict(&src, pid) {
                src.objects.insert(pid, Object::Dictionary(mat));
            }
        }
        src.renumber_objects_with(max_id);
        max_id = src.max_id + 1;

        let renumbered_pages: Vec<ObjectId> = src.get_pages().values().copied().collect();
        for (oid, obj) in src.objects {
            merged.objects.insert(oid, obj);
        }
        for pid in renumbered_pages {
            if let Ok(d) = merged.get_dictionary_mut(pid) {
                d.set("Parent", pages_id);
            }
            all_page_ids.push(Object::Reference(pid));
        }
    }

    let count = all_page_ids.len() as i64;
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => count,
            "Kids" => all_page_ids,
        }),
    );
    merged.max_id = max_id;
    let catalog_id = merged.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    merged.trailer.set("Root", catalog_id);

    let mut out = Vec::new();
    merged
        .save_to(&mut out)
        .map_err(|e| format!("Error guardando PDF combinado: {e}"))?;
    Ok(out)
}

pub fn combine_pdf_files(paths: &[std::path::PathBuf]) -> Result<Vec<u8>, String> {
    combine_pdf_documents(paths.iter().map(|path| {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("Error leyendo {}: {e}", path.display()))?;
        Document::load_mem(&bytes).map_err(|e| format!("Error leyendo {}: {e}", path.display()))
    }))
}

/// Extracts the specified 0-based `page_indices` from `tab` into a standalone PDF (retaining markups & scales).
pub fn extract_pages_to_pdf(tab: &DocumentTab, page_indices: &[usize]) -> Result<Vec<u8>, String> {
    if page_indices.is_empty() {
        return Err("No se seleccionaron páginas para extraer.".to_string());
    }
    let mut doc = Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| e.to_string())?;
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let pages_id = doc
        .get_dictionary(catalog_id)
        .and_then(|d| d.get(b"Pages"))
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;

    let existing: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut kept_ids = Vec::new();
    let mut index_remap: HashMap<usize, usize> = HashMap::new();
    for &idx in page_indices {
        if let Some(&pid) = existing.get(idx)
            && !index_remap.contains_key(&idx)
        {
            if let Some(mat) = materialize_page_dict(&doc, pid) {
                doc.objects.insert(pid, Object::Dictionary(mat));
            }
            index_remap.insert(idx, kept_ids.len());
            kept_ids.push(pid);
        }
    }
    if kept_ids.is_empty() {
        return Err("Las páginas indicadas no existen.".to_string());
    }

    let kids: Vec<Object> = kept_ids.iter().map(|&id| Object::Reference(id)).collect();
    for &pid in &kept_ids {
        if let Ok(d) = doc.get_dictionary_mut(pid) {
            d.set("Parent", pages_id);
        }
    }
    if let Ok(pages_dict) = doc.get_dictionary_mut(pages_id) {
        pages_dict.set("Kids", kids);
        pages_dict.set("Count", kept_ids.len() as i64);
    }

    let mut extracted_pages = Vec::new();
    for &old_idx in page_indices {
        if let Some(pm) = tab.pages.get(old_idx)
            && let Some(&new_idx) = index_remap.get(&old_idx)
            && new_idx == extracted_pages.len()
        {
            let mut npm = pm.clone();
            npm.page_index = new_idx;
            extracted_pages.push(npm);
        }
    }
    let mut extracted_markups = Vec::new();
    for m in &tab.markups {
        if let Some(&new_idx) = index_remap.get(&m.page_index) {
            let mut nm = m.clone();
            nm.page_index = new_idx;
            extracted_markups.push(nm);
        }
    }
    let mut extracted_spaces = Vec::new();
    for sp in &tab.spaces {
        if let Some(&new_idx) = index_remap.get(&sp.page_index) {
            let mut nsp = sp.clone();
            nsp.page_index = new_idx;
            extracted_spaces.push(nsp);
        }
    }
    let state = RedBimEmbeddedState {
        version: 2,
        pages: extracted_pages,
        markups: extracted_markups,
        layers: tab.layers.clone(),
        bookmarks: Vec::new(),
        spaces: extracted_spaces,
        custom_columns: tab.custom_columns.clone(),
    };
    if let Ok(json) = serde_json::to_vec(&state) {
        let mut s = Stream::new(dictionary! {}, json);
        let _ = s.compress();
        let sid = doc.add_object(s);
        if let Ok(cat) = doc.get_dictionary_mut(catalog_id) {
            cat.set("RedBIMState", sid);
            cat.remove(b"Outlines");
        }
    }

    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Replaces the underlying PDF graphics of `target_page` with `source_page` from `source_pdf_bytes`
/// while preserving all existing markups, measurements, scales, viewports, spaces, and page labels
/// (mirrors Bluebeam Revu's `SCmdReplacePages` with `pContentOnly = true`).
pub fn replace_page_content_in_doc(
    tab: &mut DocumentTab,
    target_page: usize,
    source_pdf_bytes: &[u8],
    source_page: usize,
) -> Result<(), String> {
    if target_page >= tab.pages.len() {
        return Err("Página destino inválida.".to_string());
    }
    let mut dst = Document::load_mem(&tab.raw_pdf_bytes)
        .map_err(|e| format!("Error leyendo documento actual: {e}"))?;
    let mut src = Document::load_mem(source_pdf_bytes)
        .map_err(|e| format!("Error leyendo PDF de reemplazo: {e}"))?;

    let src_pages: Vec<ObjectId> = src.get_pages().values().copied().collect();
    let Some(&src_page_id) = src_pages.get(source_page) else {
        return Err("Página origen inválida en el PDF seleccionado.".to_string());
    };
    if let Some(mat) = materialize_page_dict(&src, src_page_id) {
        src.objects.insert(src_page_id, Object::Dictionary(mat));
    }

    // Measure new page visual dimensions via hayro so PageModel stays exact
    let src_hayro = hayro::hayro_syntax::Pdf::new(source_pdf_bytes.to_vec())
        .map_err(|e| format!("PDF de reemplazo no legible: {e:?}"))?;
    let (new_w, new_h) = src_hayro
        .pages()
        .get(source_page)
        .map(|p| {
            let (w, h) = p.render_dimensions();
            (w as f64, h as f64)
        })
        .unwrap_or((tab.pages[target_page].width_pt, tab.pages[target_page].height_pt));
    let new_box_rot = read_page_box_and_rotation(&src, src_page_id);

    // Renumber source objects above dst.max_id and import them into dst
    let base_id = dst.max_id + 1;
    src.renumber_objects_with(base_id);
    let new_src_page_id = src
        .get_pages()
        .values()
        .nth(source_page)
        .copied()
        .ok_or_else(|| "Error renumerando página origen.".to_string())?;
    let new_max_id = src.max_id;
    for (oid, obj) in src.objects {
        dst.objects.insert(oid, obj);
    }
    dst.max_id = dst.max_id.max(new_max_id);

    // Strip any external markups/annots from the imported source page so only clean graphics replace content
    if let Ok(sd) = dst.get_dictionary_mut(new_src_page_id) {
        sd.remove(b"Annots");
    }

    // Swap target_page in the destination /Pages /Kids array
    let catalog_id = dst
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let pages_id = dst
        .get_dictionary(catalog_id)
        .and_then(|d| d.get(b"Pages"))
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let mut dst_page_ids: Vec<ObjectId> = dst.get_pages().values().copied().collect();
    if target_page >= dst_page_ids.len() {
        return Err("Índice de página destino fuera de rango.".to_string());
    }
    dst_page_ids[target_page] = new_src_page_id;
    for &pid in &dst_page_ids {
        if let Some(mat) = materialize_page_dict(&dst, pid) {
            dst.objects.insert(pid, Object::Dictionary(mat));
        }
    }
    for &pid in &dst_page_ids {
        if let Ok(d) = dst.get_dictionary_mut(pid) {
            d.set("Parent", pages_id);
        }
    }
    let kids: Vec<Object> = dst_page_ids.iter().map(|&id| Object::Reference(id)).collect();
    if let Ok(pages_dict) = dst.get_dictionary_mut(pages_id) {
        pages_dict.set("Kids", kids);
        pages_dict.set("Count", dst_page_ids.len() as i64);
    }

    let mut out = Vec::new();
    dst.save_to(&mut out)
        .map_err(|e| format!("Error guardando página reemplazada: {e}"))?;
    tab.raw_pdf_bytes = out.into();

    if let Some(pm) = tab.pages.get_mut(target_page) {
        pm.width_pt = new_w;
        pm.height_pt = new_h;
        if let Some((box_arr, rot)) = new_box_rot {
            pm.pdf_crop_box = Some(box_arr);
            pm.pdf_native_rotation = rot;
            pm.rotation_deg = rot;
        }
    }
    tab.camera.fit_page = true;
    tab.secondary_camera.fit_page = true;
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
    Ok(())
}

/// Utility helper returning unit suffix for LengthUnit when building Scale presets.
#[allow(dead_code)]
fn _unit_is_metric(u: LengthUnit) -> bool {
    u.is_metric()
}

#[cfg(test)]
mod scale_detection_tests {
    use super::*;

    #[test]
    fn detects_only_explicit_unambiguous_metric_scales() {
        assert_eq!(
            detect_metric_scale("ESCALA:\n1 : 125,5 | UNIDADES: m"),
            Some(125.5)
        );
        assert_eq!(
            detect_metric_scale("Escala 1:100\nEscala 1:100"),
            Some(100.0)
        );
        for text in [
            "Fecha 01:50 | Plano 1:100",
            "Detalle A (Escala 1:20)",
            "Escala 1:100\nDetalle (Escala 1:20)",
            "Escala 1:50 / 1:100",
            "Sin escala\nDetalle (Escala 1:20)",
            "Escala 1:0",
            "Escala 1:1.000",
            "Escala 1:1 000",
        ] {
            assert_eq!(detect_metric_scale(text), None, "{text}");
        }
    }

    #[test]
    fn uses_pdf_text_per_sheet_and_keeps_existing_calibration() {
        let bytes = generate_demo_chile_pdf();
        let doc = Document::load_mem(&bytes).unwrap();
        let text_by_page: Vec<_> = (1..=3).map(|num| doc.extract_text(&[num]).unwrap()).collect();
        let mut tab = open_pdf_from_bytes("demo.pdf", None, bytes).unwrap();
        tab.pages[2].scale = PageScale::from_metric_ratio(75.0);

        apply_text_page_scales(&mut tab.pages, &text_by_page);

        assert_eq!(tab.pages[0].scale.ratio_x(), 100.0);
        assert!(!tab.pages[1].scale.is_set);
        assert_eq!(tab.pages[2].scale.ratio_x(), 75.0);
    }
}
