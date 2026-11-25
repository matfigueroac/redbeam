use crate::geometry::math::Rect2;
use crate::model::markup::{Markup, MarkupKind, MarkupStyle};
use crate::model::settings::CsvDelimiter;
use crate::pdf::export::export_markups_csv;
use crate::pdf::flatten::flatten_pdf_to_bytes_with_options;
use crate::pdf::headers_footers::{HeaderFooterConfig, apply_headers_footers_in_tab};
use crate::pdf::io::{atomic_write_file, open_pdf_file, save_pdf_to_bytes};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatchStampPlacement {
    #[default]
    TopRight,
    BottomRight,
    TopLeft,
    Center,
}

impl BatchStampPlacement {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::TopRight => "Esquina superior derecha",
            Self::BottomRight => "Esquina inferior derecha",
            Self::TopLeft => "Esquina superior izquierda",
            Self::Center => "Centro de lámina",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatchJobConfig {
    pub input_files: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub flatten_markups: bool,
    pub flatten_recoverable: bool,
    pub apply_headers_footers: bool,
    pub headers_footers: HeaderFooterConfig,
    pub apply_stamp: bool,
    pub stamp_title: String,
    pub stamp_subtitle: String,
    pub stamp_color: [u8; 3],
    pub stamp_placement: BatchStampPlacement,
    pub author_name: String,
    pub export_consolidated_csv: bool,
    pub csv_delimiter: CsvDelimiter,
}

#[derive(Debug, Clone)]
pub struct BatchJobReport {
    pub processed_files: usize,
    pub failed_files: Vec<(String, String)>,
    pub total_pages_processed: usize,
    pub total_markups_exported: usize,
    pub consolidated_csv_path: Option<PathBuf>,
}

/// Runs a 100% local offline batch pipeline across multiple PDF files (mirroring Bluebeam's
/// `SCmdBatch` / Batch Stamp / Batch Flatten / Batch Headers & Footers / Batch CSV Summary).
pub fn run_batch_job(cfg: &BatchJobConfig) -> Result<BatchJobReport, String> {
    if cfg.input_files.is_empty() {
        return Err("Seleccione al menos un archivo PDF de entrada.".to_string());
    }
    std::fs::create_dir_all(&cfg.output_dir)
        .map_err(|e| format!("No se pudo crear carpeta de salida: {e}"))?;

    let mut report = BatchJobReport {
        processed_files: 0,
        failed_files: Vec::new(),
        total_pages_processed: 0,
        total_markups_exported: 0,
        consolidated_csv_path: None,
    };

    let d = cfg.csv_delimiter.as_char();
    let mut csv_lines: Vec<String> = Vec::new();
    let mut csv_header_written = false;

    for src_path in &cfg.input_files {
        let file_name = src_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Documento.pdf")
            .to_string();

        let mut tab = match open_pdf_file(src_path) {
            Ok(t) => t,
            Err(e) => {
                report.failed_files.push((file_name, e));
                continue;
            }
        };

        // 1. Optional batch stamp on every page
        if cfg.apply_stamp && !cfg.stamp_title.trim().is_empty() {
            let [r, g, b] = cfg.stamp_color;
            let style = MarkupStyle {
                stroke_color: [r, g, b, 255],
                fill_color: Some([r, g, b, 36]),
                fill_opacity: 0.14,
                text_color: [r, g, b, 255],
                bold: true,
                font_size: 14.0,
                line_width: 2.0,
                ..Default::default()
            };
            let stamp_w = 210.0;
            let stamp_h = 52.0;
            let margin = 28.0;
            for p in &tab.pages {
                let rect = match cfg.stamp_placement {
                    BatchStampPlacement::TopRight => Rect2::from_xywh(
                        (p.width_pt - stamp_w - margin).max(10.0),
                        margin,
                        stamp_w,
                        stamp_h,
                    ),
                    BatchStampPlacement::BottomRight => Rect2::from_xywh(
                        (p.width_pt - stamp_w - margin).max(10.0),
                        (p.height_pt - stamp_h - margin).max(10.0),
                        stamp_w,
                        stamp_h,
                    ),
                    BatchStampPlacement::TopLeft => {
                        Rect2::from_xywh(margin, margin, stamp_w, stamp_h)
                    }
                    BatchStampPlacement::Center => Rect2::from_xywh(
                        ((p.width_pt - stamp_w) * 0.5).max(10.0),
                        ((p.height_pt - stamp_h) * 0.5).max(10.0),
                        stamp_w,
                        stamp_h,
                    ),
                };
                tab.markups.push(Markup::new(
                    p.page_index,
                    "Sello en lote",
                    &cfg.author_name,
                    style.clone(),
                    MarkupKind::Stamp {
                        rect,
                        title: cfg.stamp_title.trim().to_string(),
                        subtitle: cfg.stamp_subtitle.trim().to_string(),
                        show_date: true,
                        show_author: true,
                    },
                ));
            }
        }

        // 2. Collect consolidated CSV rows BEFORE flattening clears live markups
        if cfg.export_consolidated_csv && !tab.markups.is_empty() {
            let doc_csv = export_markups_csv(&tab, cfg.csv_delimiter);
            let stripped = doc_csv.trim_start_matches('\u{FEFF}');
            let mut lines = stripped.lines();
            if let Some(hdr) = lines.next()
                && !csv_header_written
            {
                csv_lines.push(format!("\u{FEFF}Documento{d}{hdr}"));
                csv_header_written = true;
            }
            for line in lines {
                if line.starts_with("TOTALES") || line.trim().is_empty() {
                    continue;
                }
                csv_lines.push(format!("{file_name}{d}{line}"));
                report.total_markups_exported += 1;
            }
        }

        // 3. Optional Headers & Footers
        if cfg.apply_headers_footers
            && !cfg.headers_footers.is_empty()
            && let Err(e) = apply_headers_footers_in_tab(&mut tab, &cfg.headers_footers)
        {
            report.failed_files.push((file_name, e));
            continue;
        }

        // 4. Flatten or Save output PDF atomically
        let out_bytes = if cfg.flatten_markups {
            flatten_pdf_to_bytes_with_options(&tab, cfg.flatten_recoverable)
        } else {
            save_pdf_to_bytes(&tab)
        };

        let out_path = cfg.output_dir.join(&file_name);
        match out_bytes.and_then(|bytes| atomic_write_file(&out_path, &bytes)) {
            Ok(()) => {
                report.processed_files += 1;
                report.total_pages_processed += tab.pages.len();
            }
            Err(e) => {
                report.failed_files.push((file_name, e));
            }
        }
    }

    if cfg.export_consolidated_csv && csv_header_written {
        let csv_path = cfg.output_dir.join("Consolidado_Cubicacion_RedBeam.csv");
        let csv_body = csv_lines.join("\n") + "\n";
        atomic_write_file(&csv_path, csv_body.as_bytes())?;
        report.consolidated_csv_path = Some(csv_path);
    }

    Ok(report)
}
