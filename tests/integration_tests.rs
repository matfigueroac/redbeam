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
            png_b64: b64,
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
