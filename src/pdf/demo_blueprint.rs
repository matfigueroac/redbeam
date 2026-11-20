use crate::geometry::math::{Point2, Rect2};
use crate::model::markup::{
    HatchPattern, LineEnd, LineStyle, Markup, MarkupKind, MarkupStatus, MarkupStyle,
    MeasurementOptions, NoteIcon,
};
use crate::model::units::PT_PER_MM;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};

/// Converts UTF-8 string into WinAnsiEncoding bytes so Spanish accents and `°`/`²`/`³`
/// render crisply in PDF Type1 standard fonts (`/WinAnsiEncoding`).
pub fn to_winansi_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let b = match ch {
            '\u{0020}'..='\u{007E}' => ch as u8,
            '\u{00A0}'..='\u{00FF}' => ch as u8,
            '€' => 0x80,
            '–' | '—' => b'-',
            '‘' | '’' => b'\'',
            '“' | '”' => b'"',
            '•' => 0x95,
            _ => b'?',
        };
        out.push(b);
    }
    out
}

fn pdf_text(s: &str) -> Object {
    Object::string_literal(to_winansi_bytes(s))
}

struct PdfDraw<'a> {
    ops: &'a mut Vec<Operation>,
    page_h: f64,
}

impl<'a> PdfDraw<'a> {
    #[inline]
    fn y(&self, top_y: f64) -> f64 {
        self.page_h - top_y
    }

    fn stroke_color(&mut self, r: f64, g: f64, b: f64) {
        self.ops
            .push(Operation::new("RG", vec![r.into(), g.into(), b.into()]));
    }

    fn fill_color(&mut self, r: f64, g: f64, b: f64) {
        self.ops
            .push(Operation::new("rg", vec![r.into(), g.into(), b.into()]));
    }

    fn line_width(&mut self, w: f64) {
        self.ops.push(Operation::new("w", vec![w.into()]));
    }

    fn line(&mut self, x1: f64, y1_top: f64, x2: f64, y2_top: f64) {
        self.ops
            .push(Operation::new("m", vec![x1.into(), self.y(y1_top).into()]));
        self.ops
            .push(Operation::new("l", vec![x2.into(), self.y(y2_top).into()]));
        self.ops.push(Operation::new("S", vec![]));
    }

    fn rect_stroke(&mut self, x: f64, y_top: f64, w: f64, h: f64) {
        self.ops.push(Operation::new(
            "re",
            vec![x.into(), self.y(y_top + h).into(), w.into(), h.into()],
        ));
        self.ops.push(Operation::new("S", vec![]));
    }

    fn rect_fill(&mut self, x: f64, y_top: f64, w: f64, h: f64) {
        self.ops.push(Operation::new(
            "re",
            vec![x.into(), self.y(y_top + h).into(), w.into(), h.into()],
        ));
        self.ops.push(Operation::new("f", vec![]));
    }

    fn polyline(&mut self, pts: &[(f64, f64)], closed: bool) {
        if let Some(&(x0, y0)) = pts.first() {
            self.ops
                .push(Operation::new("m", vec![x0.into(), self.y(y0).into()]));
            for &(x, y) in &pts[1..] {
                self.ops
                    .push(Operation::new("l", vec![x.into(), self.y(y).into()]));
            }
            if closed {
                self.ops.push(Operation::new("s", vec![]));
            } else {
                self.ops.push(Operation::new("S", vec![]));
            }
        }
    }

    fn circle(&mut self, cx: f64, cy_top: f64, r: f64) {
        let mut pts = Vec::with_capacity(24);
        for i in 0..24 {
            let a = (i as f64 / 24.0) * std::f64::consts::TAU;
            pts.push((cx + r * a.cos(), cy_top + r * a.sin()));
        }
        self.polyline(&pts, true);
    }

    fn text(&mut self, font: &str, size: f64, x: f64, y_top: f64, s: &str) {
        self.ops.push(Operation::new("BT", vec![]));
        self.ops
            .push(Operation::new("Tf", vec![Object::Name(font.as_bytes().to_vec()), size.into()]));
        self.ops
            .push(Operation::new("Td", vec![x.into(), self.y(y_top).into()]));
        self.ops.push(Operation::new("Tj", vec![pdf_text(s)]));
        self.ops.push(Operation::new("ET", vec![]));
    }
}

fn add_standard_fonts(doc: &mut Document) -> Dictionary {
    let f1 = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let f2 = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    dictionary! {
        "F1" => f1,
        "F2" => f2,
    }
}

/// Creates a blank multi-page PDF document with the given page dimensions in points.
pub fn create_blank_pdf_bytes(width_pt: f64, height_pt: f64, page_count: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_dict = add_standard_fonts(&mut doc);
    let resources_id = doc.add_object(dictionary! {
        "Font" => font_dict,
    });

    let mut kids = Vec::new();
    for _ in 0..page_count.max(1) {
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), width_pt.into(), height_pt.into()],
            "Resources" => resources_id,
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => (kids.len() as i64),
            "Kids" => kids,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut out = Vec::new();
    let _ = doc.save_to(&mut out);
    out
}

/// Generates a 3-sheet Chilean A1 blueprint PDF (`841 x 594 mm`) inspired by PV & structural plans:
/// - Page 1 (`PV-01`): Planta General Cubierta Fotovoltaica, exact 1:100 scale (`10 mm = 1 m`)
/// - Page 2 (`ARQ-02`): Planta Arquitectura Estructural, exact 1:50 scale + 1:20 detail box
/// - Page 3 (`ELE-03`): Detalles de Montaje y Canalizaciones, exact 1:25 scale
pub fn generate_demo_chile_pdf() -> Vec<u8> {
    let w_pt = 841.0 * PT_PER_MM; // 2383.937 pt
    let h_pt = 594.0 * PT_PER_MM; // 1683.780 pt

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_dict = add_standard_fonts(&mut doc);
    let resources_id = doc.add_object(dictionary! {
        "Font" => font_dict,
    });

    let page1_ops = build_page1_pv(w_pt, h_pt);
    let page2_ops = build_page2_arq(w_pt, h_pt);
    let page3_ops = build_page3_ele(w_pt, h_pt);

    let mut kids = Vec::new();
    let mut page_ids = Vec::new();
    for ops in [page1_ops, page2_ops, page3_ops] {
        let bytes = Content { operations: ops }.encode().unwrap_or_default();
        let content_id = doc.add_object(Stream::new(dictionary! {}, bytes));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.0.into(), 0.0.into(), w_pt.into(), h_pt.into()],
            "Resources" => resources_id,
            "Contents" => content_id,
        });
        page_ids.push(page_id);
        kids.push(page_id.into());
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => (kids.len() as i64),
            "Kids" => kids,
        }),
    );

    // Add PDF Outlines / Bookmarks for all 3 Chilean sheets
    let o1 = doc.new_object_id();
    let o2 = doc.new_object_id();
    let o3 = doc.new_object_id();
    let outlines_id = doc.new_object_id();

    doc.objects.insert(
        o1,
        Object::Dictionary(dictionary! {
            "Title" => pdf_text("PV-01 Planta General Fotovoltaica (1:100)"),
            "Parent" => outlines_id,
            "Next" => o2,
            "Dest" => vec![page_ids[0].into(), Object::Name(b"Fit".to_vec())],
        }),
    );
    doc.objects.insert(
        o2,
        Object::Dictionary(dictionary! {
            "Title" => pdf_text("ARQ-02 Planta Arquitectura Estructural (1:50)"),
            "Parent" => outlines_id,
            "Prev" => o1,
            "Next" => o3,
            "Dest" => vec![page_ids[1].into(), Object::Name(b"Fit".to_vec())],
        }),
    );
    doc.objects.insert(
        o3,
        Object::Dictionary(dictionary! {
            "Title" => pdf_text("ELE-03 Detalles de Montaje (1:25)"),
            "Parent" => outlines_id,
            "Prev" => o2,
            "Dest" => vec![page_ids[2].into(), Object::Name(b"Fit".to_vec())],
        }),
    );
    doc.objects.insert(
        outlines_id,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => o1,
            "Last" => o3,
            "Count" => 3i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Outlines" => outlines_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut out = Vec::new();
    let _ = doc.save_to(&mut out);
    out
}

fn draw_title_block(d: &mut PdfDraw<'_>, w_pt: f64, h_pt: f64, code: &str, title: &str, scale_label: &str) {
    let m = 15.0 * PT_PER_MM;
    let frame_w = w_pt - 2.0 * m;
    let frame_h = h_pt - 2.0 * m;

    // White sheet background ensures crisp rendering
    d.fill_color(1.0, 1.0, 1.0);
    d.rect_fill(0.0, 0.0, w_pt, h_pt);

    // Outer double border
    d.stroke_color(0.12, 0.14, 0.18);
    d.line_width(1.6);
    d.rect_stroke(m, m, frame_w, frame_h);
    d.line_width(0.6);
    d.rect_stroke(m + 4.0, m + 4.0, frame_w - 8.0, frame_h - 8.0);

    // Bottom Chilean title block (viñeta técnica)
    let tb_h = 34.0 * PT_PER_MM;
    let tb_y = h_pt - m - tb_h;
    d.line_width(1.2);
    d.line(m, tb_y, w_pt - m, tb_y);

    let cols = [0.22, 0.45, 0.68, 0.86];
    for frac in cols {
        let cx = m + frame_w * frac;
        d.line_width(0.8);
        d.line(cx, tb_y, cx, h_pt - m);
    }
    // Horizontal divider in last column
    let last_col_x = m + frame_w * 0.86;
    d.line(last_col_x, tb_y + tb_h * 0.5, w_pt - m, tb_y + tb_h * 0.5);

    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 10.0, m + 14.0, tb_y + 18.0, "PROYECTO DEMO CHILE - CUBIERTA FV");
    d.text("F1", 8.5, m + 14.0, tb_y + 34.0, "Ubicación: Buin / Región Metropolitana, Chile");
    d.text("F1", 8.5, m + 14.0, tb_y + 49.0, "Normativa: NCh / SEC RIC N° 18 - Sistema Métrico SI");
    d.text("F1", 8.5, m + 14.0, tb_y + 64.0, "Coordenadas: WGS84 -33.87324 / -70.74076");

    let c1 = m + frame_w * 0.22 + 12.0;
    d.text("F2", 9.5, c1, tb_y + 18.0, "NOTAS GENERALES DE OBRA");
    d.text("F1", 8.0, c1, tb_y + 34.0, "1. Cotas expresadas en metros (m), salvo indicación contraria.");
    d.text("F1", 8.0, c1, tb_y + 48.0, "2. Verificar dimensiones y niveles en terreno antes de fabricar.");
    d.text("F1", 8.0, c1, tb_y + 62.0, "3. Todas las fijaciones a estructura deberán contar con sello estanco.");

    let c2 = m + frame_w * 0.45 + 12.0;
    d.text("F2", 9.5, c2, tb_y + 18.0, "CUADRO DE REVISIONES");
    d.text("F1", 8.0, c2, tb_y + 34.0, "REV A  |  Emisión para revisión interna  |  2026-10-12");
    d.text("F1", 8.0, c2, tb_y + 48.0, "REV 0  |  Emisión coordinación BIM       |  2026-11-20");

    let c3 = m + frame_w * 0.68 + 12.0;
    d.text("F2", 9.5, c3, tb_y + 18.0, "PLANO");
    d.text("F2", 11.5, c3, tb_y + 38.0, title);
    d.text("F1", 8.5, c3, tb_y + 58.0, "Especialidad: Arquitectura / Energía Solar");

    let c4 = last_col_x + 12.0;
    d.text("F1", 8.0, c4, tb_y + 16.0, "LÁMINA");
    d.text("F2", 16.0, c4, tb_y + 37.0, code);
    d.text("F1", 8.0, c4, tb_y + tb_h * 0.5 + 16.0, "ESCALA");
    d.text("F2", 13.0, c4, tb_y + tb_h * 0.5 + 36.0, scale_label);
}

/// 1:100 scale helper: 1 meter = 10 mm = 10 * PT_PER_MM points.
const PT_PER_M_1_100: f64 = 10.0 * PT_PER_MM;

fn build_page1_pv(w_pt: f64, h_pt: f64) -> Vec<Operation> {
    let mut ops = Vec::new();
    let mut d = PdfDraw {
        ops: &mut ops,
        page_h: h_pt,
    };
    draw_title_block(
        &mut d,
        w_pt,
        h_pt,
        "PV-01",
        "PLANTA GENERAL CUBIERTA FV",
        "1:100",
    );

    let m = 22.0 * PT_PER_MM;
    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 18.0, m + 10.0, m + 28.0, "PLANTA GENERAL - CUBIERTA FOTOVOLTAICA");
    d.text("F1", 10.0, m + 10.0, m + 46.0, "ESCALA 1:100  |  UNIDADES: METROS (m)");

    // Building roof outline at exact 1:100 scale:
    // Top width: 59.50 m, right side: 24.00 m, left side: 15.50 m then stepped notch:
    // +5.00m right, +4.50m down, +7.30m right, +4.00m down, +47.20m right = 59.50 m total!
    let ox = 45.0 * PT_PER_MM;
    let oy = 110.0 * PT_PER_MM;
    let s = PT_PER_M_1_100;

    let roof = vec![
        (ox, oy),
        (ox + 59.50 * s, oy),
        (ox + 59.50 * s, oy + 24.00 * s),
        (ox + 12.30 * s, oy + 24.00 * s),
        (ox + 12.30 * s, oy + 20.00 * s),
        (ox + 5.00 * s, oy + 20.00 * s),
        (ox + 5.00 * s, oy + 15.50 * s),
        (ox, oy + 15.50 * s),
    ];

    d.stroke_color(0.10, 0.12, 0.16);
    d.line_width(1.6);
    d.polyline(&roof, true);

    // Ridge line (cumbrera) dividing F2 north slope and F1 south slope
    d.line_width(0.9);
    d.line(ox, oy + 11.50 * s, ox + 59.50 * s, oy + 11.50 * s);

    // 120 Photovoltaic modules (1.13m x 2.28m scaled visually cleanly inside grid)
    d.stroke_color(0.16, 0.42, 0.72);
    d.line_width(0.65);
    let mod_w = 1.12 * s;
    let mod_h = 2.25 * s;
    // Upper array (F2): 48 + 12 modules
    for i in 0..48 {
        let mx = ox + (2.0 + i as f64 * 1.15) * s;
        let my = oy + 8.60 * s;
        d.rect_stroke(mx, my, mod_w, mod_h);
    }
    for i in 0..12 {
        let mx = ox + (2.0 + i as f64 * 1.15) * s;
        let my = oy + 6.10 * s;
        d.rect_stroke(mx, my, mod_w, mod_h);
    }
    // Lower array (F1): 48 + 12 modules
    for i in 0..48 {
        let mx = ox + (2.0 + i as f64 * 1.15) * s;
        let my = oy + 12.20 * s;
        d.rect_stroke(mx, my, mod_w, mod_h);
    }
    for i in 36..48 {
        let mx = ox + (2.0 + i as f64 * 1.15) * s;
        let my = oy + 14.70 * s;
        d.rect_stroke(mx, my, mod_w, mod_h);
    }

    // Slope arrows F2 / 10° and F1 / 10°
    d.stroke_color(0.08, 0.55, 0.58);
    d.fill_color(0.08, 0.55, 0.58);
    d.line_width(1.0);
    let f2x = ox + 30.0 * s;
    d.line(f2x, oy + 5.2 * s, f2x, oy + 3.0 * s);
    d.line(f2x, oy + 3.0 * s, f2x - 5.0, oy + 3.0 * s + 8.0);
    d.line(f2x, oy + 3.0 * s, f2x + 5.0, oy + 3.0 * s + 8.0);
    d.text("F2", 9.0, f2x + 8.0, oy + 4.4 * s, "F2 / 10°");

    let f1x = ox + 36.0 * s;
    d.line(f1x, oy + 18.0 * s, f1x, oy + 20.4 * s);
    d.line(f1x, oy + 20.4 * s, f1x - 5.0, oy + 20.4 * s - 8.0);
    d.line(f1x, oy + 20.4 * s, f1x + 5.0, oy + 20.4 * s - 8.0);
    d.text("F2", 9.0, f1x + 8.0, oy + 19.6 * s, "F1 / 10°");

    // Dimension lines in meters matching exact geometry
    d.stroke_color(0.25, 0.28, 0.34);
    d.fill_color(0.15, 0.18, 0.22);
    d.line_width(0.6);
    // Top dimension: 59.50 m
    let dim_top = oy - 16.0;
    d.line(ox, oy, ox, dim_top - 4.0);
    d.line(ox + 59.50 * s, oy, ox + 59.50 * s, dim_top - 4.0);
    d.line(ox, dim_top, ox + 59.50 * s, dim_top);
    d.text("F2", 9.0, ox + 28.0 * s, dim_top - 4.0, "59.50 m");

    // Right dimension: 24.00 m
    let dim_right = ox + 59.50 * s + 18.0;
    d.line(ox + 59.50 * s, oy, dim_right + 4.0, oy);
    d.line(ox + 59.50 * s, oy + 24.00 * s, dim_right + 4.0, oy + 24.00 * s);
    d.line(dim_right, oy, dim_right, oy + 24.00 * s);
    d.text("F2", 9.0, dim_right + 6.0, oy + 12.0 * s, "24.00 m");

    // Bottom main span: 47.20 m
    let dim_bot = oy + 24.00 * s + 16.0;
    d.line(ox + 12.30 * s, oy + 24.00 * s, ox + 12.30 * s, dim_bot + 4.0);
    d.line(ox + 59.50 * s, oy + 24.00 * s, ox + 59.50 * s, dim_bot + 4.0);
    d.line(ox + 12.30 * s, dim_bot, ox + 59.50 * s, dim_bot);
    d.text("F2", 9.0, ox + 34.0 * s, dim_bot + 12.0, "47.20 m");

    // Graphic Scale bar: 0 - 5 - 10 m (exact 100 mm = 10 m at 1:100)
    let sb_x = ox;
    let sb_y = oy + 32.0 * s;
    d.stroke_color(0.1, 0.1, 0.1);
    d.line_width(1.2);
    d.line(sb_x, sb_y, sb_x + 10.0 * s, sb_y);
    for tick in [0.0, 5.0, 10.0] {
        let tx = sb_x + tick * s;
        d.line(tx, sb_y - 5.0, tx, sb_y + 5.0);
        d.text("F1", 8.5, tx - 4.0, sb_y + 16.0, &format!("{tick:.0} m"));
    }

    // Right info panels (Cubierta Fotovoltaica, Simbología, Cuadro de Implantación)
    let px = 665.0 * PT_PER_MM;
    let pw = 152.0 * PT_PER_MM;
    d.stroke_color(0.25, 0.28, 0.34);
    d.line_width(0.9);
    d.rect_stroke(px, m + 12.0, pw, 160.0);
    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 11.0, px + 14.0, m + 34.0, "CUBIERTA FOTOVOLTAICA");
    d.text("F1", 9.0, px + 14.0, m + 56.0, "120 módulos de 595 Wp");
    d.text("F1", 9.0, px + 14.0, m + 74.0, "Potencia FV instalada: 71,40 kWp");
    d.text("F1", 9.0, px + 14.0, m + 92.0, "Superficie bruta cubierta: 1.348,25 m²");
    d.text("F1", 9.0, px + 14.0, m + 110.0, "Pendiente faldones F1 / F2: 10° (17,6%)");
    d.text("F1", 9.0, px + 14.0, m + 128.0, "Cotas y cubicaciones en sistema métrico");

    d.rect_stroke(px, m + 190.0, pw, 190.0);
    d.text("F2", 11.0, px + 14.0, m + 214.0, "SIMBOLOGÍA");
    d.stroke_color(0.16, 0.42, 0.72);
    d.rect_stroke(px + 14.0, m + 232.0, 26.0, 14.0);
    d.fill_color(0.08, 0.10, 0.14);
    d.text("F1", 9.0, px + 50.0, m + 243.0, "Módulo fotovoltaico 595 Wp");
    d.stroke_color(0.10, 0.12, 0.16);
    d.line_width(1.5);
    d.line(px + 14.0, m + 268.0, px + 40.0, m + 268.0);
    d.text("F1", 9.0, px + 50.0, m + 272.0, "Contorno de cubierta / alero");
    d.text("F1", 9.0, px + 14.0, m + 305.0, "Norte de cuadrícula: Huso 19S");

    // North arrow symbol
    let nx = px + 60.0;
    let ny = m + 345.0;
    d.circle(nx, ny, 14.0);
    d.line(nx, ny + 14.0, nx, ny - 18.0);
    d.text("F2", 10.0, nx - 3.5, ny - 22.0, "N");

    ops
}

fn build_page2_arq(w_pt: f64, h_pt: f64) -> Vec<Operation> {
    let mut ops = Vec::new();
    let mut d = PdfDraw {
        ops: &mut ops,
        page_h: h_pt,
    };
    draw_title_block(
        &mut d,
        w_pt,
        h_pt,
        "ARQ-02",
        "PLANTA ARQUITECTURA ESTRUCTURAL",
        "1:50",
    );

    let m = 22.0 * PT_PER_MM;
    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 18.0, m + 10.0, m + 28.0, "PLANTA ARQUITECTURA - SALA ELÉCTRICA E INVERSORES");
    d.text("F1", 10.0, m + 10.0, m + 46.0, "ESCALA PRINCIPAL 1:50  |  DETALLE A EN VIEWPORT 1:20");

    // 1:50 scale -> 1 m = 20 mm = 20 * PT_PER_MM
    let s50 = 20.0 * PT_PER_MM;
    let ox = 60.0 * PT_PER_MM;
    let oy = 115.0 * PT_PER_MM;

    // 18m x 10m building at 1:50
    let bw = 18.0 * s50;
    let bh = 10.0 * s50;
    d.stroke_color(0.12, 0.14, 0.18);
    d.line_width(2.0);
    d.rect_stroke(ox, oy, bw, bh);
    // Wall thickness inner outline (0.20m)
    let wall = 0.20 * s50;
    d.line_width(0.8);
    d.rect_stroke(ox + wall, oy + wall, bw - 2.0 * wall, bh - 2.0 * wall);

    // Interior partition at x = 6.0 m and x = 12.0 m
    d.line_width(1.6);
    d.line(ox + 6.0 * s50, oy, ox + 6.0 * s50, oy + bh);
    d.line(ox + 12.0 * s50, oy, ox + 12.0 * s50, oy + bh);

    // Grid axes A, B, C, D and 1, 2
    d.stroke_color(0.55, 0.25, 0.25);
    d.fill_color(0.55, 0.25, 0.25);
    d.line_width(0.5);
    for (idx, x_m) in [0.0, 6.0, 12.0, 18.0].iter().enumerate() {
        let gx = ox + x_m * s50;
        d.line(gx, oy - 36.0, gx, oy + bh + 20.0);
        d.circle(gx, oy - 48.0, 11.0);
        let label = ((b'A' + idx as u8) as char).to_string();
        d.text("F2", 10.0, gx - 3.5, oy - 44.5, &label);
    }

    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 11.0, ox + 1.5 * s50, oy + 5.0 * s50, "SALA ELÉCTRICA (6.00 × 10.00 m)");
    d.text("F2", 11.0, ox + 7.2 * s50, oy + 5.0 * s50, "BODEGA INVERSORES (60.00 m²)");
    d.text("F2", 11.0, ox + 13.5 * s50, oy + 5.0 * s50, "OFICINA TÉCNICA");

    // Detail Viewport box on right side at 1:20 (1 m = 50 mm)
    let dx = 480.0 * PT_PER_MM;
    let dy = 115.0 * PT_PER_MM;
    let s20 = 50.0 * PT_PER_MM;
    d.stroke_color(0.15, 0.35, 0.70);
    d.line_width(1.2);
    d.rect_stroke(dx, dy, 260.0 * PT_PER_MM, 260.0 * PT_PER_MM);
    d.fill_color(0.15, 0.35, 0.70);
    d.text("F2", 11.0, dx + 12.0, dy + 22.0, "DETALLE A - FUNDACIÓN CORRIDA (ESCALA 1:20)");
    d.stroke_color(0.12, 0.14, 0.18);
    d.line_width(1.6);
    // 2.00m x 0.80m footing at 1:20
    d.rect_stroke(dx + 60.0, dy + 140.0, 2.0 * s20, 0.8 * s20);
    d.rect_stroke(dx + 60.0 + 0.7 * s20, dy + 60.0, 0.6 * s20, 80.0);
    d.fill_color(0.1, 0.1, 0.1);
    d.text("F1", 9.0, dx + 60.0, dy + 140.0 + 0.8 * s20 + 20.0, "Zapata 2.00 m × 0.80 m (escala 1:20)");

    ops
}

fn build_page3_ele(w_pt: f64, h_pt: f64) -> Vec<Operation> {
    let mut ops = Vec::new();
    let mut d = PdfDraw {
        ops: &mut ops,
        page_h: h_pt,
    };
    draw_title_block(
        &mut d,
        w_pt,
        h_pt,
        "ELE-03",
        "DETALLES DE MONTAJE Y ZANJA",
        "1:25",
    );
    let m = 22.0 * PT_PER_MM;
    d.fill_color(0.08, 0.10, 0.14);
    d.text("F2", 18.0, m + 10.0, m + 28.0, "DETALLES CONSTRUCTIVOS - CANALIZACIÓN SUBTERRÁNEA");
    d.text("F1", 10.0, m + 10.0, m + 46.0, "ESCALA 1:25  |  PROFUNDIDAD EXCAVACIÓN 0.80 m");

    // 1:25 -> 1 m = 40 mm
    let s25 = 40.0 * PT_PER_MM;
    let tx = 140.0 * PT_PER_MM;
    let ty = 150.0 * PT_PER_MM;
    d.stroke_color(0.12, 0.14, 0.18);
    d.line_width(1.5);
    // Trench section 1.20 m wide x 0.80 m deep
    let tw = 1.20 * s25;
    let td = 0.80 * s25;
    d.line(tx - 80.0, ty, tx, ty);
    d.line(tx, ty, tx, ty + td);
    d.line(tx, ty + td, tx + tw, ty + td);
    d.line(tx + tw, ty + td, tx + tw, ty);
    d.line(tx + tw, ty, tx + tw + 80.0, ty);
    // Conduits
    d.stroke_color(0.85, 0.25, 0.15);
    d.circle(tx + tw * 0.35, ty + td - 18.0, 10.0);
    d.circle(tx + tw * 0.65, ty + td - 18.0, 10.0);
    d.fill_color(0.1, 0.1, 0.1);
    d.text("F2", 10.0, tx + 10.0, ty + td + 24.0, "Zanja típica 1.20 m × 0.80 m - 2× PVC Conduit Ø110 mm");

    ops
}

/// Pre-populated Chilean review markups and measurements for `--demo`.
pub fn create_demo_markups() -> Vec<Markup> {
    let s = PT_PER_M_1_100;
    let ox = 45.0 * PT_PER_MM;
    let oy = 110.0 * PT_PER_MM;

    let red = [229, 57, 53, 255];
    let blue = [37, 99, 235, 255];
    let green = [22, 163, 74, 255];
    let amber = [245, 158, 11, 255];

    // 1. Nube+ roja revisando sector oeste de cubierta
    let mut m1 = Markup::new(
        0,
        "Obs. Arquitectura",
        "M. Figueroa",
        MarkupStyle {
            stroke_color: red,
            fill_color: Some([255, 255, 255, 240]),
            fill_opacity: 0.94,
            line_style: LineStyle::Cloud,
            end_end: LineEnd::ClosedArrow,
            text_color: red,
            font_size: 28.0,
            ..Default::default()
        },
        MarkupKind::CloudPlus {
            cloud_points: Rect2::from_xywh(ox + 1.5 * s, oy + 5.6 * s, 14.5 * s, 5.5 * s)
                .corners()
                .to_vec(),
            is_rect: true,
            knee: Point2::new(ox + 17.5 * s, oy + 4.0 * s),
            box_rect: Rect2::from_xywh(ox + 19.2 * s, oy + 2.5 * s, 15.5 * s, 3.3 * s),
            text: "Verificar distanciamiento 1,20 m al borde".to_string(),
        },
    );
    m1.status = MarkupStatus::Pending;
    m1.layer = "Arquitectura".to_string();

    // 2. Llamada / Callout con flecha azul de ingeniería
    let mut m2 = Markup::new(
        0,
        "Obs. Estructural",
        "Ing. Estructural",
        MarkupStyle {
            stroke_color: blue,
            fill_color: Some([239, 246, 255, 245]),
            fill_opacity: 0.94,
            end_end: LineEnd::ClosedArrow,
            text_color: blue,
            font_size: 28.0,
            ..Default::default()
        },
        MarkupKind::Callout {
            arrow_tip: Point2::new(ox + 48.0 * s, oy + 14.5 * s),
            knee: Point2::new(ox + 50.5 * s, oy + 26.2 * s),
            box_rect: Rect2::from_xywh(ox + 52.0 * s, oy + 24.8 * s, 14.8 * s, 3.3 * s),
            text: "Reforzar costanera en faldón F1".to_string(),
        },
    );
    m2.status = MarkupStatus::Accepted;
    m2.layer = "Estructuras".to_string();

    // 3. Nota adhesiva amarilla
    let mut m3 = Markup::new(
        0,
        "Nota de terreno",
        "ITO Obra",
        MarkupStyle {
            stroke_color: amber,
            fill_color: Some([254, 240, 138, 255]),
            note_icon: NoteIcon::Help,
            ..Default::default()
        },
        MarkupKind::Note {
            pos: Point2::new(ox + 30.0 * s, oy + 11.5 * s),
        },
    );
    m3.comment = "Confirmar acceso grúa por costado sur antes de montaje.".to_string();
    m3.status = MarkupStatus::Pending;

    // 4. Medición de Longitud (m) exacta sobre borde superior: 59.50 m
    let mut m4 = Markup::new(
        0,
        "Largo cubierta",
        "M. Figueroa",
        MarkupStyle {
            stroke_color: red,
            start_end: LineEnd::Slash,
            end_end: LineEnd::Slash,
            text_color: red,
            font_size: 11.0,
            ..Default::default()
        },
        MarkupKind::MeasureLength {
            start: Point2::new(ox, oy),
            end: Point2::new(ox + 59.50 * s, oy),
            offset_pt: -34.0,
            opts: MeasurementOptions::default(),
        },
    );
    m4.label = "Eje Norte".to_string();
    m4.layer = "Mediciones".to_string();

    // 5. Medición de Área (m²) sobre faldón sur con un recorte
    let mut m5 = Markup::new(
        0,
        "Superficie útil F1",
        "M. Figueroa",
        MarkupStyle {
            stroke_color: green,
            fill_color: Some(green),
            fill_opacity: 0.20,
            hatch: HatchPattern::Diagonal45,
            text_color: green,
            font_size: 11.0,
            ..Default::default()
        },
        MarkupKind::MeasureArea {
            points: Rect2::from_xywh(ox + 13.0 * s, oy + 12.0 * s, 45.0 * s, 11.0 * s)
                .corners()
                .to_vec(),
            cutouts: vec![
                Rect2::from_xywh(ox + 26.0 * s, oy + 15.0 * s, 4.0 * s, 2.5 * s)
                    .corners()
                    .to_vec(),
            ],
            opts: MeasurementOptions {
                show_segments: true,
                ..Default::default()
            },
        },
    );
    m5.label = "Faldón F1".to_string();
    m5.layer = "Mediciones".to_string();

    // 6. Conteo de tableros / anclajes
    let mut m6 = Markup::new(
        0,
        "Anclajes críticos",
        "ITO Obra",
        MarkupStyle {
            stroke_color: blue,
            text_color: blue,
            ..Default::default()
        },
        MarkupKind::MeasureCount {
            points: vec![
                Point2::new(ox + 2.0 * s, oy + 2.0 * s),
                Point2::new(ox + 20.0 * s, oy + 2.0 * s),
                Point2::new(ox + 40.0 * s, oy + 2.0 * s),
                Point2::new(ox + 57.5 * s, oy + 2.0 * s),
            ],
            opts: MeasurementOptions::default(),
        },
    );
    m6.layer = "Mediciones".to_string();

    // 7. Sello "PRELIMINAR" en esquina inferior izquierda
    let mut m7 = Markup::new(
        0,
        "Sello estado",
        "M. Figueroa",
        MarkupStyle {
            stroke_color: amber,
            fill_color: Some(amber),
            fill_opacity: 0.12,
            text_color: amber,
            bold: true,
            font_size: 16.0,
            line_width: 2.5,
            ..Default::default()
        },
        MarkupKind::Stamp {
            rect: Rect2::from_xywh(ox + 18.0 * s, oy + 28.0 * s, 17.5 * s, 3.8 * s),
            title: "PRELIMINAR".to_string(),
            subtitle: "Revisión RedBIM Chile".to_string(),
            show_date: true,
            show_author: true,
        },
    );
    m7.layer = "General".to_string();

    vec![m1, m2, m3, m4, m5, m6, m7]
}
