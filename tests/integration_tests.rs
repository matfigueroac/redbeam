use hayro::RenderSettings;
use redbim::geometry::math::{Point2, Rect2};
use redbim::model::document::AlignKind;
use redbim::model::markup::{Markup, MarkupKind, MarkupStyle, MeasurementOptions};
use redbim::model::scale::PageScale;
use redbim::model::settings::CsvDelimiter;
use redbim::model::units::PT_PER_MM;
use redbim::pdf::compare::compare_pdf_pages;
use redbim::pdf::demo_blueprint::{create_blank_pdf_bytes, generate_demo_chile_pdf};
use redbim::pdf::export::export_markups_csv;
use redbim::pdf::flatten::flatten_pdf_to_bytes;
use redbim::pdf::io::{
    combine_pdf_bytes, delete_page_in_doc, extract_pages_to_pdf, insert_blank_page_in_doc,
    open_demo_chile_document, open_pdf_from_bytes, rotate_page_in_doc, save_pdf_to_bytes,
    strip_annots_for_render,
};
use redbim::pdf::renderer::{
    default_interpreter_settings, extract_snap_index, render_page_rgba, render_tile_rgba,
};
use redbim::pdf::{base64_decode, base64_encode};

#[test]
fn demo_chile_pdf_renders_and_extracts_snap_geometry() {
    let bytes = generate_demo_chile_pdf();
    assert!(bytes.len() > 2000);

    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();
    let img = render_page_rgba(&bytes, 0, 900, &interp, &rs).expect("render page 0");
    assert!(img.width > 400 && img.height > 250);
    // Confirm image has non-white blueprint lines
    let non_white = img
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 245 || px[1] < 245 || px[2] < 245)
        .count();
    assert!(non_white > 500, "expected drawn lines, got {non_white}");

    // Confirm high-res tile render works
    let tile = render_tile_rgba(
        &bytes,
        0,
        Rect2::from_xywh(100.0, 200.0, 500.0, 400.0),
        400,
        320,
        &interp,
        &rs,
    )
    .expect("tile render");
    assert_eq!(tile.width, 400);
    assert_eq!(tile.height, 320);

    // Confirm SnapIndex finds exact top-left roof corner (45mm, 110mm)
    let snap = extract_snap_index(&bytes, 0, &interp).expect("snap index");
    assert!(snap.segment_count > 50);
    let corner = Point2::new(45.0 * PT_PER_MM, 110.0 * PT_PER_MM);
    let hit = snap
        .query_nearest(Point2::new(corner.x + 2.0, corner.y - 2.0), 8.0)
        .expect("snap hit near roof corner");
    assert!(hit.point.dist(corner) < 0.5);
}

#[test]
fn round_trip_save_preserves_scales_viewports_and_markups() {
    let mut tab = open_demo_chile_document().expect("open demo");
    assert_eq!(tab.pages.len(), 3);
    assert_eq!(tab.markups.len(), 7);
    assert_eq!(tab.bookmarks.len(), 3);

    // Check 59.50 m measurement accuracy on page 0 (1:100)
    let len_markup = tab
        .markups
        .iter()
        .find(|m| m.subject == "Largo cubierta")
        .unwrap();
    let metrics = len_markup.compute_metrics(&tab.pages[0].scale, &tab.pages[0].viewports);
    assert!((metrics.length_m.unwrap() - 59.5).abs() < 1e-4);
    assert_eq!(metrics.primary_display.as_deref(), Some("59.50 m"));

    // Save PDF with standard /Annots, /VP and /RedBIMState
    let saved_bytes = save_pdf_to_bytes(&tab).expect("save pdf");

    // Reopen and verify full lossless restore
    let reopened = open_pdf_from_bytes("reopened.pdf", None, saved_bytes.clone()).expect("reopen");
    assert_eq!(reopened.markups.len(), tab.markups.len());
    assert!((reopened.pages[0].scale.ratio_x() - 100.0).abs() < 1e-4);
    assert!((reopened.pages[1].scale.ratio_x() - 50.0).abs() < 1e-4);
    assert_eq!(reopened.pages[1].viewports.len(), 1);
    assert_eq!(reopened.pages[1].viewports[0].name, "Detalle A (1:20)");

    // Also confirm strip_annots_for_render succeeds cleanly
    let clean = strip_annots_for_render(&saved_bytes);
    assert!(!clean.is_empty());

    // Flatten and verify hayro renders flattened output
    tab.dirty = false;
    let flattened = flatten_pdf_to_bytes(&tab).expect("flatten");
    let interp = default_interpreter_settings();
    let img = render_page_rgba(&flattened, 0, 800, &interp, &RenderSettings::default())
        .expect("render flattened");
    assert!(img.width > 400);
}

#[test]
fn csv_export_and_page_operations_work() {
    let mut tab = open_demo_chile_document().expect("open demo");
    let csv = export_markups_csv(&tab, CsvDelimiter::Semicolon);
    assert!(csv.starts_with('\u{FEFF}'));
    assert!(csv.contains("Largo cubierta;Eje Norte;1;"));
    assert!(csv.contains("59.50 m"));
    assert!(csv.contains("TOTALES;7 marcas"));

    let blank = create_blank_pdf_bytes(595.28, 841.89, 1);
    let blank_tab = open_pdf_from_bytes("A4.pdf", None, blank).expect("blank A4");
    assert_eq!(blank_tab.pages.len(), 1);

    // Insert, rotate, delete page
    insert_blank_page_in_doc(&mut tab, 1, 841.89, 595.28).expect("insert A4");
    assert_eq!(tab.pages.len(), 4);
    rotate_page_in_doc(&mut tab, 1, 90).expect("rotate");
    assert_eq!(tab.pages[1].rotation_deg, 90);
    delete_page_in_doc(&mut tab, 1).expect("delete");
    assert_eq!(tab.pages.len(), 3);
}

#[test]
fn page_operations_preserve_inherited_rotation_and_prune_unreferenced_content() {
    use lopdf::{Dictionary, Document, Stream};

    let mut doc = Document::load_mem(&create_blank_pdf_bytes(600.0, 400.0, 3)).unwrap();
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(root).unwrap().set("Rotate", 90);
    let mut streams = Vec::new();
    for (index, page_id) in doc.get_pages().into_values().enumerate() {
        let content = format!("q {} 0 m 10 10 l S Q", index + 1).into_bytes();
        let stream = doc.add_object(Stream::new(Dictionary::new(), content));
        let page = doc.get_dictionary_mut(page_id).unwrap();
        page.remove(b"Rotate");
        page.set("Contents", stream);
        streams.push(stream);
    }
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();
    let mut tab = open_pdf_from_bytes("Paginas.pdf", None, raw).unwrap();
    assert_eq!(tab.pages[1].pdf_native_rotation, 90);
    assert!((tab.pages[1].width_pt - 400.0).abs() < 0.01);

    rotate_page_in_doc(&mut tab, 1, 90).unwrap();
    assert_eq!(tab.pages[1].pdf_native_rotation, 180);
    assert!((tab.pages[1].width_pt - 600.0).abs() < 0.01);
    let reopened = open_pdf_from_bytes("Rotada.pdf", None, tab.raw_pdf_bytes.clone()).unwrap();
    assert_eq!(reopened.pages[1].pdf_native_rotation, 180);
    assert_eq!(reopened.pages[1].width_pt, tab.pages[1].width_pt);
    assert_eq!(reopened.pages[0].pdf_native_rotation, 90);

    let before = tab.raw_pdf_bytes.clone();
    let extracted = Document::load_mem(&extract_pages_to_pdf(&tab, &[1]).unwrap()).unwrap();
    assert_eq!(extracted.get_pages().len(), 1);
    assert!(extracted.get_object(streams[1]).is_ok());
    assert!(extracted.get_object(streams[0]).is_err());
    assert!(extracted.get_object(streams[2]).is_err());
    assert!(std::sync::Arc::ptr_eq(&tab.raw_pdf_bytes, &before));

    delete_page_in_doc(&mut tab, 1).unwrap();
    let deleted = Document::load_mem(&tab.raw_pdf_bytes).unwrap();
    assert_eq!(deleted.get_pages().len(), 2);
    assert!(deleted.get_object(streams[0]).is_ok());
    assert!(deleted.get_object(streams[1]).is_err());
    assert!(deleted.get_object(streams[2]).is_ok());
}

#[test]
fn imperial_units_and_cloud_plus_round_trip() {
    use redbim::model::markup::MarkupKind;
    use redbim::model::scale::IMPERIAL_PRESETS;
    use redbim::model::scale::PageScale;
    use redbim::model::units::{LengthUnit, PT_PER_INCH, Precision, UnitSystem};

    // 1/4" = 1'-0" -> 3.0 inches on paper = 12'-0" real
    let p = IMPERIAL_PRESETS
        .iter()
        .find(|x| x.label == "1/4\" = 1'-0\"")
        .unwrap();
    let mut scale = PageScale::from_preset(p);
    scale.precision = Precision::Frac16;
    let d_m = scale.distance_meters(
        Point2::new(0.0, 0.0),
        Point2::new(3.125 * PT_PER_INCH, 0.0),
    );
    // 3.125 in / 0.25 in/ft = 12.5 ft = 12'-6"
    assert_eq!(scale.format_length_val(d_m), "12'-6\"");

    // Switching back to Chilean Metric defaults to meters
    scale.set_unit_system(UnitSystem::Metric);
    assert_eq!(scale.display_unit, LengthUnit::Meter);

    // Confirm CloudPlus from demo survives save+reopen intact
    let tab = open_demo_chile_document().expect("demo");
    let saved = save_pdf_to_bytes(&tab).expect("save");
    let reopened = open_pdf_from_bytes("demo.pdf", None, saved).expect("reopen");
    assert!(reopened.markups.iter().any(|m| matches!(
        &m.kind,
        MarkupKind::CloudPlus { text, .. } if text.contains("1,20 m")
    )));
}

#[test]
fn perimeter_wall_area_and_unit_cost_csv() {
    let blank = create_blank_pdf_bytes(841.0 * PT_PER_MM, 594.0 * PT_PER_MM, 1);
    let mut tab = open_pdf_from_bytes("presupuesto.pdf", None, blank).expect("open blank");
    tab.pages[0].scale = PageScale::from_metric_ratio(100.0); // 10 mm = 1 m

    let ten_m = 100.0 * PT_PER_MM;
    let five_m = 50.0 * PT_PER_MM;
    let mut m = Markup::new(
        0,
        "Tabique Volcometal",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::MeasurePerimeter {
            points: vec![
                Point2::new(100.0, 100.0),
                Point2::new(100.0 + ten_m, 100.0),
                Point2::new(100.0 + ten_m, 100.0 + five_m),
                Point2::new(100.0, 100.0 + five_m),
            ],
            closed: true,
            opts: MeasurementOptions {
                depth_meters: 2.60,
                ..Default::default()
            },
        },
    );
    m.unit_cost = Some(15_000.0);

    let metrics = m.compute_metrics(&tab.pages[0].scale, &tab.pages[0].viewports);
    assert!((metrics.perimeter_m.unwrap() - 30.0).abs() < 1e-4);
    assert!((metrics.wall_area_m2.unwrap() - 78.0).abs() < 1e-4);
    let item_cost = m.total_cost(&tab.pages[0].scale, &tab.pages[0].viewports).unwrap();
    assert!((item_cost - 450_000.0).abs() < 1e-2);

    tab.markups.push(m);
    let totals = tab.compute_totals(Some(0));
    assert!((totals.total_cost - 450_000.0).abs() < 1e-2);

    let csv = export_markups_csv(&tab, CsvDelimiter::Semicolon);
    assert!(csv.contains("Precio Unitario"));
    assert!(csv.contains("Total ($)"));
    assert!(csv.contains("15000"));
    assert!(csv.contains("450000"));
}

#[test]
fn combine_and_extract_pdf_pages_roundtrip() {
    let pdf_a = generate_demo_chile_pdf();
    let pdf_b = create_blank_pdf_bytes(595.28, 841.89, 2);
    let combined = combine_pdf_bytes(&[&pdf_a, &pdf_b]).expect("combine");

    let comb_tab = open_pdf_from_bytes("comb.pdf", None, combined).expect("open combined");
    assert_eq!(comb_tab.pages.len(), 5);

    let extracted = extract_pages_to_pdf(&comb_tab, &[3]).expect("extract single page");
    let ext_tab = open_pdf_from_bytes("ext.pdf", None, extracted).expect("open extracted");
    assert_eq!(ext_tab.pages.len(), 1);

    let interp = default_interpreter_settings();
    let rendered = render_page_rgba(&ext_tab.raw_pdf_bytes, 0, 600, &interp, &RenderSettings::default())
        .expect("render extracted");
    assert!(rendered.width > 100 && rendered.height > 100);
}

#[test]
fn compare_pdf_pages_detects_real_differences() {
    let tab_a = open_demo_chile_document().expect("demo");
    let bytes_a = strip_annots_for_render(&tab_a.raw_pdf_bytes);
    let mut tab_b = open_pdf_from_bytes("rev_b.pdf", None, bytes_a.clone()).expect("rev b");
    tab_b.markups.push(Markup::new(
        0,
        "Ampliación nueva",
        "Matías",
        MarkupStyle {
            stroke_color: [20, 20, 20, 255],
            fill_color: Some([20, 20, 20, 255]),
            fill_opacity: 1.0,
            line_width: 4.0,
            ..Default::default()
        },
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(160.0, 160.0, 180.0, 110.0),
        },
    ));
    let bytes_b = flatten_pdf_to_bytes(&tab_b).expect("flatten rev b");

    let (diff_tab, report) = compare_pdf_pages(&bytes_a, 0, &bytes_b, 0, 900).expect("compare");
    assert!(report.diff_regions_count >= 1);
    assert!(report.added_pixels + report.removed_pixels > 50);
    assert!(
        diff_tab
            .markups
            .iter()
            .any(|m| matches!(m.kind, MarkupKind::Cloud { .. }))
    );

    let interp = default_interpreter_settings();
    let img = render_page_rgba(&diff_tab.raw_pdf_bytes, 0, 700, &interp, &RenderSettings::default())
        .expect("render diff");
    assert!(img.width > 200);
}

#[test]
fn saved_annotations_include_ap_appearance_streams() {
    let tab = open_demo_chile_document().expect("demo");
    let saved = save_pdf_to_bytes(&tab).expect("save");
    let doc = lopdf::Document::load_mem(&saved).expect("lopdf load");
    let pages = doc.get_pages();
    let p0_id = pages[&1];
    let p0_dict = doc.get_object(p0_id).and_then(|o| o.as_dict()).expect("page 0 dict");
    let annots = p0_dict
        .get(b"Annots")
        .ok()
        .and_then(|o| match o {
            lopdf::Object::Array(a) => Some(a),
            lopdf::Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok(),
            _ => None,
        })
        .expect("annots array");
    assert!(!annots.is_empty());

    for aref in annots {
        let aid = aref.as_reference().expect("annot ref");
        let adict = doc.get_object(aid).and_then(|o| o.as_dict()).expect("annot dict");
        let ap_obj = adict.get(b"AP").expect("/AP missing on saved annot");
        let ap_dict = ap_obj.as_dict().expect("/AP dict");
        let n_id = ap_dict.get(b"N").and_then(|o| o.as_reference()).expect("/AP /N stream ref");
        let stream = doc.get_object(n_id).and_then(|o| o.as_stream()).expect("/AP /N stream");
        assert_eq!(
            stream.dict.get(b"Subtype").and_then(|o| o.as_name()).ok(),
            Some(b"Form".as_slice())
        );
    }
}

#[test]
fn base64_png_and_image_markup_flatten_roundtrip() {
    let img_buf = image::RgbaImage::from_fn(8, 8, |_, _| image::Rgba([220, 38, 38, 255]));
    let mut png_bytes = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image::DynamicImage::ImageRgba8(img_buf)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode png");
    }
    let b64 = base64_encode(&png_bytes);
    let decoded = base64_decode(&b64).expect("decode b64");
    assert_eq!(decoded, png_bytes);
    let dec_img = image::load_from_memory(&decoded).expect("load png");
    assert_eq!(dec_img.width(), 8);
    assert_eq!(dec_img.height(), 8);

    let blank = create_blank_pdf_bytes(595.28, 841.89, 1);
    let mut tab = open_pdf_from_bytes("img_doc.pdf", None, blank).expect("open");
    tab.markups.push(Markup::new(
        0,
        "Imagen",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::Image {
            rect: Rect2::from_xywh(100.0, 100.0, 120.0, 120.0),
            png_b64: b64.into(),
            caption: "Red8x8".to_string(),
        },
    ));

    let saved = save_pdf_to_bytes(&tab).expect("save");
    let reopened = open_pdf_from_bytes("re.pdf", None, saved).expect("reopen");
    assert!(reopened.markups.iter().any(|m| matches!(m.kind, MarkupKind::Image { .. })));

    let flattened = flatten_pdf_to_bytes(&tab).expect("flatten image");
    let interp = default_interpreter_settings();
    let rendered = render_page_rgba(&flattened, 0, 600, &interp, &RenderSettings::default())
        .expect("render flattened image");
    let colored = rendered
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 150 && px[1] < 100 && px[2] < 100)
        .count();
    assert!(colored > 100, "expected rendered red image pixels, got {colored}");
}

#[test]
fn history_shares_image_bytes_and_moves_owned_edit_states() {
    use redbim::model::markup::MarkupAttachment;
    use std::sync::Arc;

    let mut tab = open_pdf_from_bytes("historia", None, create_blank_pdf_bytes(100., 100., 1)).unwrap();
    let bytes = redbim::ui::icons::REDBIM_LOGO_PNG;
    let image: Arc<str> = base64_encode(bytes).into();
    let mut mark = Markup::new(0, "Foto", "Test", MarkupStyle::default(), MarkupKind::Image {
        rect: Rect2::from_xywh(1., 1., 12., 12.), png_b64: image.clone(), caption: "Plano".into(),
    });
    let attachment = MarkupAttachment::from_image_bytes("Terreno", bytes).unwrap();
    let photo = attachment.image_b64.clone();
    mark.attachments.push(attachment);
    tab.markups.push(mark);
    tab.push_undo();
    tab.markups[0].comment = "Revisión".into();
    let live_vector = tab.markups.as_ptr();
    let historical = &tab.undo_stack[0].markups[0];
    assert!(matches!(&historical.kind, MarkupKind::Image { png_b64, .. } if Arc::ptr_eq(&image, png_b64)));
    assert!(Arc::ptr_eq(&photo, &historical.attachments[0].image_b64));
    assert!(tab.undo());
    assert_eq!(tab.redo_stack[0].markups.as_ptr(), live_vector);
    assert!(tab.redo());
    assert_eq!(tab.markups.as_ptr(), live_vector);
    let saved = save_pdf_to_bytes(&tab).unwrap();
    let reopened = open_pdf_from_bytes("reabierto", None, saved).unwrap();
    assert_eq!(reopened.markups, tab.markups);
}

#[test]
fn align_selected_updates_positions() {
    let blank = create_blank_pdf_bytes(595.28, 841.89, 1);
    let mut tab = open_pdf_from_bytes("align.pdf", None, blank).expect("open");
    let r1 = Markup::new(
        0,
        "R1",
        "M",
        MarkupStyle::default(),
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(50.0, 100.0, 40.0, 20.0),
        },
    );
    let r2 = Markup::new(
        0,
        "R2",
        "M",
        MarkupStyle::default(),
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(120.0, 200.0, 40.0, 20.0),
        },
    );
    let r3 = Markup::new(
        0,
        "R3",
        "M",
        MarkupStyle::default(),
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(290.0, 300.0, 40.0, 20.0),
        },
    );
    let ids = vec![r1.id, r2.id, r3.id];
    tab.markups = vec![r1, r2, r3];
    tab.selected_ids = ids;

    assert!(tab.align_selected(AlignKind::DistributeH));
    // Centers were 70, 140, 310 -> distributed 70, 190, 310 -> r2 min_x = 170
    if let MarkupKind::Rectangle { rect } = &tab.markups[1].kind {
        assert!((rect.min_x - 170.0).abs() < 1e-6);
    }

    assert!(tab.align_selected(AlignKind::Left));
    for m in &tab.markups {
        if let MarkupKind::Rectangle { rect } = &m.kind {
            assert!((rect.min_x - 50.0).abs() < 1e-6);
        }
    }
}

#[test]
fn async_worker_extracts_search_text_and_renders_priority_page() {
    use redbim::pdf::renderer::{PdfRenderWorker, RenderCommand, RenderResult};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let tab = open_demo_chile_document().expect("demo");
    let doc_id = tab.id;
    let raw = Arc::new(save_pdf_to_bytes(&tab).expect("PDF with annotation appearances"));
    let interp = default_interpreter_settings();
    let settings = RenderSettings::default();
    let clean = render_page_rgba(&strip_annots_for_render(&raw), 0, 500, &interp, &settings).unwrap();
    let annotated = render_page_rgba(&raw, 0, 500, &interp, &settings).unwrap();
    assert!(clean.premul_rgba != annotated.premul_rgba, "fixture must have visible annotations");

    let worker = PdfRenderWorker::spawn(|| {});
    worker.send(RenderCommand::LoadDocument {
        doc_id,
        revision: 7,
        clean_pdf_bytes: Arc::clone(&raw),
    });
    worker.send(RenderCommand::RenderBasePage {
        doc_id,
        page_index: 0,
        max_side_px: 500,
        generation: 1,
    });
    worker.send(RenderCommand::ExtractSearchText {
        doc_id,
        revision: 7,
        raw_pdf_bytes: Arc::clone(&raw),
        detect_scales: true,
    });

    let deadline = Instant::now() + Duration::from_secs(6);
    let mut got_base = false;
    let mut got_search = false;
    while Instant::now() < deadline && (!got_base || !got_search) {
        for r in worker.drain_results() {
            assert_eq!(r.document_revision(), 7);
            match r {
                RenderResult::BasePageReady { image, .. } => {
                    assert!(image.premul_rgba == clean.premul_rgba, "base page drew annotations twice");
                    got_base = true;
                }
                RenderResult::SearchTextReady {
                    text_by_page,
                    scale_detection_source,
                    ..
                } => {
                    assert_eq!(text_by_page.len(), 3);
                    assert!(text_by_page[0].contains("PLANTA GENERAL"));
                    assert!(Arc::ptr_eq(&raw, &scale_detection_source.unwrap()));
                    got_search = true;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(got_base && got_search, "got_base={got_base} got_search={got_search}");
}

#[test]
fn ap_bbox_matches_annot_rect_and_vp_preserves_page_scale_with_detail_viewports() {
    let tab = open_demo_chile_document().expect("demo");
    let saved = save_pdf_to_bytes(&tab).expect("save");
    let mut doc = lopdf::Document::load_mem(&saved).expect("lopdf load");
    let pages = doc.get_pages();

    // Verify /Rect == /AP /N /BBox on page 1 annotations (no non-uniform squash)
    let p0_dict = doc.get_dictionary(pages[&1]).expect("p0 dict");
    let annots = p0_dict.get(b"Annots").and_then(|o| o.as_array()).expect("annots");
    for aref in annots {
        let aid = aref.as_reference().expect("annot ref");
        let adict = doc.get_dictionary(aid).expect("annot dict");
        let rect = adict.get(b"Rect").and_then(|o| o.as_array()).expect("Rect");
        let n_id = adict
            .get(b"AP")
            .and_then(|o| o.as_dict())
            .and_then(|ap| ap.get(b"N"))
            .and_then(|o| o.as_reference())
            .expect("/AP /N");
        let stream = doc.get_object(n_id).and_then(|o| o.as_stream()).expect("AP stream");
        let bbox = stream.dict.get(b"BBox").and_then(|o| o.as_array()).expect("BBox");
        assert_eq!(rect, bbox, "/Rect and /AP /N /BBox must match 1:1 per ISO 32000-1");
    }

    // Verify Page 2 (ARQ-02) preserves BOTH full-page scale AND Detail Viewport in /VP
    let p1_dict = doc.get_dictionary(pages[&2]).expect("p1 dict");
    let vps = p1_dict.get(b"VP").and_then(|o| o.as_array()).expect("/VP array");
    assert_eq!(vps.len(), 2, "expected page-level /VP + detail /VP");

    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root).unwrap().remove(b"RedBIMState");
    let mut standard_pdf = Vec::new();
    doc.save_to(&mut standard_pdf).unwrap();
    let imported = open_pdf_from_bytes("standard.pdf", None, standard_pdf).unwrap();
    assert!((imported.pages[1].scale.ratio_x() - 50.0).abs() < 1e-4);
    assert_eq!(imported.pages[1].viewports.len(), 1);
    assert!((imported.pages[1].viewports[0].scale.ratio_x() - 20.0).abs() < 1e-4);
}

#[test]
fn csv_neutralizes_formula_injection_and_avoids_perimeter_double_counting() {
    let blank = create_blank_pdf_bytes(841.0 * PT_PER_MM, 594.0 * PT_PER_MM, 1);
    let mut tab = open_pdf_from_bytes("sec.pdf", None, blank).expect("open");
    tab.pages[0].scale = PageScale::from_metric_ratio(100.0);

    let mut perim = Markup::new(
        0,
        "=CMD|' /C calc'!A0",
        "@attacker",
        MarkupStyle::default(),
        MarkupKind::MeasurePerimeter {
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(100.0 * PT_PER_MM, 0.0),
                Point2::new(100.0 * PT_PER_MM, 50.0 * PT_PER_MM),
                Point2::new(0.0, 50.0 * PT_PER_MM),
            ],
            closed: true,
            opts: MeasurementOptions::default(),
        },
    );
    perim.comment = "+SUM(A1:A2)".to_string();
    tab.markups.push(perim);

    let totals = tab.compute_totals(Some(0));
    assert!((totals.total_perimeter_m - 30.0).abs() < 1e-4);
    assert!(
        totals.total_length_m.abs() < 1e-6,
        "MeasurePerimeter must not double-count into total_length_m"
    );

    let csv = export_markups_csv(&tab, CsvDelimiter::Semicolon);
    assert!(csv.contains("'=CMD"));
    assert!(csv.contains("'@attacker"));
    assert!(csv.contains("'+SUM(A1:A2)"));
}

#[test]
fn layer_locking_blocks_delete_and_align_without_polluting_undo() {
    let blank = create_blank_pdf_bytes(595.28, 841.89, 1);
    let mut tab = open_pdf_from_bytes("lock.pdf", None, blank).expect("open");
    tab.layers[0].locked = true;

    let r1 = Markup::new(
        0,
        "R1",
        "M",
        MarkupStyle::default(),
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(50.0, 50.0, 40.0, 20.0),
        },
    );
    let r2 = Markup::new(
        0,
        "R2",
        "M",
        MarkupStyle::default(),
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(150.0, 150.0, 40.0, 20.0),
        },
    );
    let ids = vec![r1.id, r2.id];
    tab.markups = vec![r1, r2];
    tab.selected_ids = ids;

    assert!(tab.is_markup_locked(&tab.markups[0]));
    assert!(!tab.align_selected(AlignKind::Left));
    assert_eq!(tab.delete_selected(), 0);
    assert_eq!(tab.markups.len(), 2);
    assert!(tab.undo_stack.is_empty(), "blocked ops must not push no-op undo snapshots");
}

#[test]
fn json_import_remaps_uuids_and_groups_and_atomic_write_replaces_file() {
    use redbim::pdf::export::{export_markups_json_file, import_markups_json_file};
    use redbim::pdf::io::atomic_write_file;

    let mut tab = open_demo_chile_document().expect("demo");
    let orig_count = tab.markups.len();
    let tmp_dir = std::env::temp_dir().join(format!("redbim_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);

    let json_path = tmp_dir.join("markups.json");
    export_markups_json_file(&tab, &json_path).expect("export json");
    let imported = import_markups_json_file(&mut tab, &json_path).expect("import json");
    assert_eq!(imported, orig_count);
    assert_eq!(tab.markups.len(), orig_count * 2);
    let unique_ids: std::collections::HashSet<_> = tab.markups.iter().map(|m| m.id).collect();
    assert_eq!(unique_ids.len(), tab.markups.len(), "imported markups must get fresh UUIDs");

    let target = tmp_dir.join("atomic_test.pdf");
    atomic_write_file(&target, b"first version").expect("write v1");
    atomic_write_file(&target, b"second version").expect("replace v2");
    assert_eq!(std::fs::read(&target).unwrap(), b"second version");

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn spaces_page_labels_custom_columns_and_hyperlinks_roundtrip_and_csv() {
    use redbim::model::custom_columns::{
        CustomColumnDef, CustomColumnKind, CustomColumnValue, evaluate_column_cell,
    };
    use redbim::model::document::SpaceRegion;
    use redbim::model::markup::HyperlinkTarget;

    let blank = create_blank_pdf_bytes(800.0, 600.0, 2);
    let mut tab = open_pdf_from_bytes("p0_test.pdf", None, blank).expect("open");

    // 1. Page Labels
    tab.pages[0].label = Some("ARQ-01 · Planta".to_string());
    tab.pages[1].label = Some("EST-02 · Cortes".to_string());
    tab.pages[0].scale = PageScale::from_metric_ratio(100.0);

    // 2. Hierarchical Spaces: Piso 1 > Bodega Norte
    let parent = SpaceRegion::new(
        0,
        "Piso 1",
        vec![
            Point2::new(40.0, 40.0),
            Point2::new(700.0, 40.0),
            Point2::new(700.0, 520.0),
            Point2::new(40.0, 520.0),
        ],
    );
    let mut child = SpaceRegion::new(
        0,
        "Bodega Norte",
        vec![
            Point2::new(80.0, 80.0),
            Point2::new(360.0, 80.0),
            Point2::new(360.0, 320.0),
            Point2::new(80.0, 320.0),
        ],
    );
    child.parent_id = Some(parent.id);
    tab.spaces = vec![parent, child];

    // 3. Measurement markup inside child space + custom column values + formula
    let mut area_m = Markup::new(
        0,
        "Radier",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::MeasureArea {
            points: vec![
                Point2::new(100.0, 100.0),
                Point2::new(241.7323, 100.0), // 50 mm on paper = 5.0 m at 1:100
                Point2::new(241.7323, 241.7323),
                Point2::new(100.0, 241.7323),
            ],
            cutouts: Vec::new(),
            opts: MeasurementOptions::default(),
        },
    );
    area_m.unit_cost = Some(10_000.0);
    // Use default starter columns + add custom IVA formula column
    let iva_col = CustomColumnDef::new(
        "Total c/IVA",
        CustomColumnKind::Formula {
            expression: "round(SubtotalcDesp * 1.19, 0)".to_string(),
            decimals: 0,
            prefix: "$ ".to_string(),
            suffix: String::new(),
            include_in_totals: true,
        },
    );
    tab.custom_columns.push(iva_col.clone());
    // Override Desperdicio to 1.10 on this markup
    let desp_col_id = tab
        .custom_columns
        .iter()
        .find(|c| c.name == "Desperdicio")
        .unwrap()
        .id;
    area_m
        .custom_values
        .insert(desp_col_id, CustomColumnValue::Number(1.10));

    // Verify space inheritance chooses deepest child ("Piso 1 > Bodega Norte")
    assert_eq!(
        tab.resolve_space_for_markup(&area_m).as_deref(),
        Some("Piso 1 > Bodega Norte")
    );

    // Verify formula chain: 25 m2 * 10,000 * 1.10 = 275,000; * 1.19 = 327,250
    let metrics = area_m.compute_metrics(&tab.pages[0].scale, &tab.pages[0].viewports);
    let iva_eval = evaluate_column_cell(
        &iva_col,
        &tab.custom_columns,
        &area_m,
        &metrics,
        &tab.pages[0].scale,
        &tab.pages[0].viewports,
    );
    assert!(iva_eval.error.is_none(), "unexpected formula error: {:?}", iva_eval.error);
    let iva_num = iva_eval.numeric.expect("numeric iva result");
    assert!(
        (iva_num - 327_250.0).abs() < 20.0,
        "expected ~327250, got {iva_num}"
    );

    // 4. Hyperlinks: Page jump + External URL
    let page_link = Markup::new(
        0,
        "Hipervínculo",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::Hyperlink {
            rect: Rect2::from_xywh(420.0, 120.0, 120.0, 40.0),
            target: HyperlinkTarget::Page { page_index: 1 },
            description: "Ir a EST-02".to_string(),
        },
    );
    let url_link = Markup::new(
        0,
        "Hipervínculo",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::Hyperlink {
            rect: Rect2::from_xywh(420.0, 180.0, 120.0, 40.0),
            target: HyperlinkTarget::Url("https://www.minvu.gob.cl".to_string()),
            description: "OGUC".to_string(),
        },
    );
    tab.markups = vec![area_m, page_link, url_link];

    // 5. CSV export contains Page Label, Space hierarchy, and Custom Column values
    let csv = export_markups_csv(&tab, CsvDelimiter::Semicolon);
    assert!(csv.contains("Lámina;Espacio;"), "CSV missing Lámina/Espacio headers");
    assert!(csv.contains("ARQ-01 · Planta"), "CSV missing page label");
    assert!(csv.contains("Piso 1 > Bodega Norte"), "CSV missing hierarchical space");
    assert!(csv.contains("Total c/IVA"), "CSV missing custom formula column header");

    // 6. Save to PDF, inspect ISO /Subtype /Link annotations, and reopen full state roundtrip
    let saved = save_pdf_to_bytes(&tab).expect("save P0 doc");
    let doc = lopdf::Document::load_mem(&saved).expect("load saved pdf");
    let p0_id = doc.get_pages()[&1];
    let p0_dict = doc.get_dictionary(p0_id).expect("page 0 dict");
    let annots = p0_dict.get(b"Annots").and_then(|o| o.as_array()).expect("annots");
    let mut found_dest_link = false;
    let mut found_uri_link = false;
    for a in annots {
        if let Ok(aid) = a.as_reference()
            && let Ok(ad) = doc.get_dictionary(aid)
            && ad.get(b"Subtype").and_then(|o| o.as_name()).ok() == Some(b"Link")
        {
            if ad.has(b"Dest") {
                found_dest_link = true;
            }
            if let Ok(act) = ad.get(b"A").and_then(|o| o.as_dict())
                && act.get(b"S").and_then(|o| o.as_name()).ok() == Some(b"URI")
            {
                found_uri_link = true;
            }
        }
    }
    assert!(found_dest_link, "expected ISO /Subtype /Link with /Dest");
    assert!(found_uri_link, "expected ISO /Subtype /Link with /A /URI");

    let reopened = open_pdf_from_bytes("p0_reopen.pdf", None, saved).expect("reopen P0");
    assert_eq!(reopened.pages[0].label.as_deref(), Some("ARQ-01 · Planta"));
    assert_eq!(reopened.pages[1].label.as_deref(), Some("EST-02 · Cortes"));
    assert_eq!(reopened.spaces.len(), 2);
    assert_eq!(reopened.custom_columns.len(), tab.custom_columns.len());
    assert!(reopened
        .markups
        .iter()
        .any(|m| matches!(m.kind, MarkupKind::Hyperlink { target: HyperlinkTarget::Page { page_index: 1 }, .. })));
}

#[test]
fn rotated_and_cropped_pdf_flattens_at_exact_visual_coordinates() {
    use lopdf::{Dictionary, Document, Object, Stream, dictionary};

    // Build a PDF page with MediaBox [0 0 600 800], CropBox [100 50 500 350] (400x300 raw),
    // and /Rotate 90 => visual dimensions are 300 wide x 400 tall!
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 600.0.into(), 800.0.into()],
        "CropBox" => vec![100.0.into(), 50.0.into(), 500.0.into(), 350.0.into()],
        "Rotate" => 90i64,
        "Resources" => Dictionary::new(),
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).expect("save rotated cropped pdf");

    let mut tab = open_pdf_from_bytes("rot_crop.pdf", None, raw).expect("open rot crop");
    assert_eq!(tab.pages[0].pdf_native_rotation, 90);
    assert!((tab.pages[0].width_pt - 300.0).abs() < 1.0);
    assert!((tab.pages[0].height_pt - 400.0).abs() < 1.0);

    // Place a solid red rectangle in the visual TOP-LEFT quadrant: [15..115, 15..115]
    let style = MarkupStyle {
        stroke_color: [220, 20, 20, 255],
        fill_color: Some([220, 20, 20, 255]),
        fill_opacity: 1.0,
        ..MarkupStyle::default()
    };
    tab.markups.push(Markup::new(
        0,
        "TopLeftTest",
        "M",
        style,
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(15.0, 15.0, 100.0, 100.0),
        },
    ));

    let flattened = flatten_pdf_to_bytes(&tab).expect("flatten rotated cropped pdf");
    let interp = default_interpreter_settings();
    let img = render_page_rgba(&flattened, 0, 400, &interp, &RenderSettings::default())
        .expect("render flattened rotated cropped");

    // Count red pixels in visual top-left quadrant vs bottom-right quadrant
    let w = img.width;
    let h = img.height;
    let mut tl_red = 0usize;
    let mut br_red = 0usize;
    for y in 0..h {
        for x in 0..w {
            let off = (y * w + x) * 4;
            let r = img.premul_rgba[off];
            let g = img.premul_rgba[off + 1];
            let b = img.premul_rgba[off + 2];
            if r > 170 && g < 80 && b < 80 {
                if x < w / 2 && y < h / 2 {
                    tl_red += 1;
                } else if x >= w / 2 && y >= h / 2 {
                    br_red += 1;
                }
            }
        }
    }
    assert!(
        tl_red > 500 && br_red == 0,
        "expected red rect exclusively in visual top-left of rotated+cropped page, got tl_red={tl_red}, br_red={br_red}"
    );
}

#[test]
fn save_preserves_existing_widget_form_annotations() {
    use lopdf::{Document, Object, dictionary};

    let blank = create_blank_pdf_bytes(600.0, 800.0, 1);
    let mut doc = Document::load_mem(&blank).expect("load blank");
    let p0_id = doc.get_pages()[&1];
    let widget_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal("ProjectNumber"),
        "Rect" => vec![50.0.into(), 700.0.into(), 200.0.into(), 725.0.into()],
    });
    if let Ok(pdict) = doc.get_dictionary_mut(p0_id) {
        pdict.set("Annots", vec![Object::Reference(widget_id)]);
    }
    let mut with_widget = Vec::new();
    doc.save_to(&mut with_widget).expect("save with widget");

    let mut tab = open_pdf_from_bytes("form.pdf", None, with_widget).expect("open form");
    tab.markups.push(Markup::new(
        0,
        "Nota",
        "M",
        MarkupStyle::default(),
        MarkupKind::Note {
            pos: Point2::new(120.0, 120.0),
        },
    ));
    let saved = save_pdf_to_bytes(&tab).expect("save form with markup");

    let reloaded = Document::load_mem(&saved).expect("reload saved");
    let rp0_id = reloaded.get_pages()[&1];
    let rp0 = reloaded.get_dictionary(rp0_id).expect("page dict");
    let annots = rp0.get(b"Annots").and_then(|o| o.as_array()).expect("annots");
    let has_widget = annots.iter().any(|o| {
        o.as_reference()
            .ok()
            .and_then(|id| reloaded.get_dictionary(id).ok())
            .and_then(|d| d.get(b"Subtype").ok())
            .and_then(|s| s.as_name().ok())
            == Some(b"Widget")
    });
    assert!(has_widget, "existing /Subtype /Widget form field must be preserved on save");
}

#[test]
fn flatten_and_unflatten_roundtrip_strips_overlay_and_restores_markups() {
    use redbim::pdf::flatten::{
        flatten_in_place_in_tab, has_recoverable_flattened_markups, unflatten_pdf_in_tab,
    };

    let blank = create_blank_pdf_bytes(600.0, 400.0, 1);
    let mut tab = open_pdf_from_bytes("unflat.pdf", None, blank).expect("open");
    let style = MarkupStyle {
        stroke_color: [220, 20, 20, 255],
        fill_color: Some([220, 20, 20, 255]),
        fill_opacity: 1.0,
        ..MarkupStyle::default()
    };
    tab.markups.push(Markup::new(
        0,
        "Muro a demoler",
        "Matías",
        style,
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(80.0, 80.0, 140.0, 100.0),
        },
    ));
    let orig_id = tab.markups[0].id;

    // 1. Flatten in-place (recoverable)
    let flat_count = flatten_in_place_in_tab(&mut tab, true).expect("flatten in place");
    assert_eq!(flat_count, 1);
    assert!(tab.markups.is_empty());
    assert!(has_recoverable_flattened_markups(&tab.raw_pdf_bytes));

    // Confirm flattened red pixels appear in the rendered PDF
    let interp = default_interpreter_settings();
    let img_flat = render_page_rgba(&tab.raw_pdf_bytes, 0, 400, &interp, &RenderSettings::default())
        .expect("render flattened");
    let red_flat = img_flat
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 160 && px[1] < 80 && px[2] < 80)
        .count();
    assert!(red_flat > 150, "expected flattened red pixels, got {red_flat}");

    // 2. Simulate closing & reopening the flattened PDF file, then Unflatten
    let mut reopened = open_pdf_from_bytes("unflat_reopen.pdf", None, tab.raw_pdf_bytes.clone())
        .expect("reopen flattened");
    assert!(
        reopened.markups.is_empty(),
        "flattened PDF must not double-load markups until unflattened"
    );
    let restored = unflatten_pdf_in_tab(&mut reopened).expect("unflatten");
    assert_eq!(restored, 1);
    assert_eq!(reopened.markups.len(), 1);
    assert_eq!(reopened.markups[0].id, orig_id);
    assert!(!has_recoverable_flattened_markups(&reopened.raw_pdf_bytes));

    // Confirm the flattened red overlay was stripped from the page content stream!
    let img_clean =
        render_page_rgba(&reopened.raw_pdf_bytes, 0, 400, &interp, &RenderSettings::default())
            .expect("render unflattened");
    let red_clean = img_clean
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 160 && px[1] < 80 && px[2] < 80)
        .count();
    assert_eq!(red_clean, 0, "expected 0 red pixels after stripping flattened overlay");
}

#[test]
fn overlay_pages_auto_aligns_shifted_revision_and_supports_three_point_affine() {
    use redbim::pdf::overlay::{OverlayAlignmentMode, OverlayConfig, overlay_pdf_pages};

    // Build Rev A with a bold dark structural grid rectangle flattened onto the sheet
    let make_rev = |shift_x: f64, shift_y: f64| -> Vec<u8> {
        let blank = create_blank_pdf_bytes(600.0, 400.0, 1);
        let mut t = open_pdf_from_bytes("rev.pdf", None, blank).expect("open");
        let st = MarkupStyle {
            stroke_color: [10, 10, 10, 255],
            line_width: 4.0,
            ..MarkupStyle::default()
        };
        t.markups.push(Markup::new(
            0,
            "Frame",
            "M",
            st.clone(),
            MarkupKind::Rectangle {
                rect: Rect2::from_xywh(120.0 + shift_x, 90.0 + shift_y, 260.0, 180.0),
            },
        ));
        t.markups.push(Markup::new(
            0,
            "Diag",
            "M",
            st,
            MarkupKind::Line {
                start: Point2::new(120.0 + shift_x, 90.0 + shift_y),
                end: Point2::new(380.0 + shift_x, 270.0 + shift_y),
            },
        ));
        flatten_pdf_to_bytes(&t).expect("flatten rev")
    };

    let rev_a = make_rev(0.0, 0.0);
    let rev_b = make_rev(24.0, 16.0);

    // 1. Auto-Align should detect the (24, 16) pt shift and achieve high ink overlap
    let cfg_auto = OverlayConfig {
        alignment: OverlayAlignmentMode::AutoTranslate,
        max_side_px: 600,
        ..OverlayConfig::default()
    };
    let (ov_tab, rep_auto) = overlay_pdf_pages(&rev_a, &rev_b, &cfg_auto).expect("auto overlay");
    assert_eq!(rep_auto.pages_overlaid, 1);
    assert_eq!(ov_tab.pages.len(), 1);
    assert!(
        (rep_auto.applied_shift_pt.0 - 24.0).abs() <= 4.0
            && (rep_auto.applied_shift_pt.1 - 16.0).abs() <= 4.0,
        "expected shift near (24, 16) pt, got {:?}",
        rep_auto.applied_shift_pt
    );
    assert!(
        rep_auto.overlap_ratio > 0.75,
        "expected >75% ink overlap after auto-align, got {:.2}",
        rep_auto.overlap_ratio
    );

    // 2. Exact 3-Point Affine alignment should recover near-100% overlap
    let cfg_3pt = OverlayConfig {
        alignment: OverlayAlignmentMode::ThreePoint {
            pts_a: [
                Point2::new(120.0, 90.0),
                Point2::new(380.0, 90.0),
                Point2::new(120.0, 270.0),
            ],
            pts_b: [
                Point2::new(144.0, 106.0),
                Point2::new(404.0, 106.0),
                Point2::new(144.0, 286.0),
            ],
        },
        max_side_px: 600,
        ..OverlayConfig::default()
    };
    let (_ov3, rep_3pt) = overlay_pdf_pages(&rev_a, &rev_b, &cfg_3pt).expect("3pt overlay");
    assert!(
        rep_3pt.overlap_ratio > 0.90,
        "expected >90% overlap after exact 3-point affine align, got {:.2}",
        rep_3pt.overlap_ratio
    );
}

#[test]
fn replace_page_content_preserves_markups_scale_spaces_and_labels() {
    use redbim::pdf::io::replace_page_content_in_doc;

    let mut tab = open_demo_chile_document().expect("demo");
    let orig_markups = tab.markups.len();
    let orig_spaces = tab.spaces.len();
    let orig_label = tab.pages[0].label.clone();
    let orig_ratio = tab.pages[0].scale.ratio_x();

    let replacement_pdf = create_blank_pdf_bytes(1000.0, 700.0, 1);
    replace_page_content_in_doc(&mut tab, 0, &replacement_pdf, 0).expect("replace page content");

    assert_eq!(tab.markups.len(), orig_markups);
    assert_eq!(tab.spaces.len(), orig_spaces);
    assert_eq!(tab.pages[0].label, orig_label);
    assert!((tab.pages[0].scale.ratio_x() - orig_ratio).abs() < 1e-6);
    assert!((tab.pages[0].width_pt - 1000.0).abs() < 1.0);
    assert!((tab.pages[0].height_pt - 700.0).abs() < 1.0);
}

#[test]
fn headers_footers_apply_replace_and_remove_cleanly() {
    use redbim::pdf::headers_footers::{
        HeaderFooterConfig, apply_headers_footers_in_tab, remove_headers_footers_in_tab,
    };

    let blank = create_blank_pdf_bytes(600.0, 400.0, 2);
    let mut tab = open_pdf_from_bytes("hf_doc.pdf", None, blank).expect("open");
    tab.pages[0].label = Some("ARQ-01".to_string());
    tab.pages[1].label = Some("ARQ-02".to_string());

    let cfg = HeaderFooterConfig {
        top_left: "PROYECTO CHILE".to_string(),
        bottom_center: "<<Label>>".to_string(),
        bottom_right: "Pag <<Page>>/<<TotalPages>>".to_string(),
        font_size_pt: 12.0,
        margin_mm: 8.0,
        color: [20, 20, 20],
        ..HeaderFooterConfig::default()
    };

    let applied = apply_headers_footers_in_tab(&mut tab, &cfg).expect("apply hf");
    assert_eq!(applied, 2);

    let interp = default_interpreter_settings();
    let img_with_hf = render_page_rgba(&tab.raw_pdf_bytes, 0, 600, &interp, &RenderSettings::default())
        .expect("render hf");
    let dark_with_hf = img_with_hf
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 120 && px[1] < 120 && px[2] < 120)
        .count();
    assert!(dark_with_hf > 30, "expected rendered header/footer text pixels, got {dark_with_hf}");

    // Re-applying replaces instead of stacking
    let applied2 = apply_headers_footers_in_tab(&mut tab, &cfg).expect("reapply hf");
    assert_eq!(applied2, 2);

    // Removing strips all RedBIM header/footer overlays completely
    let removed = remove_headers_footers_in_tab(&mut tab).expect("remove hf");
    assert_eq!(removed, 2);
    let img_clean = render_page_rgba(&tab.raw_pdf_bytes, 0, 600, &interp, &RenderSettings::default())
        .expect("render clean");
    let dark_clean = img_clean
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 120 && px[1] < 120 && px[2] < 120)
        .count();
    assert_eq!(dark_clean, 0, "expected 0 dark pixels after removing headers/footers");
}

#[test]
fn markup_capture_photo_attachment_roundtrip_and_csv() {
    use redbim::model::markup::MarkupAttachment;

    let img_buf = image::RgbaImage::from_fn(24, 16, |_, _| image::Rgba([40, 120, 220, 255]));
    let mut png_bytes = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image::DynamicImage::ImageRgba8(img_buf)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode png");
    }
    let att = MarkupAttachment::from_image_bytes("Fisura losa eje B", &png_bytes).expect("attach");
    assert!(!att.image_b64.is_empty());

    let blank = create_blank_pdf_bytes(600.0, 400.0, 1);
    let mut tab = open_pdf_from_bytes("capture.pdf", None, blank).expect("open");
    let mut m = Markup::new(
        0,
        "Observación terreno",
        "Matías",
        MarkupStyle::default(),
        MarkupKind::Note {
            pos: Point2::new(100.0, 100.0),
        },
    );
    m.attachments.push(att.clone());
    tab.markups.push(m);

    let csv = export_markups_csv(&tab, CsvDelimiter::Semicolon);
    assert!(csv.contains(";Fotos;"));

    let saved = save_pdf_to_bytes(&tab).expect("save with photo");
    let reopened = open_pdf_from_bytes("capture_re.pdf", None, saved).expect("reopen photo");
    assert_eq!(reopened.markups[0].attachments.len(), 1);
    assert_eq!(reopened.markups[0].attachments[0].caption, "Fisura losa eje B");
}

#[test]
fn tool_chest_sequences_and_toolset_export_import_roundtrip() {
    use redbim::model::tool_chest::{ToolChest, ToolSet};

    let mut chest = ToolChest::default_chilean_sets();
    let seq_set = chest
        .sets
        .iter()
        .find(|s| s.title.contains("Secuencias"))
        .expect("starter sequence set")
        .clone();
    let rdi_id = seq_set.items[0].id;
    assert_eq!(chest.advance_item_sequence(rdi_id).as_deref(), Some("RDI-001"));
    assert_eq!(chest.advance_item_sequence(rdi_id).as_deref(), Some("RDI-002"));

    let tmp_dir = std::env::temp_dir().join(format!("redbim_ts_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let ts_path = tmp_dir.join("secuencias.redbim-tools.json");
    seq_set.export_to_file(&ts_path).expect("export toolset");
    let imported = ToolSet::import_from_file(&ts_path).expect("import toolset");
    assert_eq!(imported.title, seq_set.title);
    assert_eq!(imported.items.len(), seq_set.items.len());
    assert_ne!(imported.id, seq_set.id, "imported set must receive a fresh UUID");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn batch_job_flattens_stamps_headers_and_exports_consolidated_csv() {
    use redbim::pdf::batch::{BatchJobConfig, BatchStampPlacement, run_batch_job};
    use redbim::pdf::flatten::has_recoverable_flattened_markups;
    use redbim::pdf::headers_footers::HeaderFooterConfig;

    let tmp_dir = std::env::temp_dir().join(format!("redbim_batch_test_{}", std::process::id()));
    let in_dir = tmp_dir.join("in");
    let out_dir = tmp_dir.join("out");
    let _ = std::fs::create_dir_all(&in_dir);

    let p1 = in_dir.join("Lamina_01.pdf");
    let p2 = in_dir.join("Lamina_02.pdf");
    let demo = open_demo_chile_document().expect("demo");
    let demo_bytes = save_pdf_to_bytes(&demo).expect("save demo");
    std::fs::write(&p1, &demo_bytes).expect("write p1");
    std::fs::write(&p2, &demo_bytes).expect("write p2");

    let cfg = BatchJobConfig {
        input_files: vec![p1, p2],
        output_dir: out_dir.clone(),
        flatten_markups: true,
        flatten_recoverable: true,
        apply_headers_footers: true,
        headers_footers: HeaderFooterConfig::default(),
        apply_stamp: true,
        stamp_title: "EMITIDO".to_string(),
        stamp_subtitle: "Lote obra".to_string(),
        stamp_color: [37, 99, 235],
        stamp_placement: BatchStampPlacement::TopRight,
        author_name: "Matías".to_string(),
        export_consolidated_csv: true,
        csv_delimiter: CsvDelimiter::Semicolon,
    };

    let rep = run_batch_job(&cfg).expect("batch run");
    assert_eq!(rep.processed_files, 2);
    assert!(rep.failed_files.is_empty());
    assert_eq!(rep.total_pages_processed, 6);
    assert!(rep.total_markups_exported >= 20);

    let out_pdf1 = std::fs::read(out_dir.join("Lamina_01.pdf")).expect("read out pdf 1");
    assert!(has_recoverable_flattened_markups(&out_pdf1));

    let csv_path = rep.consolidated_csv_path.expect("csv path");
    let csv_text = std::fs::read_to_string(&csv_path).expect("read consolidated csv");
    assert!(csv_text.contains("Documento;Tipo;Asunto;"));
    assert!(csv_text.contains("Lamina_01.pdf;"));
    assert!(csv_text.contains("Lamina_02.pdf;"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn destructive_redaction_removes_pixels_and_clears_recovery_metadata() {
    use redbim::pdf::flatten::{flatten_in_place_in_tab, has_recoverable_flattened_markups};
    use redbim::pdf::redact::apply_redactions_in_tab;

    let blank = create_blank_pdf_bytes(600.0, 400.0, 2);
    let mut tab = open_pdf_from_bytes("secret.pdf", None, blank).expect("open");

    // Bake bright red sensitive content onto page 0
    let st = MarkupStyle {
        stroke_color: [230, 20, 20, 255],
        fill_color: Some([230, 20, 20, 255]),
        fill_opacity: 1.0,
        line_width: 4.0,
        ..MarkupStyle::default()
    };
    tab.markups.push(Markup::new(
        0,
        "Dato confidencial",
        "M",
        st,
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(120.0, 120.0, 140.0, 60.0),
        },
    ));
    flatten_in_place_in_tab(&mut tab, true).expect("flatten sensitive box");
    assert!(has_recoverable_flattened_markups(&tab.raw_pdf_bytes));

    // Mark a slightly larger redaction box covering the sensitive content
    tab.markups.push(Markup::new(
        0,
        "Redacción",
        "M",
        MarkupStyle::default(),
        MarkupKind::Redaction {
            rect: Rect2::from_xywh(100.0, 100.0, 180.0, 100.0),
            overlay_text: String::new(),
        },
    ));
    let rep = apply_redactions_in_tab(&mut tab, [0, 0, 0]).expect("apply redactions");
    assert_eq!(rep.redactions_applied, 1);
    assert_eq!(rep.pages_sanitized, 1);
    assert!(tab.markups.is_empty());
    assert!(
        !has_recoverable_flattened_markups(&tab.raw_pdf_bytes),
        "destructive redaction must strip /RedBIMFlattened recovery metadata"
    );

    // Render sanitized page: zero red pixels must survive and black redaction box must be present
    let interp = default_interpreter_settings();
    let img = render_page_rgba(&tab.raw_pdf_bytes, 0, 600, &interp, &RenderSettings::default())
        .expect("render redacted");
    let surviving_red = img
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] > 160 && px[1] < 80 && px[2] < 80)
        .count();
    let black_box = img
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 15 && px[1] < 15 && px[2] < 15)
        .count();
    assert_eq!(surviving_red, 0, "sensitive red pixels must be completely destroyed");
    assert!(black_box > 1000, "expected solid black redaction box, got {black_box}");
}

#[test]
fn color_processing_grayscale_bw_and_invert_preserve_live_markups() {
    use redbim::pdf::color_process::{
        ColorProcessConfig, ColorProcessMode, apply_color_process_in_tab,
    };
    use redbim::pdf::flatten::flatten_in_place_in_tab;

    let blank = create_blank_pdf_bytes(600.0, 400.0, 1);
    let mut tab = open_pdf_from_bytes("color.pdf", None, blank).expect("open");
    let st = MarkupStyle {
        stroke_color: [20, 80, 235, 255],
        fill_color: Some([20, 80, 235, 255]),
        fill_opacity: 1.0,
        line_width: 4.0,
        ..MarkupStyle::default()
    };
    tab.markups.push(Markup::new(
        0,
        "Azul",
        "M",
        st,
        MarkupKind::Rectangle {
            rect: Rect2::from_xywh(100.0, 100.0, 180.0, 100.0),
        },
    ));
    flatten_in_place_in_tab(&mut tab, false).expect("flatten blue");

    // Add a live markup that MUST survive color processing untouched
    tab.markups.push(Markup::new(
        0,
        "Nota viva",
        "M",
        MarkupStyle::default(),
        MarkupKind::Note {
            pos: Point2::new(50.0, 50.0),
        },
    ));

    let n = apply_color_process_in_tab(
        &mut tab,
        &ColorProcessConfig {
            mode: ColorProcessMode::Grayscale,
            page_range: "1".to_string(),
            max_side_px: 800,
        },
    )
    .expect("grayscale");
    assert_eq!(n, 1);
    assert_eq!(tab.markups.len(), 1, "live markups must survive color processing");

    let interp = default_interpreter_settings();
    let img_gray = render_page_rgba(&tab.raw_pdf_bytes, 0, 400, &interp, &RenderSettings::default())
        .expect("render gray");
    let chromatic = img_gray
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0].abs_diff(px[2]) > 6)
        .count();
    assert_eq!(chromatic, 0, "grayscale sheet must contain zero chromatic pixels");

    // Invert should turn white background pixels dark
    apply_color_process_in_tab(
        &mut tab,
        &ColorProcessConfig {
            mode: ColorProcessMode::Invert,
            page_range: "1".to_string(),
            max_side_px: 800,
        },
    )
    .expect("invert");
    let img_inv = render_page_rgba(&tab.raw_pdf_bytes, 0, 400, &interp, &RenderSettings::default())
        .expect("render inverted");
    let dark_bg = img_inv
        .premul_rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[0] < 20 && px[1] < 20 && px[2] < 20)
        .count();
    assert!(dark_bg > 5000, "inverted white background must become dark");
}

#[test]
fn reduce_pdf_file_size_compacts_large_embedded_raster_images() {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};
    use redbim::pdf::optimize::{OptimizePreset, reduce_pdf_file_size_in_tab};

    // Build a PDF with an uncompressed 1600x1000 noisy-gradient RGB image (~4.8 MB raw)
    let w = 1600usize;
    let h = 1000usize;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            rgb.push(((x * 255) / w) as u8);
            rgb.push(((y * 255) / h) as u8);
            rgb.push((((x + y) * 127) / (w + h)) as u8);
        }
    }
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let img_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => (w as i64),
            "Height" => (h as i64),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8i64,
        },
        rgb,
    ));
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![600.0.into(), 0.0.into(), 0.0.into(), 400.0.into(), 0.0.into(), 0.0.into()],
        ),
        Operation::new("Do", vec![Object::Name(b"BIG0".to_vec())]),
        Operation::new("Q", vec![]),
    ];
    let cid = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().unwrap(),
    ));
    let pid = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.0.into(), 0.0.into(), 600.0.into(), 400.0.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "BIG0" => img_id } },
        "Contents" => cid,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![pid.into()], "Count" => 1i64 }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).expect("save bloated pdf");

    let mut tab = open_pdf_from_bytes("bloated.pdf", None, raw).expect("open bloated");
    let rep = reduce_pdf_file_size_in_tab(&mut tab, OptimizePreset::MaxCompression)
        .expect("optimize");
    assert!(rep.images_reencoded >= 1);
    assert!(
        rep.saved_percent() > 80.0,
        "expected >80% reduction on raw RGB image PDF, got {:.1}% ({} -> {})",
        rep.saved_percent(),
        rep.bytes_before,
        rep.bytes_after
    );

    // Ensure optimized PDF still renders cleanly via hayro
    let interp = default_interpreter_settings();
    let rendered = render_page_rgba(&tab.raw_pdf_bytes, 0, 400, &interp, &RenderSettings::default())
        .expect("render optimized pdf");
    assert!(rendered.width > 100 && rendered.height > 100);
}
