use hayro::RenderSettings;
use redbim::geometry::math::{Point2, Rect2};
use redbim::model::settings::CsvDelimiter;
use redbim::model::units::PT_PER_MM;
use redbim::pdf::demo_blueprint::{create_blank_pdf_bytes, generate_demo_chile_pdf};
use redbim::pdf::export::export_markups_csv;
use redbim::pdf::flatten::flatten_pdf_to_bytes;
use redbim::pdf::io::{
    delete_page_in_doc, insert_blank_page_in_doc, open_demo_chile_document, open_pdf_from_bytes,
    rotate_page_in_doc, save_pdf_to_bytes, strip_annots_for_render,
};
use redbim::pdf::renderer::{
    default_interpreter_settings, extract_snap_index, render_page_rgba, render_tile_rgba,
};

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
        .chunks_exact(4)
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
