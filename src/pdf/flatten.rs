use crate::geometry::hit_test::{box_rect_anchor, callout_landing, markup_bounds};
use crate::geometry::math::{
    Point2, Rect2, clip_hatch_lines, dimension_offset_segment, generate_cloud_polyline,
    polygon_centroid, sample_ellipse, sample_three_point_arc,
};
use crate::model::document::{DocumentTab, PageModel, RedBimEmbeddedState};
use crate::model::markup::{HatchPattern, LineEnd, LineStyle, Markup, MarkupKind};
use crate::pdf::base64_decode;
use crate::pdf::demo_blueprint::to_winansi_bytes;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct PdfOverlayResources {
    pub font_reg: ObjectId,
    pub font_bold: ObjectId,
    pub gs_full: ObjectId,
    pub gs_fill: ObjectId,
    pub gs_hi: ObjectId,
}

impl PdfOverlayResources {
    pub fn create(doc: &mut Document) -> Self {
        let font_reg = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let font_bold = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica-Bold",
            "Encoding" => "WinAnsiEncoding",
        });
        let gs_full = doc.add_object(dictionary! {
            "Type" => "ExtGState",
            "ca" => 1.0,
            "CA" => 1.0,
        });
        let gs_fill = doc.add_object(dictionary! {
            "Type" => "ExtGState",
            "ca" => 0.25,
            "CA" => 1.0,
        });
        let gs_hi = doc.add_object(dictionary! {
            "Type" => "ExtGState",
            "ca" => 0.42,
            "CA" => 0.42,
            "BM" => "Multiply",
        });
        Self {
            font_reg,
            font_bold,
            gs_full,
            gs_fill,
            gs_hi,
        }
    }

    pub fn into_dict(self, xobjects: Dictionary) -> Dictionary {
        let mut res = dictionary! {
            "Font" => dictionary! { "RB_F1" => self.font_reg, "RB_F2" => self.font_bold },
            "ExtGState" => dictionary! { "RB_GS1" => self.gs_full, "RB_GSF" => self.gs_fill, "RB_GSH" => self.gs_hi },
        };
        if !xobjects.is_empty() {
            res.set("XObject", xobjects);
        }
        res
    }
}

/// Canonical visual bounding box used identically for `/Rect` and `/AP /N /BBox`
/// so external ISO 32000-1 PDF viewers never non-uniformly squash appearance streams.
pub fn markup_visual_bounds(m: &Markup) -> Rect2 {
    let base = markup_bounds(m);
    let pad = (m.style.line_width as f64 * 1.5).max(12.0);
    base.expand(pad)
}

/// Builds a standard ISO 32000-1 Form XObject Appearance Stream (`/AP << /N ... >>`)
/// for any RedBIM markup or measurement so external PDF viewers render it natively.
pub fn build_markup_appearance_stream(
    doc: &mut Document,
    res: &PdfOverlayResources,
    m: &Markup,
    page: &PageModel,
    page_h: f64,
) -> ObjectId {
    let b = markup_visual_bounds(m);
    let [bx0, by0, bx1, by1] = page.page_rect_to_pdf_rect(b);
    let bbox = vec![bx0.into(), by0.into(), bx1.into(), by1.into()];
    let mut ops = Vec::new();
    let mut xobjects = Dictionary::new();
    ops.push(Operation::new("q", vec![]));
    if let Some([a, b_m, c, d, e, f]) = page.pdf_overlay_cm_matrix() {
        ops.push(Operation::new(
            "cm",
            vec![a.into(), b_m.into(), c.into(), d.into(), e.into(), f.into()],
        ));
    }
    draw_markup_pdf_ops(doc, &mut ops, &mut xobjects, m, page, page_h, page.page_index);
    ops.push(Operation::new("Q", vec![]));
    let content = Content { operations: ops }.encode().unwrap_or_default();
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1i64,
            "BBox" => bbox,
            "Matrix" => vec![1.0.into(), 0.0.into(), 0.0.into(), 1.0.into(), 0.0.into(), 0.0.into()],
            "Resources" => res.into_dict(xobjects),
        },
        content,
    );
    let _ = stream.compress();
    doc.add_object(stream)
}

/// Flattens all RedBIM markups and measurements directly into the PDF page content streams
/// and clears `/Annots` for universal viewing (recoverable by default for `Unflatten`).
pub fn flatten_pdf_to_bytes(tab: &DocumentTab) -> Result<Vec<u8>, String> {
    flatten_pdf_to_bytes_with_options(tab, true)
}

/// Flattens all visible RedBIM markups into PDF page content streams wrapped in ISO 32000-1
/// Marked Content (`/RedBIM_Flattened BDC ... EMC`). When `recoverable` is true, embeds a
/// compressed `/RedBIMFlattened` recovery payload in the PDF catalog so `unflatten_pdf_in_tab`
/// can reverse the flatten losslessly (mirroring Bluebeam's `SCmdFlatten` / `SCmdUnflatten`).
pub fn flatten_pdf_to_bytes_with_options(
    tab: &DocumentTab,
    recoverable: bool,
) -> Result<Vec<u8>, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let res = PdfOverlayResources::create(&mut doc);
    let q_push_id = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
    let q_pop_id = doc.add_object(Stream::new(dictionary! {}, b"\nQ\n".to_vec()));

    let mut flattened_markups: Vec<Markup> = Vec::new();
    let markups_by_page = tab.markups_by_page();
    let pages = doc.get_pages();
    for (&page_num, &page_id) in &pages {
        let idx = page_num.saturating_sub(1) as usize;
        let Some(page_model) = tab.pages.get(idx) else {
            continue;
        };
        let page_markups: Vec<&Markup> = markups_by_page[idx]
            .iter()
            .copied()
            .filter(|m| tab.is_layer_visible(&m.layer))
            .collect();
        if page_markups.is_empty() {
            if let Ok(p) = doc.get_dictionary_mut(page_id) {
                p.remove(b"Annots");
            }
            continue;
        }

        let page_h = page_model.height_pt;
        let mut ops = Vec::new();
        let mut xobjects = Dictionary::new();
        ops.push(Operation::new(
            "BDC",
            vec![
                Object::Name(b"RedBIM_Flattened".to_vec()),
                Object::Dictionary(dictionary! { "Count" => page_markups.len() as i64 }),
            ],
        ));
        ops.push(Operation::new("q", vec![]));
        if let Some([a, b_m, c, d, e, f]) = page_model.pdf_overlay_cm_matrix() {
            ops.push(Operation::new(
                "cm",
                vec![a.into(), b_m.into(), c.into(), d.into(), e.into(), f.into()],
            ));
        }

        for m in page_markups {
            if recoverable {
                flattened_markups.push(m.clone());
            }
            draw_markup_pdf_ops(&mut doc, &mut ops, &mut xobjects, m, page_model, page_h, idx);
        }

        ops.push(Operation::new("Q", vec![]));
        ops.push(Operation::new("EMC", vec![]));
        let overlay_bytes = Content { operations: ops }.encode().unwrap_or_default();
        let mut overlay_stream = Stream::new(
            dictionary! { "RedBIMFlattenedOverlay" => true },
            overlay_bytes,
        );
        let _ = overlay_stream.compress();
        let overlay_id = doc.add_object(overlay_stream);

        // Isolate existing page streams inside `q ... Q` so dirty CAD CTMs / clip paths
        // never shift, scale, or clip our flattened overlay.
        let existing_contents = doc.get_page_contents(page_id);
        let mut new_contents: Vec<Object> = Vec::with_capacity(existing_contents.len() + 3);
        if !existing_contents.is_empty() {
            new_contents.push(Object::Reference(q_push_id));
            new_contents.extend(existing_contents.into_iter().map(Object::Reference));
            new_contents.push(Object::Reference(q_pop_id));
        }
        new_contents.push(Object::Reference(overlay_id));

        // Safely materialize & clone /Resources (handling inherited & indirect sub-dicts)
        // so we never clobber original page fonts/images or collide across shared /Resources.
        let mut res_dict = resolve_page_resources_cloned(&doc, page_id);
        inject_resources_safely(&doc, &mut res_dict, &res, &xobjects);

        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            page_dict.set("Contents", new_contents);
            page_dict.set("Resources", Object::Dictionary(res_dict));
            page_dict.remove(b"Annots");
        }
    }

    // Update catalog metadata: strip live /RedBIMState (so reopening won't double-render)
    // and optionally store recoverable /RedBIMFlattened state for Unflatten.
    if let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        let recovery_stream_id = if recoverable && !flattened_markups.is_empty() {
            let mut merged_markups = read_flattened_state(&doc)
                .map(|s| s.markups)
                .unwrap_or_default();
            merged_markups.extend(flattened_markups);
            let recovery = RedBimEmbeddedState {
                version: 2,
                pages: tab.pages.clone(),
                markups: merged_markups,
                layers: tab.layers.clone(),
                bookmarks: tab.bookmarks.clone(),
                spaces: tab.spaces.clone(),
                custom_columns: tab.custom_columns.clone(),
            };
            serde_json::to_vec(&recovery).ok().map(|json| {
                let mut s = Stream::new(dictionary! {}, json);
                let _ = s.compress();
                doc.add_object(s)
            })
        } else {
            None
        };
        if let Ok(cat) = doc.get_dictionary_mut(catalog_id) {
            cat.remove(b"RedBIMState");
            if let Some(sid) = recovery_stream_id {
                cat.set("RedBIMFlattened", sid);
            } else {
                cat.remove(b"RedBIMFlattened");
            }
        }
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error al exportar PDF aplanado: {e}"))?;
    Ok(out)
}

fn read_flattened_state(doc: &Document) -> Option<RedBimEmbeddedState> {
    let catalog_id = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let catalog = doc.get_dictionary(catalog_id).ok()?;
    let state_id = catalog.get(b"RedBIMFlattened").ok()?.as_reference().ok()?;
    let stream = doc.get_object(state_id).ok()?.as_stream().ok()?;
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    serde_json::from_slice(&bytes).ok()
}

/// Returns true when the PDF bytes carry a recoverable `/RedBIMFlattened` payload.
pub fn has_recoverable_flattened_markups(raw_pdf_bytes: &[u8]) -> bool {
    Document::load_mem(raw_pdf_bytes)
        .ok()
        .and_then(|d| read_flattened_state(&d))
        .is_some_and(|s| !s.markups.is_empty())
}

/// Flattens visible markups in-place on `tab`, updating `raw_pdf_bytes` and removing
/// flattened markups from `tab.markups` (mirroring Bluebeam's in-document Flatten).
pub fn flatten_in_place_in_tab(tab: &mut DocumentTab, recoverable: bool) -> Result<usize, String> {
    let count = tab
        .markups
        .iter()
        .filter(|m| tab.is_layer_visible(&m.layer))
        .count();
    if count == 0 {
        return Err("No hay marcas visibles para aplanar en este documento.".to_string());
    }
    let flattened = flatten_pdf_to_bytes_with_options(tab, recoverable)?;
    tab.raw_pdf_bytes = flattened.into();
    let hidden_layers: std::collections::HashSet<String> = tab
        .layers
        .iter()
        .filter(|l| !l.visible)
        .map(|l| l.name.clone())
        .collect();
    tab.markups.retain(|m| hidden_layers.contains(&m.layer));
    tab.selected_ids.clear();
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
    Ok(count)
}

/// Reverses a recoverable flatten operation (`SCmdUnflatten`): strips `/RedBIM_Flattened`
/// overlay content streams from every page and restores the original editable markups.
pub fn unflatten_pdf_in_tab(tab: &mut DocumentTab) -> Result<usize, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let Some(state) = read_flattened_state(&doc) else {
        return Err(
            "Este PDF no contiene marcas aplanadas recuperables (RedBIMFlattened).".to_string(),
        );
    };
    if state.markups.is_empty() {
        return Err("No hay marcas recuperables registradas en este PDF.".to_string());
    }

    // Strip every RedBIM flattened overlay stream from all pages' /Contents
    let pages = doc.get_pages();
    for &page_id in pages.values() {
        let existing = doc.get_page_contents(page_id);
        let mut kept: Vec<Object> = Vec::with_capacity(existing.len());
        for cid in existing {
            let is_overlay = doc
                .get_object(cid)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .is_some_and(|st| {
                    if st
                        .dict
                        .get(b"RedBIMFlattenedOverlay")
                        .and_then(|o| o.as_bool())
                        .unwrap_or(false)
                    {
                        return true;
                    }
                    let raw = st
                        .decompressed_content()
                        .unwrap_or_else(|_| st.content.clone());
                    raw.windows(b"/RedBIM_Flattened".len())
                        .any(|w| w == b"/RedBIM_Flattened")
                });
            if !is_overlay {
                kept.push(Object::Reference(cid));
            }
        }
        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            page_dict.set("Contents", kept);
        }
    }

    if let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(|o| o.as_reference())
        && let Ok(cat) = doc.get_dictionary_mut(catalog_id)
    {
        cat.remove(b"RedBIMFlattened");
    }

    let mut clean_bytes = Vec::new();
    doc.save_to(&mut clean_bytes)
        .map_err(|e| format!("Error al desaplanar PDF: {e}"))?;
    tab.raw_pdf_bytes = clean_bytes.into();

    // Restore recovered markups (deduplicating by UUID against any markups added since flatten)
    let existing_ids: std::collections::HashSet<_> = tab.markups.iter().map(|m| m.id).collect();
    let mut restored = 0usize;
    for m in state.markups {
        if m.page_index < tab.pages.len() && !existing_ids.contains(&m.id) {
            tab.ensure_layer(&m.layer);
            tab.markups.push(m);
            restored += 1;
        }
    }
    if tab.spaces.is_empty() && !state.spaces.is_empty() {
        tab.spaces = state.spaces;
    }
    if !state.custom_columns.is_empty() {
        for col in state.custom_columns {
            if !tab.custom_columns.iter().any(|c| c.id == col.id) {
                tab.custom_columns.push(col);
            }
        }
    }
    for (i, sp) in state.pages.into_iter().enumerate() {
        if let Some(p) = tab.pages.get_mut(i) {
            if !p.scale.is_set && sp.scale.is_set {
                p.scale = sp.scale;
            }
            if p.label.is_none() && sp.label.is_some() {
                p.label = sp.label;
            }
            if p.viewports.is_empty() && !sp.viewports.is_empty() {
                p.viewports = sp.viewports;
            }
        }
    }

    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
    Ok(restored)
}

pub fn flatten_pdf_file(tab: &DocumentTab, path: &Path) -> Result<(), String> {
    let bytes = flatten_pdf_to_bytes(tab)?;
    crate::pdf::io::atomic_write_file(path, &bytes)
}

fn resolve_page_resources_cloned(doc: &Document, page_id: ObjectId) -> Dictionary {
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

fn resolve_subdict_cloned(doc: &Document, parent: &Dictionary, key: &[u8]) -> Dictionary {
    match parent.get(key) {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).cloned().unwrap_or_default(),
        _ => Dictionary::new(),
    }
}

fn inject_resources_safely(
    doc: &Document,
    res_dict: &mut Dictionary,
    res: &PdfOverlayResources,
    xobjects: &Dictionary,
) {
    let mut fonts = resolve_subdict_cloned(doc, res_dict, b"Font");
    fonts.set("RB_F1", res.font_reg);
    fonts.set("RB_F2", res.font_bold);
    res_dict.set("Font", Object::Dictionary(fonts));

    let mut gs = resolve_subdict_cloned(doc, res_dict, b"ExtGState");
    gs.set("RB_GS1", res.gs_full);
    gs.set("RB_GSF", res.gs_fill);
    gs.set("RB_GSH", res.gs_hi);
    res_dict.set("ExtGState", Object::Dictionary(gs));

    if !xobjects.is_empty() {
        let mut xo = resolve_subdict_cloned(doc, res_dict, b"XObject");
        for (k, v) in xobjects.iter() {
            xo.set(k.clone(), v.clone());
        }
        res_dict.set("XObject", Object::Dictionary(xo));
    }
}

fn embed_image_xobject(doc: &mut Document, png_b64: &str) -> Option<ObjectId> {
    let raw = base64_decode(png_b64)?;
    let dyn_img = image::load_from_memory(&raw).ok()?;
    let rgb = dyn_img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => (w as i64),
            "Height" => (h as i64),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8i64,
        },
        rgb.into_raw(),
    );
    let _ = stream.compress();
    Some(doc.add_object(stream))
}

fn draw_markup_pdf_ops(
    doc: &mut Document,
    ops: &mut Vec<Operation>,
    xobjects: &mut Dictionary,
    m: &Markup,
    page: &PageModel,
    page_h: f64,
    page_idx: usize,
) {
    let py = |y: f64| page_h - y;
    let sc = m.style.stroke_color;
    let sr = sc[0] as f64 / 255.0;
    let sg = sc[1] as f64 / 255.0;
    let sb = sc[2] as f64 / 255.0;

    ops.push(Operation::new("q", vec![]));
    ops.push(Operation::new(
        "gs",
        vec![Object::Name(b"RB_GS1".to_vec())],
    ));
    ops.push(Operation::new("RG", vec![sr.into(), sg.into(), sb.into()]));
    ops.push(Operation::new(
        "w",
        vec![(m.style.line_width as f64).max(0.5).into()],
    ));

    match m.style.line_style {
        LineStyle::Dashed => {
            ops.push(Operation::new(
                "d",
                vec![vec![6.0.into(), 4.0.into()].into(), 0.into()],
            ));
        }
        LineStyle::Dotted => {
            ops.push(Operation::new(
                "d",
                vec![vec![2.0.into(), 3.0.into()].into(), 0.into()],
            ));
        }
        LineStyle::DashDot => {
            ops.push(Operation::new(
                "d",
                vec![
                    vec![8.0.into(), 3.0.into(), 2.0.into(), 3.0.into()].into(),
                    0.into(),
                ],
            ));
        }
        _ => {}
    }

    let emit_poly = |ops: &mut Vec<Operation>, pts: &[Point2], closed: bool, fill: Option<[u8; 4]>| {
        if pts.len() < 2 {
            return;
        }
        if let Some(fc) = fill {
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "gs",
                vec![Object::Name(b"RB_GSF".to_vec())],
            ));
            ops.push(Operation::new(
                "rg",
                vec![
                    (fc[0] as f64 / 255.0).into(),
                    (fc[1] as f64 / 255.0).into(),
                    (fc[2] as f64 / 255.0).into(),
                ],
            ));
            ops.push(Operation::new("m", vec![pts[0].x.into(), py(pts[0].y).into()]));
            for p in &pts[1..] {
                ops.push(Operation::new("l", vec![p.x.into(), py(p.y).into()]));
            }
            ops.push(Operation::new("h", vec![]));
            ops.push(Operation::new("f", vec![]));
            ops.push(Operation::new("Q", vec![]));
        }
        ops.push(Operation::new("m", vec![pts[0].x.into(), py(pts[0].y).into()]));
        for p in &pts[1..] {
            ops.push(Operation::new("l", vec![p.x.into(), py(p.y).into()]));
        }
        if closed {
            ops.push(Operation::new("s", vec![]));
        } else {
            ops.push(Operation::new("S", vec![]));
        }
    };

    let emit_text = |ops: &mut Vec<Operation>, x: f64, y_top: f64, size: f64, col: [u8; 4], txt: &str| {
        if txt.is_empty() {
            return;
        }
        ops.push(Operation::new(
            "rg",
            vec![
                (col[0] as f64 / 255.0).into(),
                (col[1] as f64 / 255.0).into(),
                (col[2] as f64 / 255.0).into(),
            ],
        ));
        let font_name = if m.style.bold { b"RB_F2".to_vec() } else { b"RB_F1".to_vec() };
        let line_h = size * 1.25;
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(font_name), size.into()],
        ));
        for (li, line) in txt.split('\n').enumerate() {
            let clean_line = line.trim_end_matches('\r');
            if li == 0 {
                ops.push(Operation::new("Td", vec![x.into(), py(y_top).into()]));
            } else {
                ops.push(Operation::new("Td", vec![0.0.into(), (-line_h).into()]));
            }
            ops.push(Operation::new(
                "Tj",
                vec![Object::string_literal(to_winansi_bytes(clean_line))],
            ));
        }
        ops.push(Operation::new("ET", vec![]));
    };

    let emit_arrow_head = |ops: &mut Vec<Operation>, from: Point2, to: Point2, end: LineEnd| {
        if end == LineEnd::None {
            return;
        }
        let d = (to - from).normalize();
        let n = d.perp();
        let sz = (m.style.line_width as f64 * 3.5).clamp(6.0, 14.0);
        match end {
            LineEnd::OpenArrow => {
                let back = to - d * sz;
                let left = back + n * (sz * 0.45);
                let right = back - n * (sz * 0.45);
                ops.push(Operation::new("m", vec![left.x.into(), py(left.y).into()]));
                ops.push(Operation::new("l", vec![to.x.into(), py(to.y).into()]));
                ops.push(Operation::new("l", vec![right.x.into(), py(right.y).into()]));
                ops.push(Operation::new("S", vec![]));
            }
            LineEnd::ClosedArrow => {
                let back = to - d * sz;
                let left = back + n * (sz * 0.45);
                let right = back - n * (sz * 0.45);
                ops.push(Operation::new("rg", vec![sr.into(), sg.into(), sb.into()]));
                ops.push(Operation::new("m", vec![to.x.into(), py(to.y).into()]));
                ops.push(Operation::new("l", vec![left.x.into(), py(left.y).into()]));
                ops.push(Operation::new("l", vec![right.x.into(), py(right.y).into()]));
                ops.push(Operation::new("h", vec![]));
                ops.push(Operation::new("f", vec![]));
            }
            LineEnd::Slash => {
                let s = (d + n).normalize() * (sz * 0.55);
                let a = to - s;
                let b = to + s;
                ops.push(Operation::new("m", vec![a.x.into(), py(a.y).into()]));
                ops.push(Operation::new("l", vec![b.x.into(), py(b.y).into()]));
                ops.push(Operation::new("S", vec![]));
            }
            LineEnd::Butt => {
                let a = to - n * (sz * 0.5);
                let b = to + n * (sz * 0.5);
                ops.push(Operation::new("m", vec![a.x.into(), py(a.y).into()]));
                ops.push(Operation::new("l", vec![b.x.into(), py(b.y).into()]));
                ops.push(Operation::new("S", vec![]));
            }
            _ => {
                let r = sz * 0.35;
                let b = Rect2::from_xywh(to.x - r, to.y - r, r * 2.0, r * 2.0);
                let c = b.corners();
                ops.push(Operation::new("m", vec![c[0].x.into(), py(c[0].y).into()]));
                for p in &c[1..] {
                    ops.push(Operation::new("l", vec![p.x.into(), py(p.y).into()]));
                }
                ops.push(Operation::new("s", vec![]));
            }
        }
    };

    let metrics = m.compute_metrics(&page.scale, &page.viewports);

    match &m.kind {
        MarkupKind::Note { pos } => {
            let r = Rect2::from_xywh(pos.x - 9.0, pos.y - 9.0, 18.0, 18.0);
            emit_poly(
                ops,
                &r.corners(),
                true,
                m.style.fill_color.or(Some([250, 204, 21, 255])),
            );
        }
        MarkupKind::TextBox { rect, text } => {
            emit_poly(ops, &rect.corners(), true, m.style.fill_color);
            emit_text(
                ops,
                rect.min_x + 5.0,
                rect.min_y + m.style.font_size as f64 + 4.0,
                m.style.font_size as f64,
                m.style.text_color,
                text,
            );
        }
        MarkupKind::Typewriter { pos, text } => {
            emit_text(
                ops,
                pos.x,
                pos.y + m.style.font_size as f64,
                m.style.font_size as f64,
                m.style.text_color,
                text,
            );
        }
        MarkupKind::Callout {
            arrow_tip,
            knee,
            box_rect,
            text,
        } => {
            let (knee, anchor) = callout_landing(*box_rect, *knee);
            emit_poly(ops, &[*arrow_tip, knee, anchor], false, None);
            emit_arrow_head(ops, knee, *arrow_tip, LineEnd::ClosedArrow);
            emit_poly(ops, &box_rect.corners(), true, m.style.fill_color);
            emit_text(
                ops,
                box_rect.min_x + 5.0,
                box_rect.min_y + m.style.font_size as f64 + 5.0,
                m.style.font_size as f64,
                m.style.text_color,
                text,
            );
        }
        MarkupKind::Cloud { points, .. } => {
            let cloud = generate_cloud_polyline(points, m.style.cloud_arc_size as f64);
            emit_poly(ops, &cloud, true, m.style.fill_color);
        }
        MarkupKind::CloudPlus {
            cloud_points,
            knee,
            box_rect,
            text,
            ..
        } => {
            let cloud = generate_cloud_polyline(cloud_points, m.style.cloud_arc_size as f64);
            emit_poly(ops, &cloud, true, None);
            let c_cent = polygon_centroid(cloud_points);
            let anchor = box_rect_anchor(*box_rect, *knee);
            emit_poly(ops, &[c_cent, *knee, anchor], false, None);
            emit_poly(ops, &box_rect.corners(), true, m.style.fill_color);
            emit_text(
                ops,
                box_rect.min_x + 5.0,
                box_rect.min_y + m.style.font_size as f64 + 5.0,
                m.style.font_size as f64,
                m.style.text_color,
                text,
            );
        }
        MarkupKind::Line { start, end } => {
            emit_poly(ops, &[*start, *end], false, None);
        }
        MarkupKind::Arrow { start, end } => {
            emit_poly(ops, &[*start, *end], false, None);
            emit_arrow_head(ops, *start, *end, LineEnd::ClosedArrow);
        }
        MarkupKind::Polyline { points } | MarkupKind::MeasurePolylength { points, .. } => {
            emit_poly(ops, points, false, None);
        }
        MarkupKind::Arc { start, mid, end } => {
            let pts = sample_three_point_arc(*start, *mid, *end, 24);
            emit_poly(ops, &pts, false, None);
        }
        MarkupKind::Rectangle { rect } | MarkupKind::SketchRect { rect, .. } => {
            emit_poly(ops, &rect.corners(), true, m.style.fill_color);
        }
        MarkupKind::Ellipse { rect } => {
            let pts = sample_ellipse(*rect, 32);
            emit_poly(ops, &pts, true, m.style.fill_color);
        }
        MarkupKind::Polygon { points }
        | MarkupKind::MeasurePerimeter {
            points,
            closed: true,
            ..
        }
        | MarkupKind::SketchPoly {
            points,
            closed: true,
            ..
        } => {
            emit_poly(ops, points, true, m.style.fill_color);
        }
        MarkupKind::MeasurePerimeter {
            points,
            closed: false,
            ..
        }
        | MarkupKind::SketchPoly {
            points,
            closed: false,
            ..
        } => {
            emit_poly(ops, points, false, None);
        }
        MarkupKind::MeasureArea {
            points, cutouts, ..
        }
        | MarkupKind::MeasureVolume {
            points, cutouts, ..
        } => {
            // Compound even-odd fill (`f*`) properly knocks out cutout holes
            if let Some(fc) = m.style.fill_color
                && points.len() >= 3
            {
                ops.push(Operation::new("q", vec![]));
                ops.push(Operation::new(
                    "gs",
                    vec![Object::Name(b"RB_GSF".to_vec())],
                ));
                ops.push(Operation::new(
                    "rg",
                    vec![
                        (fc[0] as f64 / 255.0).into(),
                        (fc[1] as f64 / 255.0).into(),
                        (fc[2] as f64 / 255.0).into(),
                    ],
                ));
                ops.push(Operation::new("m", vec![points[0].x.into(), py(points[0].y).into()]));
                for p in &points[1..] {
                    ops.push(Operation::new("l", vec![p.x.into(), py(p.y).into()]));
                }
                ops.push(Operation::new("h", vec![]));
                for h in cutouts {
                    if h.len() >= 3 {
                        ops.push(Operation::new("m", vec![h[0].x.into(), py(h[0].y).into()]));
                        for p in &h[1..] {
                            ops.push(Operation::new("l", vec![p.x.into(), py(p.y).into()]));
                        }
                        ops.push(Operation::new("h", vec![]));
                    }
                }
                ops.push(Operation::new("f*", vec![]));
                ops.push(Operation::new("Q", vec![]));
            }
            emit_poly(ops, points, true, None);
            for h in cutouts {
                emit_poly(ops, h, true, None);
            }
            if m.style.hatch != HatchPattern::None {
                for angle in match m.style.hatch {
                    HatchPattern::Diagonal45 => vec![45.0],
                    HatchPattern::Diagonal135 => vec![135.0],
                    HatchPattern::CrossHatch => vec![45.0, 135.0],
                    HatchPattern::Horizontal => vec![0.0],
                    HatchPattern::Vertical => vec![90.0],
                    HatchPattern::None => vec![],
                } {
                    for (a, b) in clip_hatch_lines(points, cutouts, angle, 12.0) {
                        emit_poly(ops, &[a, b], false, None);
                    }
                }
            }
        }
        MarkupKind::Ink { strokes } => {
            for s in strokes {
                emit_poly(ops, s, false, None);
            }
        }
        MarkupKind::Highlight { strokes } => {
            ops.push(Operation::new(
                "gs",
                vec![Object::Name(b"RB_GSH".to_vec())],
            ));
            ops.push(Operation::new("J", vec![1i64.into()]));
            ops.push(Operation::new("j", vec![1i64.into()]));
            for s in strokes {
                emit_poly(ops, s, false, None);
            }
        }
        MarkupKind::Stamp {
            rect,
            title,
            subtitle,
            ..
        } => {
            emit_poly(ops, &rect.corners(), true, m.style.fill_color);
            if rect.width() > 10.0 && rect.height() > 10.0 {
                emit_poly(ops, &rect.expand(-4.0).corners(), true, None);
            }
            emit_text(
                ops,
                rect.min_x + 12.0,
                rect.center().y,
                m.style.font_size as f64,
                m.style.text_color,
                title,
            );
            if !subtitle.is_empty() {
                emit_text(
                    ops,
                    rect.min_x + 12.0,
                    rect.center().y + 14.0,
                    9.0,
                    m.style.text_color,
                    subtitle,
                );
            }
        }
        MarkupKind::MeasureLength {
            start,
            end,
            offset_pt,
            ..
        } => {
            let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
            emit_poly(ops, &[*start, da], false, None);
            emit_poly(ops, &[*end, db], false, None);
            emit_poly(ops, &[da, db], false, None);
            emit_arrow_head(ops, db, da, m.style.start_end);
            emit_arrow_head(ops, da, db, m.style.end_end);
        }
        MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
            emit_poly(ops, &[*p1, *vertex, *p2], false, None);
        }
        MarkupKind::MeasureRadius { center, edge, .. } => {
            let r = center.dist(*edge);
            let pts = sample_ellipse(
                Rect2::from_xywh(center.x - r, center.y - r, r * 2.0, r * 2.0),
                32,
            );
            emit_poly(ops, &pts, true, None);
            emit_poly(ops, &[*center, *edge], false, None);
        }
        MarkupKind::MeasureDiameter { p1, p2, .. } => {
            let c = p1.midpoint(*p2);
            let r = p1.dist(*p2) * 0.5;
            let pts = sample_ellipse(Rect2::from_xywh(c.x - r, c.y - r, r * 2.0, r * 2.0), 32);
            emit_poly(ops, &pts, true, None);
            emit_poly(ops, &[*p1, *p2], false, None);
        }
        MarkupKind::MeasureCount { points, .. } => {
            for (i, p) in points.iter().enumerate() {
                let b = Rect2::from_xywh(p.x - 7.0, p.y - 7.0, 14.0, 14.0);
                emit_poly(ops, &b.corners(), true, Some(sc));
                emit_text(ops, p.x + 9.0, p.y + 3.0, 8.5, sc, &(i + 1).to_string());
            }
        }
        MarkupKind::Legend {
            pos,
            title,
            width_pt,
        } => {
            let b = Rect2::from_xywh(pos.x, pos.y, *width_pt, 48.0);
            emit_poly(ops, &b.corners(), true, Some([245, 245, 248, 255]));
            emit_text(ops, pos.x + 8.0, pos.y + 18.0, 10.0, sc, title);
        }
        MarkupKind::Hyperlink {
            rect, description, ..
        } => {
            emit_poly(ops, &rect.corners(), true, m.style.fill_color.or(Some([37, 99, 235, 36])));
            if !description.is_empty() {
                emit_text(ops, rect.min_x + 4.0, rect.min_y + 12.0, 9.0, sc, description);
            }
        }
        MarkupKind::Redaction {
            rect,
            overlay_text,
        } => {
            emit_poly(ops, &rect.corners(), true, Some([0, 0, 0, 255]));
            if !overlay_text.trim().is_empty() {
                let cx = (rect.min_x + rect.max_x) * 0.5
                    - overlay_text.chars().count() as f64 * 3.0;
                let cy = (rect.min_y + rect.max_y) * 0.5 + 3.0;
                emit_text(ops, cx.max(rect.min_x + 4.0), cy, 10.0, [255, 255, 255, 255], overlay_text);
            }
        }
        MarkupKind::Image { rect, png_b64, .. } => {
            if let Some(img_id) = embed_image_xobject(doc, png_b64) {
                let name = format!("RB_IM_{page_idx}_{}", xobjects.len());
                xobjects.set(name.as_bytes(), img_id);
                ops.push(Operation::new("q", vec![]));
                ops.push(Operation::new(
                    "cm",
                    vec![
                        rect.width().into(),
                        0.0.into(),
                        0.0.into(),
                        rect.height().into(),
                        rect.min_x.into(),
                        py(rect.max_y).into(),
                    ],
                ));
                ops.push(Operation::new(
                    "Do",
                    vec![Object::Name(name.into_bytes())],
                ));
                ops.push(Operation::new("Q", vec![]));
            }
            emit_poly(ops, &rect.corners(), true, None);
        }
    }

    let show_caption = m.measurement_opts().map(|o| o.show_caption).unwrap_or(true);
    if show_caption
        && let Some(caption) = metrics.primary_display
    {
        let anchor = match &m.kind {
            MarkupKind::MeasureLength {
                start,
                end,
                offset_pt,
                ..
            } => {
                let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
                da.midpoint(db)
            }
            _ => m.anchor_point(),
        };
        emit_text(ops, anchor.x + 4.0, anchor.y - 4.0, 10.0, sc, &caption);
    }

    ops.push(Operation::new("Q", vec![]));
}
