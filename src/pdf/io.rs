use crate::geometry::hit_test::markup_bounds;
use crate::geometry::math::{Point2, Rect2};
use crate::model::document::{BookmarkNode, DocumentTab, PageModel, RedBimEmbeddedState};
use crate::model::markup::{
    LineEnd, LineStyle, Markup, MarkupKind, MarkupStyle, NoteIcon,
};
use crate::model::scale::{PageScale, ViewportScale};
use crate::model::units::LengthUnit;
use crate::pdf::demo_blueprint::{create_demo_markups, generate_demo_chile_pdf, to_winansi_bytes};
use hayro::hayro_syntax::Pdf;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use std::collections::HashMap;
use std::path::Path;

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

/// Decodes PDF string literal (UTF-16BE with BOM or PDFDocEncoding/WinAnsi) to Rust String.
fn decode_pdf_string(obj: &Object) -> String {
    let Ok(bytes) = obj.as_str() else {
        return String::new();
    };
    if bytes.starts_with(&[0xFE, 0xFF]) && bytes.len() >= 4 {
        let u16s: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    bytes.iter().map(|&b| b as char).collect()
}

pub fn open_demo_chile_document() -> Result<DocumentTab, String> {
    let bytes = generate_demo_chile_pdf();
    let mut tab = open_pdf_from_bytes("PLANOS_CHILE_DEMO.pdf", None, bytes)?;
    if let Some(p0) = tab.pages.get_mut(0) {
        p0.scale = PageScale::from_metric_ratio(100.0);
    }
    if let Some(p1) = tab.pages.get_mut(1) {
        p1.scale = PageScale::from_metric_ratio(50.0);
        use crate::model::units::PT_PER_MM;
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
    }
    tab.markups = create_demo_markups();
    tab.dirty = false;
    Ok(tab)
}

pub fn open_pdf_file(path: &Path) -> Result<DocumentTab, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
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
    raw_bytes: Vec<u8>,
) -> Result<DocumentTab, String> {
    let hayro_pdf = Pdf::new(raw_bytes.clone()).map_err(|e| format!("Error al abrir PDF: {e:?}"))?;
    let mut pages = Vec::new();
    for (idx, hp) in hayro_pdf.pages().iter().enumerate() {
        let (w, h) = hp.render_dimensions();
        pages.push(PageModel::new(idx, w as f64, h as f64));
    }
    if pages.is_empty() {
        return Err("El archivo PDF no contiene páginas.".to_string());
    }

    let doc = Document::load_mem(&raw_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let page_map = doc.get_pages(); // BTreeMap<u32 (1-based), ObjectId>
    let mut page_id_to_idx: HashMap<ObjectId, usize> = HashMap::new();
    for (&num, &oid) in &page_map {
        page_id_to_idx.insert(oid, num.saturating_sub(1) as usize);
    }

    // Extract text per page for instant Search panel
    let mut search_text_by_page = vec![String::new(); pages.len()];
    for &num in page_map.keys() {
        let idx = num.saturating_sub(1) as usize;
        if idx < search_text_by_page.len()
            && let Ok(txt) = doc.extract_text(&[num])
        {
            search_text_by_page[idx] = txt;
        }
    }

    // Read embedded lossless RedBIMState if present
    if let Some(state) = read_embedded_state(&doc) {
        let mut tab = DocumentTab::new_with_pages(title, file_path, raw_bytes, pages);
        for (i, saved_p) in state.pages.into_iter().enumerate() {
            if let Some(p) = tab.pages.get_mut(i) {
                p.scale = saved_p.scale;
                p.viewports = saved_p.viewports;
                p.rotation_deg = saved_p.rotation_deg;
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
        if let Ok(vp_list) = read_page_viewports(&doc, page_id, page_h) {
            page_model.viewports = vp_list;
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
    page_h: f64,
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
        let rect = Rect2::from_min_max(
            Point2::new(x1, page_h - y2),
            Point2::new(x2, page_h - y1),
        );
        let name = dict
            .get(b"Name")
            .map(decode_pdf_string)
            .unwrap_or_else(|_| "Viewport".to_string());
        let mut scale = PageScale::default();
        if let Ok(m_dict) = doc.get_dict_in_dict(&dict, b"Measure") {
            let ratio_str = m_dict.get(b"R").map(decode_pdf_string).unwrap_or_default();
            if let Some(rest) = ratio_str.strip_prefix("1:")
                && let Ok(ratio) = rest.trim().parse::<f64>()
                && ratio > 0.0
            {
                scale = PageScale::from_metric_ratio(ratio);
            }
        }
        viewports.push(ViewportScale::new(name, rect, scale));
    }
    Ok(viewports)
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

pub fn save_pdf_file(tab: &DocumentTab, path: &Path) -> Result<Vec<u8>, String> {
    let bytes = save_pdf_to_bytes(tab)?;
    std::fs::write(path, &bytes).map_err(|e| format!("Error al guardar {}: {e}", path.display()))?;
    Ok(bytes)
}

pub fn save_pdf_to_bytes(tab: &DocumentTab) -> Result<Vec<u8>, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error al procesar PDF: {e}"))?;
    let pages = doc.get_pages();
    let ap_res = crate::pdf::flatten::PdfOverlayResources::create(&mut doc);

    for (&page_num, &page_id) in &pages {
        let idx = page_num.saturating_sub(1) as usize;
        let Some(page_model) = tab.pages.get(idx) else {
            continue;
        };
        let page_h = page_model.height_pt;

        // Write ISO 32000-1 /Annots + Form XObject /AP appearance streams for all markups on this page
        let mut annot_refs = Vec::new();
        for m in tab.markups.iter().filter(|m| m.page_index == idx) {
            let mut annot_dict = markup_to_pdf_annot(m, page_model, page_h);
            let ap_id = crate::pdf::flatten::build_markup_appearance_stream(
                &mut doc, &ap_res, m, page_model, page_h,
            );
            annot_dict.set("AP", dictionary! { "N" => ap_id });
            let aid = doc.add_object(annot_dict);
            annot_refs.push(Object::Reference(aid));
        }

        // Write PDF /VP Viewports + Page-level scale
        let mut vp_array = Vec::new();
        for vp in &page_model.viewports {
            let measure = build_measure_dict(&vp.scale);
            let m_id = doc.add_object(measure);
            let vp_dict = dictionary! {
                "Type" => "Viewport",
                "Name" => pdf_utf16_string(&vp.name),
                "BBox" => vec![
                    vp.rect.min_x.into(),
                    (page_h - vp.rect.max_y).into(),
                    vp.rect.max_x.into(),
                    (page_h - vp.rect.min_y).into(),
                ],
                "Measure" => m_id,
            };
            vp_array.push(Object::Dictionary(vp_dict));
        }
        if page_model.scale.is_set && vp_array.is_empty() {
            let measure = build_measure_dict(&page_model.scale);
            let m_id = doc.add_object(measure);
            let vp_dict = dictionary! {
                "Type" => "Viewport",
                "Name" => pdf_utf16_string("Página"),
                "BBox" => vec![
                    0.0.into(),
                    0.0.into(),
                    page_model.width_pt.into(),
                    page_model.height_pt.into(),
                ],
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
    let c_factor = scale.pdf_measure_c_factor();
    let x_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => Object::string_literal(to_winansi_bytes(unit_str)),
        "C" => c_factor,
        "D" => 100i64,
    })];
    let d_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => Object::string_literal(to_winansi_bytes(unit_str)),
        "C" => 1.0,
        "D" => 100i64,
    })];
    let a_arr = vec![Object::Dictionary(dictionary! {
        "Type" => "NumberFormat",
        "U" => Object::string_literal(to_winansi_bytes(area_str)),
        "C" => 1.0,
        "D" => 100i64,
    })];
    let ratio = if scale.paper_unit.is_metric() && scale.real_unit.is_metric() {
        format!("1:{:.0}", scale.ratio_x().round())
    } else {
        scale.label()
    };
    dictionary! {
        "Type" => "Measure",
        "Subtype" => "RL",
        "R" => Object::string_literal(to_winansi_bytes(&ratio)),
        "X" => x_arr,
        "D" => d_arr,
        "A" => a_arr,
    }
}

fn color_arr(c: [u8; 4]) -> Vec<Object> {
    vec![
        (c[0] as f64 / 255.0).into(),
        (c[1] as f64 / 255.0).into(),
        (c[2] as f64 / 255.0).into(),
    ]
}

fn markup_to_pdf_annot(m: &Markup, page: &PageModel, page_h: f64) -> Dictionary {
    let b = markup_bounds(m).expand(4.0);
    let rect = vec![
        b.min_x.into(),
        (page_h - b.max_y).into(),
        b.max_x.into(),
        (page_h - b.min_y).into(),
    ];
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
            let anchor = crate::geometry::hit_test::box_rect_anchor(*box_rect, *knee);
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
        _ => {
            d.set("Subtype", "Square");
        }
    }
    d
}

/// Rotates a page in `tab.raw_pdf_bytes` by +90 or -90 degrees and updates `PageModel`.
pub fn rotate_page_in_doc(tab: &mut DocumentTab, page_index: usize, delta_deg: i32) -> Result<(), String> {
    let mut doc = Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let Some(&page_id) = pages.get(&(page_index as u32 + 1)) else {
        return Err("Página no encontrada".to_string());
    };
    let current_rot = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"Rotate").ok())
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0) as i32;
    let new_rot = (current_rot + delta_deg).rem_euclid(360);
    if let Ok(dict) = doc.get_dictionary_mut(page_id) {
        dict.set("Rotate", new_rot as i64);
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    tab.raw_pdf_bytes = out;

    let hayro_pdf = Pdf::new(tab.raw_pdf_bytes.clone()).map_err(|e| format!("{e:?}"))?;
    if let Some(hp) = hayro_pdf.pages().get(page_index)
        && let Some(pm) = tab.pages.get_mut(page_index)
    {
        let (w, h) = hp.render_dimensions();
        pm.width_pt = w as f64;
        pm.height_pt = h as f64;
        pm.rotation_deg = new_rot as u16;
    }
    tab.dirty = true;
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
        "Contents" => content_id,
    });

    // Rebuild flat Kids array on root Pages node (safe across nested page trees via get_pages)
    let mut existing_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
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
    tab.raw_pdf_bytes = out;

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
    }
    tab.search_text_by_page.insert(idx, String::new());
    tab.dirty = true;
    Ok(())
}

/// Deletes page `page_index` (must keep at least 1 page).
pub fn delete_page_in_doc(tab: &mut DocumentTab, page_index: usize) -> Result<(), String> {
    if tab.pages.len() <= 1 || page_index >= tab.pages.len() {
        return Err("No se puede eliminar la única página del documento.".to_string());
    }
    let mut doc = Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| e.to_string())?;
    let catalog_id = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
    let pages_id = doc.get_dictionary(catalog_id).and_then(|d| d.get(b"Pages")).and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;

    let mut existing_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if page_index >= existing_ids.len() {
        return Err("Página inválida".to_string());
    }
    existing_ids.remove(page_index);
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
    tab.raw_pdf_bytes = out;

    tab.pages.remove(page_index);
    for (i, p) in tab.pages.iter_mut().enumerate() {
        p.page_index = i;
    }
    tab.markups.retain(|m| m.page_index != page_index);
    for m in &mut tab.markups {
        if m.page_index > page_index {
            m.page_index -= 1;
        }
    }
    if page_index < tab.search_text_by_page.len() {
        tab.search_text_by_page.remove(page_index);
    }
    let max_idx = tab.pages.len().saturating_sub(1);
    tab.camera.page_index = tab.camera.page_index.min(max_idx);
    tab.secondary_camera.page_index = tab.secondary_camera.page_index.min(max_idx);
    tab.dirty = true;
    Ok(())
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
    if docs_bytes.is_empty() {
        return Err("No hay documentos para combinar.".to_string());
    }
    let mut merged = Document::with_version("1.7");
    let pages_id = merged.new_object_id();
    let mut max_id = pages_id.0 + 1;
    let mut all_page_ids = Vec::new();

    for (i, raw) in docs_bytes.iter().enumerate() {
        let mut src = Document::load_mem(raw)
            .map_err(|e| format!("Error leyendo documento #{}: {e}", i + 1))?;
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
    let mut bufs = Vec::with_capacity(paths.len());
    for p in paths {
        let b = std::fs::read(p).map_err(|e| format!("Error leyendo {}: {e}", p.display()))?;
        bufs.push(b);
    }
    let refs: Vec<&[u8]> = bufs.iter().map(|v| v.as_slice()).collect();
    combine_pdf_bytes(&refs)
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
    let state = RedBimEmbeddedState {
        version: 1,
        pages: extracted_pages,
        markups: extracted_markups,
        layers: tab.layers.clone(),
        bookmarks: Vec::new(),
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

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Utility helper returning unit suffix for LengthUnit when building Scale presets.
#[allow(dead_code)]
fn _unit_is_metric(u: LengthUnit) -> bool {
    u.is_metric()
}
