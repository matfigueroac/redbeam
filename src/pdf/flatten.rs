use crate::geometry::hit_test::{box_rect_anchor, markup_bounds};
use crate::geometry::math::{
    Point2, Rect2, clip_hatch_lines, dimension_offset_segment, generate_cloud_polyline,
    polygon_centroid, sample_ellipse, sample_three_point_arc,
};
use crate::model::document::{DocumentTab, PageModel};
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

/// Builds a standard ISO 32000-1 Form XObject Appearance Stream (`/AP << /N ... >>`)
/// for any RedBIM markup or measurement so external PDF viewers render it natively.
pub fn build_markup_appearance_stream(
    doc: &mut Document,
    res: &PdfOverlayResources,
    m: &Markup,
    page: &PageModel,
    page_h: f64,
) -> ObjectId {
    let b = markup_bounds(m).expand(24.0);
    let bbox = vec![
        b.min_x.into(),
        (page_h - b.max_y).into(),
        b.max_x.into(),
        (page_h - b.min_y).into(),
    ];
    let mut ops = Vec::new();
    let mut xobjects = Dictionary::new();
    draw_markup_pdf_ops(doc, &mut ops, &mut xobjects, m, page, page_h);
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
/// and clears `/Annots` for universal viewing.
pub fn flatten_pdf_to_bytes(tab: &DocumentTab) -> Result<Vec<u8>, String> {
    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let res = PdfOverlayResources::create(&mut doc);

    let pages = doc.get_pages();
    for (&page_num, &page_id) in &pages {
        let idx = page_num.saturating_sub(1) as usize;
        let Some(page_model) = tab.pages.get(idx) else {
            continue;
        };
        let page_markups: Vec<&Markup> = tab
            .markups
            .iter()
            .filter(|m| m.page_index == idx && tab.is_layer_visible(&m.layer))
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
        ops.push(Operation::new("q", vec![]));

        for m in page_markups {
            draw_markup_pdf_ops(&mut doc, &mut ops, &mut xobjects, m, page_model, page_h);
        }

        ops.push(Operation::new("Q", vec![]));
        let overlay_bytes = Content { operations: ops }.encode().unwrap_or_default();
        let overlay_id = doc.add_object(Stream::new(dictionary! {}, overlay_bytes));

        let existing_contents = doc.get_page_contents(page_id);
        let mut new_contents: Vec<Object> = existing_contents
            .into_iter()
            .map(Object::Reference)
            .collect();
        new_contents.push(Object::Reference(overlay_id));

        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            page_dict.set("Contents", new_contents);
            page_dict.remove(b"Annots");
        }

        let existing_res_id = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|p| p.get(b"Resources").ok())
            .and_then(|r| r.as_reference().ok());

        if let Some(res_id) = existing_res_id {
            if let Ok(res_dict) = doc.get_dictionary_mut(res_id) {
                inject_resources(res_dict, &res, &xobjects);
            }
        } else if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            if let Ok(Object::Dictionary(res_dict)) = page_dict.get_mut(b"Resources") {
                inject_resources(res_dict, &res, &xobjects);
            } else {
                let mut res_dict = Dictionary::new();
                inject_resources(&mut res_dict, &res, &xobjects);
                page_dict.set("Resources", res_dict);
            }
        }
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error al exportar PDF aplanado: {e}"))?;
    Ok(out)
}

pub fn flatten_pdf_file(tab: &DocumentTab, path: &Path) -> Result<(), String> {
    let bytes = flatten_pdf_to_bytes(tab)?;
    std::fs::write(path, bytes).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))
}

fn inject_resources(res_dict: &mut Dictionary, res: &PdfOverlayResources, xobjects: &Dictionary) {
    if let Ok(Object::Dictionary(fonts)) = res_dict.get_mut(b"Font") {
        fonts.set("RB_F1", res.font_reg);
        fonts.set("RB_F2", res.font_bold);
    } else {
        res_dict.set(
            "Font",
            dictionary! { "RB_F1" => res.font_reg, "RB_F2" => res.font_bold },
        );
    }
    if let Ok(Object::Dictionary(gs)) = res_dict.get_mut(b"ExtGState") {
        gs.set("RB_GS1", res.gs_full);
        gs.set("RB_GSF", res.gs_fill);
        gs.set("RB_GSH", res.gs_hi);
    } else {
        res_dict.set(
            "ExtGState",
            dictionary! { "RB_GS1" => res.gs_full, "RB_GSF" => res.gs_fill, "RB_GSH" => res.gs_hi },
        );
    }
    if !xobjects.is_empty() {
        if let Ok(Object::Dictionary(xo)) = res_dict.get_mut(b"XObject") {
            for (k, v) in xobjects.iter() {
                xo.set(k.clone(), v.clone());
            }
        } else {
            res_dict.set("XObject", xobjects.clone());
        }
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
        ops.push(Operation::new(
            "rg",
            vec![
                (col[0] as f64 / 255.0).into(),
                (col[1] as f64 / 255.0).into(),
                (col[2] as f64 / 255.0).into(),
            ],
        ));
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"RB_F2".to_vec()), size.into()],
        ));
        ops.push(Operation::new("Td", vec![x.into(), py(y_top).into()]));
        ops.push(Operation::new(
            "Tj",
            vec![Object::string_literal(to_winansi_bytes(txt))],
        ));
        ops.push(Operation::new("ET", vec![]));
    };

    let emit_arrow_head = |ops: &mut Vec<Operation>, from: Point2, to: Point2, end: LineEnd| {
        if end == LineEnd::None {
            return;
        }
        let d = (to - from).normalize();
        let n = d.perp();
        let sz = 8.0;
        let back = to - d * sz;
        let left = back + n * (sz * 0.45);
        let right = back - n * (sz * 0.45);
        ops.push(Operation::new("m", vec![left.x.into(), py(left.y).into()]));
        ops.push(Operation::new("l", vec![to.x.into(), py(to.y).into()]));
        ops.push(Operation::new("l", vec![right.x.into(), py(right.y).into()]));
        ops.push(Operation::new("S", vec![]));
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
            let anchor = box_rect_anchor(*box_rect, *knee);
            emit_poly(ops, &[*arrow_tip, *knee, anchor], false, None);
            emit_arrow_head(ops, *knee, *arrow_tip, LineEnd::ClosedArrow);
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
            emit_poly(ops, points, true, m.style.fill_color);
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
            emit_poly(ops, &rect.expand(-4.0).corners(), true, None);
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
        MarkupKind::Image { rect, png_b64, .. } => {
            if let Some(img_id) = embed_image_xobject(doc, png_b64) {
                let name = format!("RB_IM_{}", xobjects.len());
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

    if let Some(caption) = metrics.primary_display {
        let anchor = m.anchor_point();
        emit_text(ops, anchor.x + 4.0, anchor.y - 4.0, 10.0, sc, &caption);
    }

    ops.push(Operation::new("Q", vec![]));
}
