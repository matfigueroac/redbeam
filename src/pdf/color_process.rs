use crate::model::document::DocumentTab;
use crate::model::scale::parse_page_range;
use crate::pdf::renderer::{default_interpreter_settings, render_page_rgba};
use hayro::RenderSettings;
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProcessMode {
    /// Clean grayscale conversion (preserves line weights & shading).
    Grayscale,
    /// High-contrast pure Black & White with luminance threshold (ideal for scans & faint CAD).
    BlackAndWhite { threshold: u8 },
    /// Remaps all dark linework to a single tint color (e.g. cool gray or blueprint blue).
    TintColor { color: [u8; 3] },
    /// Inverts page colors (useful for dark-background CAD sheets).
    Invert,
}

impl ColorProcessMode {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::Grayscale => "Escala de grises",
            Self::BlackAndWhite { .. } => "Blanco y Negro puro (Alto contraste)",
            Self::TintColor { .. } => "Colorear líneas del plano (Tinte único)",
            Self::Invert => "Invertir colores (Fondo oscuro ↔ claro)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorProcessConfig {
    pub mode: ColorProcessMode,
    /// Empty string = all pages; `"current"` handled by caller or explicit range like `"1-3, 5"`.
    pub page_range: String,
    pub max_side_px: u32,
}

impl Default for ColorProcessConfig {
    fn default() -> Self {
        Self {
            mode: ColorProcessMode::Grayscale,
            page_range: String::new(),
            max_side_px: 2200,
        }
    }
}

/// Processes colors on selected pages of `tab` (mirroring Bluebeam's `SCmdColorProcess`)
/// while preserving all live markups, measurements, scales, spaces, and labels intact.
pub fn apply_color_process_in_tab(
    tab: &mut DocumentTab,
    cfg: &ColorProcessConfig,
) -> Result<usize, String> {
    let total_pages = tab.pages.len();
    if total_pages == 0 {
        return Err("El documento no contiene páginas.".to_string());
    }

    let targets: Vec<usize> = if cfg.page_range.trim().is_empty() {
        (0..total_pages).collect()
    } else {
        let p = parse_page_range(&cfg.page_range, total_pages);
        if p.is_empty() {
            return Err("Rango de páginas inválido (ej: 1-3, 5).".to_string());
        }
        p
    };

    let mut doc =
        Document::load_mem(&tab.raw_pdf_bytes).map_err(|e| format!("Error lopdf: {e}"))?;
    let pages_map = doc.get_pages();
    let interp = default_interpreter_settings();
    let rs = RenderSettings::default();
    let max_px = cfg.max_side_px.clamp(800, 3600);
    let mut processed = 0usize;

    for page_idx in targets {
        let Some(&page_id) = pages_map.get(&(page_idx as u32 + 1)) else {
            continue;
        };
        let Some(page_model) = tab.pages.get(page_idx) else {
            continue;
        };
        let w_pt = page_model.width_pt;
        let h_pt = page_model.height_pt;

        let rendered = render_page_rgba(&tab.raw_pdf_bytes, page_idx, max_px, &interp, &rs)?;
        let w = rendered.width;
        let h = rendered.height;
        if w == 0 || h == 0 {
            continue;
        }

        let img_stream = match cfg.mode {
            ColorProcessMode::Grayscale => {
                let mut gray = Vec::with_capacity(w * h);
                for px in rendered.premul_rgba.as_chunks::<4>().0 {
                    let lum = (0.299 * px[0] as f32
                        + 0.587 * px[1] as f32
                        + 0.114 * px[2] as f32)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    gray.push(lum);
                }
                let mut s = Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Image",
                        "Width" => (w as i64),
                        "Height" => (h as i64),
                        "ColorSpace" => "DeviceGray",
                        "BitsPerComponent" => 8i64,
                    },
                    gray,
                );
                let _ = s.compress();
                s
            }
            ColorProcessMode::BlackAndWhite { threshold } => {
                let mut gray = Vec::with_capacity(w * h);
                for px in rendered.premul_rgba.as_chunks::<4>().0 {
                    let lum = (0.299 * px[0] as f32
                        + 0.587 * px[1] as f32
                        + 0.114 * px[2] as f32)
                        .round() as u8;
                    gray.push(if lum < threshold { 0 } else { 255 });
                }
                let mut s = Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Image",
                        "Width" => (w as i64),
                        "Height" => (h as i64),
                        "ColorSpace" => "DeviceGray",
                        "BitsPerComponent" => 8i64,
                    },
                    gray,
                );
                let _ = s.compress();
                s
            }
            ColorProcessMode::TintColor { color: [tr, tg, tb] } => {
                let mut rgb = Vec::with_capacity(w * h * 3);
                for px in rendered.premul_rgba.as_chunks::<4>().0 {
                    let lum = 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
                    let ink = ((255.0 - lum) / 255.0).clamp(0.0, 1.0);
                    rgb.push((255.0 - ink * (255.0 - tr as f32)).round() as u8);
                    rgb.push((255.0 - ink * (255.0 - tg as f32)).round() as u8);
                    rgb.push((255.0 - ink * (255.0 - tb as f32)).round() as u8);
                }
                let mut s = Stream::new(
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
                let _ = s.compress();
                s
            }
            ColorProcessMode::Invert => {
                let mut rgb = Vec::with_capacity(w * h * 3);
                for px in rendered.premul_rgba.as_chunks::<4>().0 {
                    rgb.push(255u8.saturating_sub(px[0]));
                    rgb.push(255u8.saturating_sub(px[1]));
                    rgb.push(255u8.saturating_sub(px[2]));
                }
                let mut s = Stream::new(
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
                let _ = s.compress();
                s
            }
        };

        let img_id = doc.add_object(img_stream);
        let ops = vec![
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
            Operation::new("Do", vec![Object::Name(b"CP_IMG".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        let content_bytes = Content { operations: ops }.encode().unwrap_or_default();
        let mut cstream = Stream::new(dictionary! {}, content_bytes);
        let _ = cstream.compress();
        let content_id = doc.add_object(cstream);

        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            page_dict.set("MediaBox", vec![0.0.into(), 0.0.into(), w_pt.into(), h_pt.into()]);
            page_dict.remove(b"CropBox");
            page_dict.set("Rotate", 0i64);
            page_dict.set("Contents", content_id);
            page_dict.set(
                "Resources",
                dictionary! {
                    "XObject" => dictionary! { "CP_IMG" => img_id },
                },
            );
        }
        if let Some(pm) = tab.pages.get_mut(page_idx) {
            pm.pdf_crop_box = Some([0.0, 0.0, w_pt, h_pt]);
            pm.pdf_native_rotation = 0;
            pm.rotation_deg = 0;
        }
        processed += 1;
    }

    doc.prune_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Error guardando PDF procesado: {e}"))?;
    tab.raw_pdf_bytes = out.into();
    tab.undo_stack.clear();
    tab.redo_stack.clear();
    tab.dirty = true;
    Ok(processed)
}
