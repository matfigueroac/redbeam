use crate::geometry::math::{Point2, Rect2};
use crate::model::custom_columns::{ChoiceOption, CustomColumnDef, CustomColumnKind};
use crate::model::document::DocumentTab;
use crate::model::markup::{Markup, MarkupKind, MarkupStyle, MeasurementOptions};
use crate::model::scale::{IMPERIAL_PRESETS, METRIC_PRESETS, PageScale, parse_page_range};
use crate::model::settings::{AppSettings, CsvDelimiter, ThemeMode};
use crate::model::tool_chest::{SequenceSpec, SequenceStyle, ToolChest};
use crate::model::units::{LengthUnit, PT_PER_MM, Precision, SheetDisplayUnit, UnitSystem};
use crate::pdf::batch::{BatchJobConfig, BatchStampPlacement};
use crate::pdf::color_process::{ColorProcessConfig, ColorProcessMode};
use crate::pdf::headers_footers::HeaderFooterConfig;
use crate::pdf::optimize::OptimizePreset;
use crate::pdf::overlay::OverlayAlignmentMode;
use crate::pdf::visual_search::VisualSearchResult;
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, RichText, TextureHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalePageScope {
    Current,
    Range,
    All,
}

#[derive(Debug, Clone)]
pub enum ActiveModal {
    Calibrate {
        measured_pts: f64,
        known_value_str: String,
        unit: LengthUnit,
        page_scope: ScalePageScope,
        page_range_str: String,
    },
    SetScale {
        draft: PageScale,
        page_scope: ScalePageScope,
        page_range_str: String,
        custom_paper_str: String,
        custom_real_str: String,
        custom_y_paper_str: String,
        custom_y_real_str: String,
    },
    EditViewport {
        viewport_id: Uuid,
        name: String,
        draft: PageScale,
        custom_paper_str: String,
        custom_real_str: String,
    },
    SketchToScale {
        width_str: String,
        height_str: String,
        unit: LengthUnit,
    },
    CreateStamp {
        title: String,
        subtitle: String,
        color: [u8; 3],
        show_date: bool,
        show_author: bool,
    },
    ManageCustomColumns {
        new_name: String,
        new_kind_idx: usize,
        new_formula: String,
        new_choices_csv: String,
        new_prefix: String,
        new_suffix: String,
        new_decimals: u8,
        new_include_totals: bool,
    },
    EditPageLabel {
        page_index: usize,
        label_buf: String,
        focus_requested: bool,
    },
    OverlayPdfs {
        pdf_b_path: Option<PathBuf>,
        color_a: [u8; 3],
        color_b: [u8; 3],
        align_mode_idx: usize,
        all_pages: bool,
        manual_dx_mm: f64,
        manual_dy_mm: f64,
        manual_angle_deg: f64,
        manual_scale: f64,
        pts_a: [(f64, f64); 3],
        pts_b: [(f64, f64); 3],
    },
    VisualSearchResults {
        result: VisualSearchResult,
        threshold: f32,
        subject_name: String,
    },
    HeadersAndFooters {
        cfg: HeaderFooterConfig,
    },
    BatchProcess {
        input_files: Vec<PathBuf>,
        output_dir: Option<PathBuf>,
        flatten_markups: bool,
        flatten_recoverable: bool,
        apply_headers_footers: bool,
        hf_cfg: HeaderFooterConfig,
        apply_stamp: bool,
        stamp_title: String,
        stamp_subtitle: String,
        stamp_color: [u8; 3],
        stamp_placement: BatchStampPlacement,
        export_consolidated_csv: bool,
        csv_delimiter: CsvDelimiter,
    },
    ConfigureSequence {
        item_id: Uuid,
        initialized: bool,
        enabled: bool,
        prefix: String,
        suffix: String,
        next_value: u32,
        step: u32,
        pad_digits: usize,
        style: SequenceStyle,
    },
    PhotoPreview {
        attachment_id: Uuid,
        caption: String,
    },
    ApplyRedactionsConfirm {
        fill_color: [u8; 3],
    },
    ColorProcess {
        mode_idx: usize,
        bw_threshold: u8,
        tint_color: [u8; 3],
        page_scope_idx: usize,
        page_range: String,
        max_side_px: u32,
    },
    ReduceFileSize {
        preset: OptimizePreset,
    },
    Preferences,
    Shortcuts,
    CommandPalette {
        query: String,
        selected: usize,
        focus_requested: bool,
    },
    About,
    ConfirmClose {
        tab_idx: Option<usize>,
    },
}

fn format_scale_input(v: f64) -> String {
    if !v.is_finite() {
        return "1".to_string();
    }
    let s = format!("{v:.6}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

impl ActiveModal {
    pub fn from_request(req: DialogRequest, tab: Option<&DocumentTab>) -> Self {
        match req {
            DialogRequest::Calibrate {
                measured_pts,
                initial_value,
                unit,
                apply_all_pages,
            } => Self::Calibrate {
                measured_pts,
                known_value_str: initial_value,
                unit,
                page_scope: if apply_all_pages {
                    ScalePageScope::All
                } else {
                    ScalePageScope::Current
                },
                page_range_str: "1-2".to_string(),
            },
            DialogRequest::SetScale => {
                let draft = tab
                    .and_then(|t| t.active_page())
                    .map(|p| p.scale.clone())
                    .unwrap_or_default();
                Self::SetScale {
                    custom_paper_str: format_scale_input(draft.paper_value),
                    custom_real_str: format_scale_input(draft.real_value),
                    custom_y_paper_str: format_scale_input(draft.y_paper_value),
                    custom_y_real_str: format_scale_input(draft.y_real_value),
                    draft,
                    page_scope: ScalePageScope::Current,
                    page_range_str: "1-2".to_string(),
                }
            }
            DialogRequest::EditViewport { viewport_id } => {
                let vp = tab
                    .and_then(|t| t.active_page())
                    .and_then(|p| p.viewports.iter().find(|v| v.id == viewport_id));
                let name = vp
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "Detalle".to_string());
                let draft = vp
                    .map(|v| v.scale.clone())
                    .unwrap_or_else(|| PageScale::from_metric_ratio(50.0));
                Self::EditViewport {
                    viewport_id,
                    name,
                    custom_paper_str: format_scale_input(draft.paper_value),
                    custom_real_str: format_scale_input(draft.real_value),
                    draft,
                }
            }
            DialogRequest::SketchToScale => Self::SketchToScale {
                width_str: "10.00".to_string(),
                height_str: "5.00".to_string(),
                unit: tab
                    .and_then(|t| t.active_page())
                    .map(|p| p.scale.display_unit)
                    .unwrap_or(LengthUnit::Meter),
            },
            DialogRequest::CreateStamp => Self::CreateStamp {
                title: "REVISADO".to_string(),
                subtitle: "RedBeam Chile".to_string(),
                color: [229, 57, 53],
                show_date: true,
                show_author: true,
            },
            DialogRequest::ManageCustomColumns => Self::ManageCustomColumns {
                new_name: String::new(),
                new_kind_idx: 4, // Formula by default
                new_formula: "Medicion * PrecioUnitario".to_string(),
                new_choices_csv: "1.00 = Sin desperdicio, 1.05 = 5%, 1.10 = 10%".to_string(),
                new_prefix: "$ ".to_string(),
                new_suffix: String::new(),
                new_decimals: 0,
                new_include_totals: true,
            },
            DialogRequest::EditPageLabel { page_index } => {
                let current = tab
                    .and_then(|t| t.pages.get(page_index))
                    .and_then(|p| p.label.clone())
                    .unwrap_or_default();
                Self::EditPageLabel {
                    page_index,
                    label_buf: current,
                    focus_requested: true,
                }
            }
            DialogRequest::OverlayPdfs => Self::OverlayPdfs {
                pdf_b_path: None,
                color_a: [220, 38, 38],
                color_b: [22, 163, 74],
                align_mode_idx: 0, // AutoTranslate
                all_pages: false,
                manual_dx_mm: 0.0,
                manual_dy_mm: 0.0,
                manual_angle_deg: 0.0,
                manual_scale: 1.0,
                pts_a: [(50.0, 50.0), (500.0, 50.0), (50.0, 400.0)],
                pts_b: [(50.0, 50.0), (500.0, 50.0), (50.0, 400.0)],
            },
            DialogRequest::VisualSearchResults { result } => Self::VisualSearchResults {
                result,
                threshold: 0.74,
                subject_name: "Conteo visual".to_string(),
            },
            DialogRequest::HeadersAndFooters => Self::HeadersAndFooters {
                cfg: HeaderFooterConfig::default(),
            },
            DialogRequest::BatchProcess => Self::BatchProcess {
                input_files: Vec::new(),
                output_dir: None,
                flatten_markups: true,
                flatten_recoverable: true,
                apply_headers_footers: false,
                hf_cfg: HeaderFooterConfig::default(),
                apply_stamp: false,
                stamp_title: "PARA CONSTRUCCIÓN".to_string(),
                stamp_subtitle: "Emitido en lote - RedBeam".to_string(),
                stamp_color: [37, 99, 235],
                stamp_placement: BatchStampPlacement::TopRight,
                export_consolidated_csv: true,
                csv_delimiter: CsvDelimiter::Semicolon,
            },
            DialogRequest::ConfigureSequence { item_id } => Self::ConfigureSequence {
                item_id,
                initialized: false,
                enabled: true,
                prefix: "RDI-".to_string(),
                suffix: String::new(),
                next_value: 1,
                step: 1,
                pad_digits: 3,
                style: SequenceStyle::Numeric,
            },
            DialogRequest::PhotoPreview {
                attachment_id,
                caption,
            } => Self::PhotoPreview {
                attachment_id,
                caption,
            },
            DialogRequest::ApplyRedactionsConfirm => Self::ApplyRedactionsConfirm {
                fill_color: [0, 0, 0],
            },
            DialogRequest::ColorProcess => Self::ColorProcess {
                mode_idx: 0,
                bw_threshold: 225,
                tint_color: [120, 130, 145],
                page_scope_idx: 0,
                page_range: "1-2".to_string(),
                max_side_px: 2200,
            },
            DialogRequest::ReduceFileSize => Self::ReduceFileSize {
                preset: OptimizePreset::Balanced,
            },
            DialogRequest::Preferences => Self::Preferences,
            DialogRequest::Shortcuts => Self::Shortcuts,
            DialogRequest::CommandPalette => Self::CommandPalette {
                query: String::new(),
                selected: 0,
                focus_requested: true,
            },
            DialogRequest::About => Self::About,
            DialogRequest::ConfirmClose { tab_idx } => Self::ConfirmClose { tab_idx },
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_active_modal(
    ctx: &egui::Context,
    modal_slot: &mut Option<ActiveModal>,
    tab: Option<&mut DocumentTab>,
    tool_chest: &mut ToolChest,
    image_textures: &HashMap<Uuid, TextureHandle>,
    settings: &mut AppSettings,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(mut modal) = modal_slot.take() else {
        return;
    };
    let total_pages = tab.as_deref().map(|t| t.pages.len()).unwrap_or(1);
    let mut keep_open = true;
    let mut close_clicked = false;

    match &mut modal {
        ActiveModal::Calibrate {
            measured_pts,
            known_value_str,
            unit,
            page_scope,
            page_range_str,
        } => {
            egui::Window::new("Calibrar escala (2 puntos)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Distancia medida en lámina: {:.2} mm ({:.1} pt)",
                            *measured_pts / PT_PER_MM,
                            *measured_pts
                        ))
                        .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Medida real conocida:");
                        ui.add(
                            egui::TextEdit::singleline(known_value_str)
                                .desired_width(110.0)
                                .hint_text("Ej: 59.50"),
                        );
                        egui::ComboBox::from_id_salt("cal_unit_cb")
                            .selected_text(unit.display_name())
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for &lu in LengthUnit::ALL {
                                    ui.selectable_value(unit, lu, lu.display_name());
                                }
                            });
                    });
                    if let Ok(val) = known_value_str.trim().replace(',', ".").parse::<f64>()
                        && val > 0.0
                    {
                        let preview = PageScale::from_calibration(*measured_pts, val, *unit);
                        ui.label(
                            RichText::new(format!("Escala resultante: {}", preview.label()))
                                .strong()
                                .color(theme.accent_green),
                        );
                    }
                    ui.add_space(4.0);
                    draw_scale_scope_selector(ui, "cal", page_scope, page_range_str, total_pages);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Aplicar escala").color(theme.text_on_accent)).fill(theme.accent_red))
                            .clicked()
                            && let Ok(val) = known_value_str.trim().replace(',', ".").parse::<f64>()
                            && val > 0.0
                        {
                            let s = PageScale::from_calibration(*measured_pts, val, *unit);
                            emit_scale_action(s, *page_scope, page_range_str, total_pages, actions);
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }

        ActiveModal::SetScale {
            draft,
            page_scope,
            page_range_str,
            custom_paper_str,
            custom_real_str,
            custom_y_paper_str,
            custom_y_real_str,
        } => {
            egui::Window::new("Configurar escala y unidades")
                .collapsible(false)
                .resizable(false)
                .default_width(470.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Preestablecidas Métricas (Chile):").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Calibrar 2 pts...").clicked() {
                                actions.push(UiAction::StartTwoPointCalibration);
                                close_clicked = true;
                            }
                        });
                    });
                    draw_scale_presets_and_custom(
                        ui,
                        "dlg",
                        draft,
                        custom_paper_str,
                        custom_real_str,
                    );

                    ui.checkbox(&mut draft.separate_xy, "Escala vertical (Y) independiente");
                    if draft.separate_xy {
                        ui.horizontal(|ui| {
                            ui.label("Y:");
                            ui.add(egui::TextEdit::singleline(custom_y_paper_str).desired_width(60.0));
                            ui.label(draft.y_paper_unit.suffix());
                            ui.label("=");
                            ui.add(egui::TextEdit::singleline(custom_y_real_str).desired_width(70.0));
                            ui.label(draft.y_real_unit.suffix());
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Mostrar en:");
                        egui::ComboBox::from_id_salt("dlg_disp_u")
                            .selected_text(draft.display_unit.display_name())
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for &u in LengthUnit::ALL {
                                    ui.selectable_value(&mut draft.display_unit, u, u.display_name());
                                }
                            });
                        egui::ComboBox::from_id_salt("dlg_prec_u")
                            .selected_text(draft.precision.label())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for &pr in Precision::ALL {
                                    ui.selectable_value(&mut draft.precision, pr, pr.label());
                                }
                            });
                    });
                    ui.add_space(4.0);
                    draw_scale_scope_selector(ui, "dlg", page_scope, page_range_str, total_pages);

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Aceptar").color(theme.text_on_accent)).fill(theme.accent_red))
                            .clicked()
                        {
                            apply_custom_strings_to_draft(
                                draft,
                                custom_paper_str,
                                custom_real_str,
                                Some((custom_y_paper_str.as_str(), custom_y_real_str.as_str())),
                            );
                            emit_scale_action(
                                draft.clone(),
                                *page_scope,
                                page_range_str,
                                total_pages,
                                actions,
                            );
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }

        ActiveModal::EditViewport {
            viewport_id,
            name,
            draft,
            custom_paper_str,
            custom_real_str,
        } => {
            egui::Window::new("Editar Viewport")
                .collapsible(false)
                .resizable(false)
                .default_width(450.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Nombre del Viewport:");
                        ui.add(egui::TextEdit::singleline(name).desired_width(220.0));
                    });
                    ui.separator();
                    ui.label(RichText::new("Escala local del Viewport:").strong());
                    draw_scale_presets_and_custom(
                        ui,
                        "vp",
                        draft,
                        custom_paper_str,
                        custom_real_str,
                    );
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Mostrar en:");
                        egui::ComboBox::from_id_salt("vp_disp_u")
                            .selected_text(draft.display_unit.display_name())
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                for &u in LengthUnit::ALL {
                                    ui.selectable_value(&mut draft.display_unit, u, u.display_name());
                                }
                            });
                        egui::ComboBox::from_id_salt("vp_prec_u")
                            .selected_text(draft.precision.label())
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for &pr in Precision::ALL {
                                    ui.selectable_value(&mut draft.precision, pr, pr.label());
                                }
                            });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Guardar Viewport").color(theme.text_on_accent)).fill(theme.accent_red))
                            .clicked()
                        {
                            apply_custom_strings_to_draft(
                                draft,
                                custom_paper_str,
                                custom_real_str,
                                None,
                            );
                            actions.push(UiAction::SaveViewportEdit {
                                id: *viewport_id,
                                name: if name.trim().is_empty() {
                                    "Detalle".to_string()
                                } else {
                                    name.trim().to_string()
                                },
                                scale: draft.clone(),
                            });
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }

        ActiveModal::SketchToScale {
            width_str,
            height_str,
            unit,
        } => {
            egui::Window::new("Dibujar rectángulo a escala exacta")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Ancho:");
                        ui.add(egui::TextEdit::singleline(width_str).desired_width(80.0));
                        ui.label("Alto:");
                        ui.add(egui::TextEdit::singleline(height_str).desired_width(80.0));
                        egui::ComboBox::from_id_salt("sketch_u")
                            .selected_text(unit.suffix())
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                for &u in LengthUnit::ALL {
                                    ui.selectable_value(unit, u, u.suffix());
                                }
                            });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Insertar en lámina").color(theme.text_on_accent)).fill(theme.accent_red))
                            .clicked()
                            && let (Ok(w), Ok(h), Some(tab)) = (
                                width_str.trim().replace(',', ".").parse::<f64>(),
                                height_str.trim().replace(',', ".").parse::<f64>(),
                                tab,
                            )
                            && w > 0.0
                            && h > 0.0
                        {
                            let idx = tab.active_page_index();
                            if let Some((w_pt, h_pt, cx, cy)) = tab.pages.get(idx).map(|p| {
                                let wm = unit.to_meters(w);
                                let hm = unit.to_meters(h);
                                (
                                    wm / p.scale.pt_to_meters_x().max(1e-9),
                                    hm / p.scale.pt_to_meters_y().max(1e-9),
                                    p.width_pt * 0.5,
                                    p.height_pt * 0.5,
                                )
                            }) {
                                let rect =
                                    Rect2::from_xywh(cx - w_pt * 0.5, cy - h_pt * 0.5, w_pt, h_pt);
                                tab.push_undo();
                                let m = Markup::new(
                                    idx,
                                    "Dibujo a escala",
                                    &settings.author_name,
                                    MarkupStyle {
                                        stroke_color: [37, 99, 235, 255],
                                        fill_color: Some([37, 99, 235, 255]),
                                        fill_opacity: 0.14,
                                        text_color: [37, 99, 235, 255],
                                        ..Default::default()
                                    },
                                    MarkupKind::SketchRect {
                                        rect,
                                        opts: MeasurementOptions::default(),
                                    },
                                );
                                let id = m.id;
                                tab.markups.push(m);
                                tab.select_only(id);
                            }
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }

        ActiveModal::CreateStamp {
            title,
            subtitle,
            color,
            show_date,
            show_author,
        } => {
            egui::Window::new("Crear sello")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Título:");
                        ui.text_edit_singleline(title);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Subtítulo:");
                        ui.text_edit_singleline(subtitle);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        ui.color_edit_button_srgb(color);
                        ui.checkbox(show_date, "Fecha");
                        ui.checkbox(show_author, "Autor");
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Insertar sello").color(theme.text_on_accent)).fill(theme.accent_red))
                            .clicked()
                            && let Some(tab) = tab
                        {
                            let idx = tab.active_page_index();
                            if let Some((pw, ph)) = tab.pages.get(idx).map(|p| (p.width_pt, p.height_pt)) {
                                tab.push_undo();
                                let c = [color[0], color[1], color[2], 255];
                                let m = Markup::new(
                                    idx,
                                    "Sello",
                                    &settings.author_name,
                                    MarkupStyle {
                                        stroke_color: c,
                                        fill_color: Some(c),
                                        fill_opacity: 0.12,
                                        text_color: c,
                                        font_size: 16.0,
                                        bold: true,
                                        line_width: 2.5,
                                        ..Default::default()
                                    },
                                    MarkupKind::Stamp {
                                        rect: Rect2::from_xywh(
                                            pw * 0.5 - 110.0,
                                            ph * 0.5 - 28.0,
                                            220.0,
                                            56.0,
                                        ),
                                        title: title.clone(),
                                        subtitle: subtitle.clone(),
                                        show_date: *show_date,
                                        show_author: *show_author,
                                    },
                                );
                                let id = m.id;
                                tab.markups.push(m);
                                tab.select_only(id);
                            }
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }

        ActiveModal::Preferences => {
            egui::Window::new("Preferencias de RedBeam")
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    let mut changed = false;
                    egui::Grid::new("prefs_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Autor de revisiones:");
                            if ui.text_edit_singleline(&mut settings.author_name).changed() {
                                changed = true;
                            }
                            ui.end_row();

                            ui.label("Tema visual:");
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(settings.theme == ThemeMode::Dark, "Oscuro")
                                    .clicked()
                                {
                                    actions.push(UiAction::SetTheme(ThemeMode::Dark));
                                }
                                if ui
                                    .selectable_label(settings.theme == ThemeMode::Light, "Claro")
                                    .clicked()
                                {
                                    actions.push(UiAction::SetTheme(ThemeMode::Light));
                                }
                            });
                            ui.end_row();

                            ui.label("Sistema por defecto:");
                            egui::ComboBox::from_id_salt("pref_sys")
                                .selected_text(settings.default_unit_system.label())
                                .show_ui(ui, |ui| {
                                    for sys in [UnitSystem::Metric, UnitSystem::Imperial] {
                                        if ui
                                            .selectable_value(
                                                &mut settings.default_unit_system,
                                                sys,
                                                sys.label(),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Unidad láminas:");
                            egui::ComboBox::from_id_salt("pref_sheet_u")
                                .selected_text(match settings.sheet_display_unit {
                                    SheetDisplayUnit::Millimeters => "Milímetros (mm)",
                                    SheetDisplayUnit::Centimeters => "Centímetros (cm)",
                                    SheetDisplayUnit::Inches => "Pulgadas (in)",
                                })
                                .show_ui(ui, |ui| {
                                    for (u, lbl) in [
                                        (SheetDisplayUnit::Millimeters, "Milímetros (mm)"),
                                        (SheetDisplayUnit::Centimeters, "Centímetros (cm)"),
                                        (SheetDisplayUnit::Inches, "Pulgadas (in)"),
                                    ] {
                                        if ui
                                            .selectable_value(
                                                &mut settings.sheet_display_unit,
                                                u,
                                                lbl,
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Separador CSV:");
                            egui::ComboBox::from_id_salt("pref_csv")
                                .selected_text(settings.csv_delimiter.label())
                                .show_ui(ui, |ui| {
                                    for d in [CsvDelimiter::Semicolon, CsvDelimiter::Comma] {
                                        if ui
                                            .selectable_value(
                                                &mut settings.csv_delimiter,
                                                d,
                                                d.label(),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Paso de grilla (m):");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut settings.grid_spacing_meters)
                                        .range(0.05..=50.0)
                                        .speed(0.05)
                                        .suffix(" m"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            ui.end_row();
                        });

                    if changed {
                        settings.save();
                    }
                    ui.separator();
                    if ui.button("Cerrar").clicked() {
                        close_clicked = true;
                    }
                });
        }

        ActiveModal::Shortcuts => {
            egui::Window::new("Atajos de teclado RedBeam")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    let shortcuts = [
                        ("V / Esc", "Seleccionar / Cancelar"),
                        ("N", "Nota adhesiva"),
                        ("T", "Cuadro de texto"),
                        ("W", "Máquina de escribir"),
                        ("Q", "Llamada con flecha (Callout)"),
                        ("C", "Nube de revisión"),
                        ("K", "Nube+ (Nube + Llamada)"),
                        ("A", "Flecha"),
                        ("L", "Línea"),
                        ("R / E / P", "Rectángulo / Elipse / Polígono"),
                        ("H / S", "Resaltador / Sello"),
                        ("Shift+L / Shift+A / Shift+V", "Medir Longitud / Área / Volumen"),
                        ("G", "Instantánea (captura región PNG al portapapeles)"),
                        ("1 .. 8", "Herramientas rápidas de Mis herramientas"),
                        ("Shift (al dibujar)", "Bloqueo ortogonal 45° / 90°"),
                        ("Alt + arrastrar esquina de nube", "Editar solo ese vértice"),
                        ("Doble clic en texto", "Editar texto directamente en el plano"),
                        ("Ctrl+Enter / Esc al editar texto", "Guardar / Cancelar"),
                        ("Espacio + Arrastrar / Rueda", "Mover plano / Zoom al cursor"),
                        ("Ctrl+O / Ctrl+S / Ctrl+Z / Ctrl+Y", "Abrir / Guardar / Deshacer / Rehacer"),
                        ("Ctrl+K", "Paleta de comandos rápida"),
                        ("Ctrl+0 / Ctrl+1 / Ctrl++ / Ctrl+-", "Ajustar página / 100% / Zoom"),
                        ("RePág / AvPág", "Página anterior / Página siguiente"),
                    ];
                    egui::Grid::new("sc_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (k, d) in shortcuts {
                                ui.label(RichText::new(k).strong().color(theme.accent_cyan));
                                ui.label(d);
                                ui.end_row();
                            }
                        });
                    ui.separator();
                    if ui.button("Cerrar").clicked() {
                        close_clicked = true;
                    }
                });
        }

        ActiveModal::CommandPalette {
            query,
            selected,
            focus_requested,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            let has_doc = tab.is_some();
            let cmds = build_palette_commands(has_doc, settings.theme);
            let q = query.trim().to_lowercase();
            let filtered: Vec<&PaletteCommand> = cmds
                .iter()
                .filter(|c| {
                    q.is_empty()
                        || c.label.to_lowercase().contains(&q)
                        || c.category.to_lowercase().contains(&q)
                        || c.shortcut.to_lowercase().contains(&q)
                })
                .collect();

            if !filtered.is_empty() {
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    *selected = (*selected + 1) % filtered.len();
                } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    *selected = (*selected + filtered.len() - 1) % filtered.len();
                }
                if *selected >= filtered.len() {
                    *selected = 0;
                }
            } else {
                *selected = 0;
            }

            let enter_hit = ctx.input(|i| i.key_pressed(egui::Key::Enter));

            egui::Window::new("Paleta de comandos (Ctrl+K)")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size([470.0, 360.0])
                .anchor(egui::Align2::CENTER_TOP, [0.0, 74.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⌘").strong().size(14.0).color(theme.accent_red));
                        let te = ui.add(
                            egui::TextEdit::singleline(query)
                                .hint_text("Buscar herramienta, escala (ej. 1:50), o acción...")
                                .desired_width(f32::INFINITY),
                        );
                        if *focus_requested {
                            te.request_focus();
                            *focus_requested = false;
                        }
                    });
                    ui.separator();

                    let mut chosen: Option<UiAction> = None;
                    egui::ScrollArea::vertical()
                        .id_salt("cmd_palette_scroll")
                        .max_height(285.0)
                        .show(ui, |ui| {
                            if filtered.is_empty() {
                                ui.label(
                                    RichText::new("Sin comandos coincidentes")
                                        .color(theme.text_muted),
                                );
                            }
                            for (i, cmd) in filtered.iter().enumerate() {
                                let is_sel = i == *selected;
                                let bg = if is_sel {
                                    theme.accent_red_soft
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                let resp = egui::Frame::new()
                                    .fill(bg)
                                    .corner_radius(egui::CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 5))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(cmd.category)
                                                    .size(10.5)
                                                    .color(theme.accent_cyan),
                                            );
                                            ui.label(
                                                RichText::new(cmd.label)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(theme.text_primary),
                                            );
                                            if !cmd.shortcut.is_empty() {
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            RichText::new(cmd.shortcut)
                                                                .size(10.5)
                                                                .color(theme.text_muted),
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                    })
                                    .response;
                                if resp.interact(egui::Sense::click()).clicked() {
                                    chosen = Some(cmd.action.clone());
                                }
                                if is_sel && enter_hit {
                                    chosen = Some(cmd.action.clone());
                                }
                            }
                        });
                    if let Some(act) = chosen {
                        actions.push(act);
                        close_clicked = true;
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("↑/↓ navegar · Enter ejecutar · Esc cerrar")
                            .size(10.5)
                            .color(theme.text_muted),
                    );
                });
        }

        ActiveModal::About => {
            egui::Window::new("Acerca de RedBeam")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(concat!("RedBeam v", env!("CARGO_PKG_VERSION")))
                            .strong()
                            .size(18.0)
                            .color(theme.accent_red),
                    );
                    ui.label("Visor, marcado y cubicación de planos PDF nativo para Windows en Rust.");
                    ui.add_space(4.0);
                    ui.label("• Enfoque por defecto en Sistema Métrico Chileno (m, cm, mm, m², m³, A0–A4)");
                    ui.label("• Soporte completo para unidades imperiales EE.UU. (ft-in, sf, cu ft)");
                    ui.label("• Sin nube obligatoria, sin suscripciones, sin publicidad ni distracciones.");
                    ui.separator();
                    if ui.button("Cerrar").clicked() {
                        close_clicked = true;
                    }
                });
        }
        ActiveModal::ConfirmClose { tab_idx } => {
            let prompt = if tab_idx.is_some() {
                "Este plano tiene cambios sin guardar. ¿Qué desea hacer antes de cerrarlo?"
            } else {
                "Hay planos con cambios sin guardar. ¿Desea guardar o descartar los cambios antes de salir?"
            };
            egui::Window::new("Cambios sin guardar")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.label(RichText::new(prompt).size(12.5));
                    ui.add_space(10.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                RichText::new("Guardar y continuar")
                                    .strong()
                                    .color(theme.accent_green),
                            )
                            .clicked()
                        {
                            if let Some(idx) = tab_idx {
                                actions.push(UiAction::SelectTab(*idx));
                                actions.push(UiAction::SaveDocument);
                                actions.push(UiAction::ForceCloseTab(*idx));
                            } else {
                                actions.push(UiAction::SaveDocument);
                                actions.push(UiAction::ForceExitApp);
                            }
                            close_clicked = true;
                        }
                        if ui
                            .button(
                                RichText::new("Descartar cambios")
                                    .color(theme.accent_red),
                            )
                            .clicked()
                        {
                            if let Some(idx) = tab_idx {
                                actions.push(UiAction::ForceCloseTab(*idx));
                            } else {
                                actions.push(UiAction::ForceExitApp);
                            }
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::EditPageLabel {
            page_index,
            label_buf,
            focus_requested,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new(format!("Etiqueta de Lámina {}", *page_index + 1))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(340.0);
                    ui.label(
                        RichText::new(
                            "Asigne un código o nombre a esta lámina (ej: ARQ-01 · Planta Cubierta). Déjelo vacío para usar la numeración por defecto.",
                        )
                        .size(11.5)
                        .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);
                    let te = ui.add(
                        egui::TextEdit::singleline(label_buf)
                            .hint_text("Ej: ARQ-01 · Planta Piso 1")
                            .desired_width(f32::INFINITY),
                    );
                    if *focus_requested {
                        te.request_focus();
                        *focus_requested = false;
                    }
                    let enter_commit =
                        te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Guardar").strong().color(theme.accent_green))
                            .clicked()
                            || enter_commit
                        {
                            actions.push(UiAction::RenamePageLabel {
                                page_index: *page_index,
                                label: label_buf.trim().to_string(),
                            });
                            close_clicked = true;
                        }
                        if ui.button("Limpiar").clicked() {
                            actions.push(UiAction::RenamePageLabel {
                                page_index: *page_index,
                                label: String::new(),
                            });
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::ManageCustomColumns {
            new_name,
            new_kind_idx,
            new_formula,
            new_choices_csv,
            new_prefix,
            new_suffix,
            new_decimals,
            new_include_totals,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new("Columnas personalizadas y Fórmulas (Cubicación)")
                .collapsible(false)
                .resizable(true)
                .default_width(540.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    let Some(tab) = tab else {
                        ui.label("Sin documento activo.");
                        return;
                    };

                    ui.label(
                        RichText::new("Columnas actuales del documento:")
                            .strong()
                            .size(12.5),
                    );
                    let mut remove_id: Option<Uuid> = None;
                    let mut visibility_toggled = false;
                    egui::ScrollArea::vertical()
                        .id_salt("manage_cc_list")
                        .max_height(175.0)
                        .show(ui, |ui| {
                            if tab.custom_columns.is_empty() {
                                ui.label(
                                    RichText::new("Sin columnas personalizadas aún.")
                                        .color(theme.text_muted),
                                );
                            }
                            for col in &mut tab.custom_columns {
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut col.visible, "").changed() {
                                        visibility_toggled = true;
                                    }
                                    ui.label(RichText::new(&col.name).strong().size(12.0));
                                    ui.label(
                                        RichText::new(format!("[{}]", col.kind.type_label_es()))
                                            .size(11.0)
                                            .color(theme.accent_cyan),
                                    );
                                    if let CustomColumnKind::Formula { expression, .. } = &col.kind
                                    {
                                        ui.label(
                                            RichText::new(format!("= {expression}"))
                                                .size(11.0)
                                                .color(theme.text_secondary),
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("Eliminar")
                                                .clicked()
                                            {
                                                remove_id = Some(col.id);
                                            }
                                        },
                                    );
                                });
                            }
                        });
                    if visibility_toggled {
                        tab.dirty = true;
                    }
                    if let Some(rid) = remove_id {
                        tab.push_undo();
                        tab.custom_columns.retain(|c| c.id != rid);
                        for m in &mut tab.markups {
                            m.custom_values.remove(&rid);
                        }
                    }

                    ui.separator();
                    ui.label(RichText::new("Agregar nueva columna").strong().size(12.5));
                    egui::Grid::new("add_cc_grid")
                        .num_columns(2)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Nombre:");
                            ui.add(
                                egui::TextEdit::singleline(new_name)
                                    .hint_text("Ej: Total c/IVA, Sector, Factor...")
                                    .desired_width(320.0),
                            );
                            ui.end_row();

                            ui.label("Tipo:");
                            let kind_labels = [
                                "Texto",
                                "Número",
                                "Lista de opciones (con valor opcional)",
                                "Casilla (Sí/No)",
                                "Fórmula aritmética",
                            ];
                            egui::ComboBox::from_id_salt("add_cc_kind")
                                .selected_text(kind_labels[(*new_kind_idx).min(4)])
                                .width(260.0)
                                .show_ui(ui, |ui| {
                                    for (i, lbl) in kind_labels.iter().enumerate() {
                                        ui.selectable_value(new_kind_idx, i, *lbl);
                                    }
                                });
                            ui.end_row();

                            if *new_kind_idx == 2 {
                                ui.label("Opciones (coma):");
                                ui.add(
                                    egui::TextEdit::singleline(new_choices_csv)
                                        .hint_text("Ej: 1.00 = Normal, 1.05 = 5%, 1.10 = 10%")
                                        .desired_width(320.0),
                                );
                                ui.end_row();
                            }
                            if *new_kind_idx == 4 {
                                ui.label("Fórmula:");
                                ui.add(
                                    egui::TextEdit::singleline(new_formula)
                                        .hint_text("Ej: Medicion * PrecioUnitario * 1.19")
                                        .desired_width(320.0),
                                );
                                ui.end_row();
                            }
                            if *new_kind_idx == 1 || *new_kind_idx == 4 {
                                ui.label("Formato:");
                                ui.horizontal(|ui| {
                                    ui.label("Pref:");
                                    ui.add(
                                        egui::TextEdit::singleline(new_prefix)
                                            .desired_width(48.0),
                                    );
                                    ui.label("Suf:");
                                    ui.add(
                                        egui::TextEdit::singleline(new_suffix)
                                            .desired_width(48.0),
                                    );
                                    ui.label("Dec:");
                                    ui.add(
                                        egui::DragValue::new(new_decimals).range(0..=4),
                                    );
                                    ui.checkbox(new_include_totals, "Sumar en TOTALES");
                                });
                                ui.end_row();
                            }
                        });

                    if *new_kind_idx == 4 {
                        ui.label(
                            RichText::new(
                                "Variables: Medicion, Area, Longitud, Perimetro, Volumen, AreaMuro, Conteo, Profundidad, PrecioUnitario, o nombres de otras columnas. Funciones: round(), ceil(), floor(), abs(), min(), max(), if(cond, a, b).",
                            )
                            .size(10.5)
                            .color(theme.text_muted),
                        );
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let can_add = !new_name.trim().is_empty()
                            && (*new_kind_idx != 4 || !new_formula.trim().is_empty());
                        if ui
                            .add_enabled(
                                can_add,
                                egui::Button::new(
                                    RichText::new("+ Agregar columna")
                                        .strong()
                                        .color(theme.accent_green),
                                ),
                            )
                            .clicked()
                        {
                            let kind = match *new_kind_idx {
                                0 => CustomColumnKind::Text {
                                    default_value: String::new(),
                                },
                                1 => CustomColumnKind::Number {
                                    default_value: 0.0,
                                    prefix: new_prefix.clone(),
                                    suffix: new_suffix.clone(),
                                    decimals: *new_decimals as usize,
                                    include_in_totals: *new_include_totals,
                                },
                                2 => {
                                    let opts: Vec<ChoiceOption> = new_choices_csv
                                        .split(',')
                                        .filter_map(|raw| {
                                            let s = raw.trim();
                                            if s.is_empty() {
                                                return None;
                                            }
                                            if let Some((left, right)) = s.split_once('=') {
                                                let num = left.trim().replace(',', ".").parse::<f64>().ok();
                                                let lbl = right.trim();
                                                Some(ChoiceOption {
                                                    label: if lbl.is_empty() {
                                                        s.to_string()
                                                    } else {
                                                        lbl.to_string()
                                                    },
                                                    numeric_value: num,
                                                })
                                            } else {
                                                Some(ChoiceOption {
                                                    label: s.to_string(),
                                                    numeric_value: s.replace(',', ".").parse::<f64>().ok(),
                                                })
                                            }
                                        })
                                        .collect();
                                    CustomColumnKind::Choice {
                                        items: opts,
                                        allow_custom_text: true,
                                    }
                                }
                                3 => CustomColumnKind::Checkmark {
                                    default_value: false,
                                },
                                _ => CustomColumnKind::Formula {
                                    expression: new_formula.trim().to_string(),
                                    prefix: new_prefix.clone(),
                                    suffix: new_suffix.clone(),
                                    decimals: *new_decimals as usize,
                                    include_in_totals: *new_include_totals,
                                },
                            };
                            tab.push_undo();
                            tab.custom_columns.push(CustomColumnDef {
                                id: Uuid::new_v4(),
                                name: new_name.trim().to_string(),
                                kind,
                                visible: true,
                            });
                            new_name.clear();
                        }
                        if ui.button("Cerrar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::OverlayPdfs {
            pdf_b_path,
            color_a,
            color_b,
            align_mode_idx,
            all_pages,
            manual_dx_mm,
            manual_dy_mm,
            manual_angle_deg,
            manual_scale,
            pts_a,
            pts_b,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new("Superponer planos (Overlay + Alineación)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(470.0);
                    ui.label(
                        RichText::new(
                            "Combina el plano actual (Rev A) y otro PDF (Rev B) con colores distintos y fusión Multiplicar.",
                        )
                        .size(11.5)
                        .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("Plano Rev B:");
                        let path_lbl = pdf_b_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("Sin seleccionar...");
                        ui.label(RichText::new(path_lbl).strong().color(theme.accent_cyan));
                        if ui.button("Elegir PDF...").clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter("Documento PDF", &["pdf"])
                                .pick_file()
                        {
                            *pdf_b_path = Some(p);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Color Rev A:");
                        ui.color_edit_button_srgb(color_a);
                        ui.add_space(12.0);
                        ui.label("Color Rev B:");
                        ui.color_edit_button_srgb(color_b);
                        ui.add_space(12.0);
                        ui.checkbox(all_pages, "Todas las páginas comunes");
                    });

                    ui.separator();
                    let align_labels = [
                        "Auto-alinear por correlación de tinta (Recomendado)",
                        "Sin ajuste (1:1 exacto)",
                        "Desplazamiento / rotación / escala manual",
                        "Alineación afín de 3 puntos (esquinas homólogas)",
                    ];
                    ui.horizontal(|ui| {
                        ui.label("Alineación:");
                        egui::ComboBox::from_id_salt("overlay_align_mode")
                            .selected_text(align_labels[(*align_mode_idx).min(3)])
                            .width(340.0)
                            .show_ui(ui, |ui| {
                                for (i, lbl) in align_labels.iter().enumerate() {
                                    ui.selectable_value(align_mode_idx, i, *lbl);
                                }
                            });
                    });

                    if *align_mode_idx == 2 {
                        egui::Grid::new("overlay_manual_grid")
                            .num_columns(4)
                            .spacing([8.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("ΔX (mm):");
                                ui.add(egui::DragValue::new(manual_dx_mm).speed(0.5));
                                ui.label("ΔY (mm):");
                                ui.add(egui::DragValue::new(manual_dy_mm).speed(0.5));
                                ui.end_row();
                                ui.label("Ángulo (°):");
                                ui.add(
                                    egui::DragValue::new(manual_angle_deg)
                                        .range(-180.0..=180.0)
                                        .speed(0.1),
                                );
                                ui.label("Escala:");
                                ui.add(
                                    egui::DragValue::new(manual_scale)
                                        .range(0.1..=10.0)
                                        .speed(0.01),
                                );
                                ui.end_row();
                            });
                    } else if *align_mode_idx == 3 {
                        ui.label(
                            RichText::new("Coordenadas en puntos PDF (pt) de 3 puntos homólogos:")
                                .size(11.0)
                                .color(theme.text_muted),
                        );
                        egui::Grid::new("overlay_3pt_grid")
                            .num_columns(5)
                            .spacing([6.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("");
                                ui.label("A.x");
                                ui.label("A.y");
                                ui.label("B.x");
                                ui.label("B.y");
                                ui.end_row();
                                for i in 0..3 {
                                    ui.label(format!("P{}:", i + 1));
                                    ui.add(egui::DragValue::new(&mut pts_a[i].0).speed(1.0));
                                    ui.add(egui::DragValue::new(&mut pts_a[i].1).speed(1.0));
                                    ui.add(egui::DragValue::new(&mut pts_b[i].0).speed(1.0));
                                    ui.add(egui::DragValue::new(&mut pts_b[i].1).speed(1.0));
                                    ui.end_row();
                                }
                            });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        let can_run = pdf_b_path.is_some();
                        if ui
                            .add_enabled(
                                can_run,
                                egui::Button::new(
                                    RichText::new("Generar Overlay")
                                        .strong()
                                        .color(theme.accent_green),
                                ),
                            )
                            .clicked()
                            && let Some(path_b) = pdf_b_path.clone()
                        {
                            let alignment = match *align_mode_idx {
                                0 => OverlayAlignmentMode::AutoTranslate,
                                1 => OverlayAlignmentMode::None,
                                2 => OverlayAlignmentMode::Manual {
                                    dx_pt: *manual_dx_mm * PT_PER_MM,
                                    dy_pt: *manual_dy_mm * PT_PER_MM,
                                    angle_deg: *manual_angle_deg,
                                    scale: *manual_scale,
                                },
                                _ => OverlayAlignmentMode::ThreePoint {
                                    pts_a: [
                                        Point2::new(pts_a[0].0, pts_a[0].1),
                                        Point2::new(pts_a[1].0, pts_a[1].1),
                                        Point2::new(pts_a[2].0, pts_a[2].1),
                                    ],
                                    pts_b: [
                                        Point2::new(pts_b[0].0, pts_b[0].1),
                                        Point2::new(pts_b[1].0, pts_b[1].1),
                                        Point2::new(pts_b[2].0, pts_b[2].1),
                                    ],
                                },
                            };
                            actions.push(UiAction::RunOverlay {
                                pdf_b_path: path_b,
                                color_a: *color_a,
                                color_b: *color_b,
                                alignment,
                                all_pages: *all_pages,
                            });
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::VisualSearchResults {
            result,
            threshold,
            subject_name,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            let matching_indices: Vec<usize> = result
                .matches
                .iter()
                .enumerate()
                .filter(|(_, m)| m.score >= *threshold)
                .map(|(i, _)| i)
                .collect();
            let active_count = matching_indices
                .iter()
                .filter(|&&i| result.matches[i].selected)
                .count();

            egui::Window::new("Búsqueda visual — Conteo automático")
                .collapsible(false)
                .resizable(true)
                .default_width(430.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Asunto del conteo:");
                        ui.add(
                            egui::TextEdit::singleline(subject_name)
                                .hint_text("Ej: Luminaria LED, Enchufe doble...")
                                .desired_width(240.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Sensibilidad mínima:");
                        ui.add(
                            egui::Slider::new(threshold, 0.55..=0.98)
                                .fixed_decimals(2)
                                .show_value(true),
                        );
                    });
                    ui.label(
                        RichText::new(format!(
                            "Coincidencias filtradas: {} (seleccionadas: {})",
                            matching_indices.len(),
                            active_count
                        ))
                        .strong()
                        .color(theme.accent_cyan),
                    );
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_salt("vsearch_matches_scroll")
                        .max_height(210.0)
                        .show(ui, |ui| {
                            if matching_indices.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "No hay coincidencias sobre este umbral; baje la sensibilidad.",
                                    )
                                    .color(theme.text_muted),
                                );
                            }
                            for idx in &matching_indices {
                                let m = &mut result.matches[*idx];
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut m.selected, "");
                                    ui.label(format!(
                                        "#{:02} · Similitud {:.0}% · Rot {}° · ({:.0}, {:.0}) pt",
                                        idx + 1,
                                        m.score * 100.0,
                                        m.rotation_deg,
                                        m.center_pt.x,
                                        m.center_pt.y
                                    ));
                                });
                            }
                        });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                active_count > 0,
                                egui::Button::new(
                                    RichText::new(format!(
                                        "Crear Conteo automático ({active_count} und)"
                                    ))
                                    .strong()
                                    .color(theme.accent_green),
                                ),
                            )
                            .clicked()
                        {
                            let pts: Vec<Point2> = matching_indices
                                .iter()
                                .filter_map(|&i| {
                                    let m = &result.matches[i];
                                    m.selected.then_some(m.center_pt)
                                })
                                .collect();
                            actions.push(UiAction::CreateCountFromVisualSearch {
                                page_index: result.page_index,
                                subject: if subject_name.trim().is_empty() {
                                    "Conteo visual".to_string()
                                } else {
                                    subject_name.trim().to_string()
                                },
                                points: pts,
                            });
                            close_clicked = true;
                        }
                        if ui.button("Cerrar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::HeadersAndFooters { cfg } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new("Encabezados y Pies de Página (Vector PDF)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(510.0);
                    ui.label(
                        RichText::new(
                            "Variables dinámicas: <<Page>>, <<TotalPages>>, <<Label>>, <<Date>>, <<File>>",
                        )
                        .size(11.0)
                        .color(theme.accent_cyan),
                    );
                    ui.add_space(6.0);
                    egui::Grid::new("hf_slots_grid")
                        .num_columns(3)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Superior Izq.").size(11.0).strong());
                            ui.label(RichText::new("Superior Centro").size(11.0).strong());
                            ui.label(RichText::new("Superior Der.").size(11.0).strong());
                            ui.end_row();
                            ui.add(egui::TextEdit::singleline(&mut cfg.top_left).desired_width(155.0));
                            ui.add(egui::TextEdit::singleline(&mut cfg.top_center).desired_width(155.0));
                            ui.add(egui::TextEdit::singleline(&mut cfg.top_right).desired_width(155.0));
                            ui.end_row();

                            ui.label(RichText::new("Inferior Izq.").size(11.0).strong());
                            ui.label(RichText::new("Inferior Centro").size(11.0).strong());
                            ui.label(RichText::new("Inferior Der.").size(11.0).strong());
                            ui.end_row();
                            ui.add(egui::TextEdit::singleline(&mut cfg.bottom_left).desired_width(155.0));
                            ui.add(egui::TextEdit::singleline(&mut cfg.bottom_center).desired_width(155.0));
                            ui.add(egui::TextEdit::singleline(&mut cfg.bottom_right).desired_width(155.0));
                            ui.end_row();
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Tamaño (pt):");
                        ui.add(egui::DragValue::new(&mut cfg.font_size_pt).range(6.0..=28.0).speed(0.5));
                        ui.label("Margen (mm):");
                        ui.add(egui::DragValue::new(&mut cfg.margin_mm).range(4.0..=40.0).speed(0.5));
                        ui.label("Color:");
                        ui.color_edit_button_srgb(&mut cfg.color);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Rango de páginas (vacío = todas):");
                        ui.add(
                            egui::TextEdit::singleline(&mut cfg.page_range)
                                .hint_text("Ej: 1-3, 5")
                                .desired_width(160.0),
                        );
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Aplicar").strong().color(theme.accent_green))
                            .clicked()
                        {
                            actions.push(UiAction::ApplyHeadersFooters(cfg.clone()));
                            close_clicked = true;
                        }
                        if ui
                            .button(RichText::new("Quitar existentes").color(theme.accent_amber))
                            .clicked()
                        {
                            actions.push(UiAction::RemoveHeadersFooters);
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::BatchProcess {
            input_files,
            output_dir,
            flatten_markups,
            flatten_recoverable,
            apply_headers_footers,
            hf_cfg,
            apply_stamp,
            stamp_title,
            stamp_subtitle,
            stamp_color,
            stamp_placement,
            export_consolidated_csv,
            csv_delimiter,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new("Procesamiento por Lotes (Batch Local Offline)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(500.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Archivos PDF ({}):", input_files.len())).strong());
                        if ui.button("+ Seleccionar PDFs...").clicked()
                            && let Some(files) = rfd::FileDialog::new()
                                .add_filter("Documentos PDF", &["pdf"])
                                .pick_files()
                        {
                            for f in files {
                                if !input_files.contains(&f) {
                                    input_files.push(f);
                                }
                            }
                        }
                        if !input_files.is_empty() && ui.small_button("Limpiar").clicked() {
                            input_files.clear();
                        }
                    });
                    if !input_files.is_empty() {
                        egui::ScrollArea::vertical()
                            .id_salt("batch_files_scroll")
                            .max_height(80.0)
                            .show(ui, |ui| {
                                for f in input_files.iter() {
                                    let name = f
                                        .file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("PDF");
                                    ui.label(RichText::new(format!("• {name}")).size(11.0));
                                }
                            });
                    }

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Carpeta de salida:").strong());
                        let out_lbl = output_dir
                            .as_ref()
                            .and_then(|p| p.to_str())
                            .unwrap_or("Sin seleccionar...");
                        ui.label(RichText::new(out_lbl).size(11.0).color(theme.accent_cyan));
                        if ui.button("Elegir carpeta...").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            *output_dir = Some(dir);
                        }
                    });

                    ui.separator();
                    ui.checkbox(flatten_markups, "Aplanar marcas en los PDFs de salida");
                    if *flatten_markups {
                        ui.indent("batch_flat_ind", |ui| {
                            ui.checkbox(
                                flatten_recoverable,
                                "Permitir desaplanado posterior (Unflatten)",
                            );
                        });
                    }
                    ui.checkbox(
                        apply_headers_footers,
                        "Aplicar pie de página automático (Archivo · Lámina · Pág. X/Y)",
                    );
                    if *apply_headers_footers {
                        ui.indent("batch_hf_ind", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Pie Der.:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut hf_cfg.bottom_right)
                                        .desired_width(220.0),
                                );
                            });
                        });
                    }
                    ui.checkbox(apply_stamp, "Aplicar sello en lote a todas las páginas");
                    if *apply_stamp {
                        ui.indent("batch_stamp_ind", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Título:");
                                ui.add(egui::TextEdit::singleline(stamp_title).desired_width(160.0));
                                ui.label("Color:");
                                ui.color_edit_button_srgb(stamp_color);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Subtítulo:");
                                ui.add(
                                    egui::TextEdit::singleline(stamp_subtitle).desired_width(150.0),
                                );
                                egui::ComboBox::from_id_salt("batch_stamp_pos")
                                    .selected_text(stamp_placement.label_es())
                                    .width(165.0)
                                    .show_ui(ui, |ui| {
                                        for pos in [
                                            BatchStampPlacement::TopRight,
                                            BatchStampPlacement::BottomRight,
                                            BatchStampPlacement::TopLeft,
                                            BatchStampPlacement::Center,
                                        ] {
                                            ui.selectable_value(
                                                stamp_placement,
                                                pos,
                                                pos.label_es(),
                                            );
                                        }
                                    });
                            });
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.checkbox(
                            export_consolidated_csv,
                            "Exportar CSV consolidado de cubicación",
                        );
                        if *export_consolidated_csv {
                            ui.radio_value(csv_delimiter, CsvDelimiter::Semicolon, "Chile (;)");
                            ui.radio_value(csv_delimiter, CsvDelimiter::Comma, "Estándar (,)");
                        }
                    });

                    ui.separator();
                    let can_run = !input_files.is_empty() && output_dir.is_some();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                can_run,
                                egui::Button::new(
                                    RichText::new("Ejecutar lote local")
                                        .strong()
                                        .color(theme.accent_green),
                                ),
                            )
                            .clicked()
                            && let Some(dir) = output_dir.clone()
                        {
                            actions.push(UiAction::RunBatchJob(BatchJobConfig {
                                input_files: input_files.clone(),
                                output_dir: dir,
                                flatten_markups: *flatten_markups,
                                flatten_recoverable: *flatten_recoverable,
                                apply_headers_footers: *apply_headers_footers,
                                headers_footers: hf_cfg.clone(),
                                apply_stamp: *apply_stamp,
                                stamp_title: stamp_title.clone(),
                                stamp_subtitle: stamp_subtitle.clone(),
                                stamp_color: *stamp_color,
                                stamp_placement: *stamp_placement,
                                author_name: settings.author_name.clone(),
                                export_consolidated_csv: *export_consolidated_csv,
                                csv_delimiter: *csv_delimiter,
                            }));
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::ConfigureSequence {
            item_id,
            initialized,
            enabled,
            prefix,
            suffix,
            next_value,
            step,
            pad_digits,
            style,
        } => {
            if !*initialized {
                *initialized = true;
                for set in &tool_chest.sets {
                    if let Some(it) = set.items.iter().find(|i| i.id == *item_id) {
                        if let Some(ref seq) = it.sequence {
                            *enabled = true;
                            *prefix = seq.prefix.clone();
                            *suffix = seq.suffix.clone();
                            *next_value = seq.next_value;
                            *step = seq.step;
                            *pad_digits = seq.pad_digits;
                            *style = seq.style;
                        } else {
                            *enabled = false;
                        }
                        break;
                    }
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new("Secuencia automática de herramienta")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.checkbox(enabled, "Activar numeración autoincremental al colocar marca");
                    ui.add_enabled_ui(*enabled, |ui| {
                        egui::Grid::new("seq_cfg_grid")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label("Prefijo:");
                                ui.add(egui::TextEdit::singleline(prefix).desired_width(140.0));
                                ui.end_row();
                                ui.label("Sufijo:");
                                ui.add(egui::TextEdit::singleline(suffix).desired_width(140.0));
                                ui.end_row();
                                ui.label("Estilo:");
                                ui.horizontal(|ui| {
                                    ui.radio_value(style, SequenceStyle::Numeric, "1, 2, 3");
                                    ui.radio_value(style, SequenceStyle::AlphaUpper, "A, B, C");
                                    ui.radio_value(style, SequenceStyle::AlphaLower, "a, b, c");
                                });
                                ui.end_row();
                                ui.label("Siguiente valor:");
                                ui.add(egui::DragValue::new(next_value).range(1..=999_999));
                                ui.end_row();
                                ui.label("Incremento:");
                                ui.add(egui::DragValue::new(step).range(1..=100));
                                ui.end_row();
                                if *style == SequenceStyle::Numeric {
                                    ui.label("Ceros (dígitos):");
                                    ui.add(egui::DragValue::new(pad_digits).range(0..=6));
                                    ui.end_row();
                                }
                            });
                        let mut preview_seq = SequenceSpec {
                            prefix: prefix.clone(),
                            suffix: suffix.clone(),
                            next_value: *next_value,
                            step: *step,
                            pad_digits: *pad_digits,
                            style: *style,
                        };
                        let p1 = preview_seq.take_next();
                        let p2 = preview_seq.take_next();
                        let p3 = preview_seq.take_next();
                        ui.label(
                            RichText::new(format!("Vista previa: {p1}  →  {p2}  →  {p3}"))
                                .strong()
                                .color(theme.accent_cyan),
                        );
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Guardar").strong().color(theme.accent_green))
                            .clicked()
                        {
                            for set in &mut tool_chest.sets {
                                if let Some(it) = set.items.iter_mut().find(|i| i.id == *item_id) {
                                    it.sequence = if *enabled {
                                        Some(SequenceSpec {
                                            prefix: prefix.clone(),
                                            suffix: suffix.clone(),
                                            next_value: *next_value,
                                            step: (*step).max(1),
                                            pad_digits: *pad_digits,
                                            style: *style,
                                        })
                                    } else {
                                        None
                                    };
                                    break;
                                }
                            }
                            tool_chest.save();
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::PhotoPreview {
            attachment_id,
            caption,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            egui::Window::new(format!("Foto de terreno — {caption}"))
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    if let Some(tex) = image_textures.get(attachment_id) {
                        let sz = tex.size_vec2();
                        let scale = (680.0 / sz.x.max(1.0)).min(520.0 / sz.y.max(1.0)).min(1.0);
                        ui.image((tex.id(), sz * scale));
                    } else {
                        ui.label("Cargando imagen...");
                    }
                    ui.separator();
                    if ui.button("Cerrar").clicked() {
                        close_clicked = true;
                    }
                });
        }
        ActiveModal::ApplyRedactionsConfirm { fill_color } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            let pending_count = tab
                .as_deref()
                .map(|t| {
                    t.markups
                        .iter()
                        .filter(|m| matches!(m.kind, MarkupKind::Redaction { .. }))
                        .count()
                })
                .unwrap_or(0);
            egui::Window::new("Aplicar Redacciones (Censura Destructiva)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(400.0);
                    ui.label(
                        RichText::new(format!(
                            "Se aplicarán {pending_count} zona(s) de redacción de forma PERMANENTE e IRREVERSIBLE."
                        ))
                        .strong()
                        .color(theme.accent_red),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Las páginas marcadas se convertirán en imágenes con las zonas cubiertas. Revise también los comentarios, adjuntos y metadatos antes de compartir el PDF.",
                        )
                        .size(11.5)
                        .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Color de relleno de censura:");
                        ui.color_edit_button_srgb(fill_color);
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                pending_count > 0,
                                egui::Button::new(
                                    RichText::new("Aplicar censura permanente")
                                        .strong()
                                        .color(theme.accent_red),
                                ),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::ApplyRedactions {
                                fill_color: *fill_color,
                            });
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::ColorProcess {
            mode_idx,
            bw_threshold,
            tint_color,
            page_scope_idx,
            page_range,
            max_side_px,
        } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            let cur_page_1 = tab
                .as_deref()
                .map(|t| t.active_page_index() + 1)
                .unwrap_or(1);
            egui::Window::new("Procesamiento de Color de Planos")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(430.0);
                    let mode_labels = [
                        "Escala de grises (Preserva espesores y grises)",
                        "Blanco y Negro puro (Alto contraste para planos/escaneos)",
                        "Colorear líneas del plano (Tinte uniforme)",
                        "Invertir colores (Modo oscuro ↔ claro)",
                    ];
                    ui.label(RichText::new("Modo de color:").strong());
                    for (i, lbl) in mode_labels.iter().enumerate() {
                        ui.radio_value(mode_idx, i, *lbl);
                    }
                    ui.add_space(4.0);
                    if *mode_idx == 1 {
                        ui.horizontal(|ui| {
                            ui.label("Umbral B/N:");
                            ui.add(egui::Slider::new(bw_threshold, 120..=250));
                        });
                    } else if *mode_idx == 2 {
                        ui.horizontal(|ui| {
                            ui.label("Color destino de líneas:");
                            ui.color_edit_button_srgb(tint_color);
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Páginas:");
                        ui.radio_value(page_scope_idx, 0, format!("Actual ({cur_page_1})"));
                        ui.radio_value(page_scope_idx, 1, "Todas");
                        ui.radio_value(page_scope_idx, 2, "Rango");
                    });
                    if *page_scope_idx == 2 {
                        ui.add(
                            egui::TextEdit::singleline(page_range)
                                .hint_text("Ej: 1-3, 5")
                                .desired_width(180.0),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Resolución (px lado mayor):");
                        ui.radio_value(max_side_px, 1600, "1600 (Rápido)");
                        ui.radio_value(max_side_px, 2200, "2200 (CAD)");
                        ui.radio_value(max_side_px, 3000, "3000 (Alta)");
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Aplicar").strong().color(theme.accent_green))
                            .clicked()
                        {
                            let mode = match *mode_idx {
                                0 => ColorProcessMode::Grayscale,
                                1 => ColorProcessMode::BlackAndWhite {
                                    threshold: *bw_threshold,
                                },
                                2 => ColorProcessMode::TintColor { color: *tint_color },
                                _ => ColorProcessMode::Invert,
                            };
                            let range_str = match *page_scope_idx {
                                0 => cur_page_1.to_string(),
                                1 => String::new(),
                                _ => page_range.clone(),
                            };
                            actions.push(UiAction::RunColorProcess(ColorProcessConfig {
                                mode,
                                page_range: range_str,
                                max_side_px: *max_side_px,
                            }));
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
        ActiveModal::ReduceFileSize { preset } => {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                close_clicked = true;
            }
            let cur_kb = tab
                .as_deref()
                .map(|t| t.raw_pdf_bytes.len() as f64 / 1024.0)
                .unwrap_or(0.0);
            egui::Window::new("Reducir Tamaño de Archivo PDF")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.set_min_width(390.0);
                    ui.label(
                        RichText::new(format!("Tamaño actual en memoria: {cur_kb:.1} KB"))
                            .strong()
                            .color(theme.accent_cyan),
                    );
                    ui.label(
                        RichText::new(
                            "Recomprime flujos PDF (Flate), limpia objetos huérfanos y optimiza imágenes raster/fotos incrustadas 100% offline.",
                        )
                        .size(11.5)
                        .color(theme.text_secondary),
                    );
                    ui.add_space(6.0);
                    for opt in [
                        OptimizePreset::HighQuality,
                        OptimizePreset::Balanced,
                        OptimizePreset::MaxCompression,
                    ] {
                        ui.radio_value(preset, opt, opt.label_es());
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new("Optimizar ahora").strong().color(theme.accent_green))
                            .clicked()
                        {
                            actions.push(UiAction::RunReduceFileSize(*preset));
                            close_clicked = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close_clicked = true;
                        }
                    });
                });
        }
    }

    if keep_open && !close_clicked {
        *modal_slot = Some(modal);
    }
}

struct PaletteCommand {
    category: &'static str,
    label: &'static str,
    shortcut: &'static str,
    action: UiAction,
}

fn build_palette_commands(has_doc: bool, theme_mode: ThemeMode) -> Vec<PaletteCommand> {
    use crate::model::tool_chest::ActiveTool;
    let mut out = vec![
        PaletteCommand {
            category: "Archivo",
            label: "Abrir archivo PDF...",
            shortcut: "Ctrl+O",
            action: UiAction::OpenPdfDialog,
        },
        PaletteCommand {
            category: "Archivo",
            label: "Nuevo plano en blanco (ISO A1)",
            shortcut: "",
            action: UiAction::NewBlankDocument,
        },
        PaletteCommand {
            category: "Archivo",
            label: "Abrir plano de ejemplo (Chile 1:100)",
            shortcut: "",
            action: UiAction::OpenDemoChile,
        },
        PaletteCommand {
            category: "Archivo",
            label: "Combinar varios PDFs...",
            shortcut: "",
            action: UiAction::CombinePdfs,
        },
        PaletteCommand {
            category: "Archivo",
            label: "Procesamiento por lotes (Batch local)...",
            shortcut: "",
            action: UiAction::OpenDialog(DialogRequest::BatchProcess),
        },
        PaletteCommand {
            category: "Caja de herramientas",
            label: "Importar conjunto de herramientas (.json)...",
            shortcut: "",
            action: UiAction::ImportToolSet,
        },
    ];

    if has_doc {
        out.extend([
            PaletteCommand {
                category: "Archivo",
                label: "Guardar PDF",
                shortcut: "Ctrl+S",
                action: UiAction::SaveDocument,
            },
            PaletteCommand {
                category: "Archivo",
                label: "Exportar PDF aplanado...",
                shortcut: "",
                action: UiAction::ExportFlattenedPdf,
            },
            PaletteCommand {
                category: "Archivo",
                label: "Exportar Lista de Marcas (CSV Chile ;)",
                shortcut: "",
                action: UiAction::ExportCsv(CsvDelimiter::Semicolon),
            },
            PaletteCommand {
                category: "Documento",
                label: "Comparar con otro PDF...",
                shortcut: "",
                action: UiAction::CompareWithPdf,
            },
            PaletteCommand {
                category: "Documento",
                label: "Superponer planos (Overlay + Alineación)...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::OverlayPdfs),
            },
            PaletteCommand {
                category: "Documento",
                label: "Reemplazar contenido de página actual...",
                shortcut: "",
                action: UiAction::ReplaceCurrentPageContent,
            },
            PaletteCommand {
                category: "Documento",
                label: "Aplanar marcas en documento (Recuperable)",
                shortcut: "",
                action: UiAction::FlattenInPlace { recoverable: true },
            },
            PaletteCommand {
                category: "Documento",
                label: "Desaplanar marcas (Unflatten)",
                shortcut: "",
                action: UiAction::UnflattenDocument,
            },
            PaletteCommand {
                category: "Documento",
                label: "Encabezados y pies de página...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::HeadersAndFooters),
            },
            PaletteCommand {
                category: "Documento",
                label: "Procesamiento de color (B/N, Grises, Tinte)...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::ColorProcess),
            },
            PaletteCommand {
                category: "Documento",
                label: "Reducir tamaño de archivo PDF...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::ReduceFileSize),
            },
            PaletteCommand {
                category: "Documento",
                label: "Aplicar redacciones (Censura destructiva)...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::ApplyRedactionsConfirm),
            },
            PaletteCommand {
                category: "Marcado",
                label: "Adjuntar foto de terreno a marca seleccionada...",
                shortcut: "",
                action: UiAction::AttachPhotoToSelected,
            },
            PaletteCommand {
                category: "Documento",
                label: "Extraer página actual a PDF...",
                shortcut: "",
                action: UiAction::ExtractCurrentPage,
            },
            PaletteCommand {
                category: "Medición",
                label: "Calibrar escala con 2 puntos",
                shortcut: "",
                action: UiAction::StartTwoPointCalibration,
            },
            PaletteCommand {
                category: "Medición",
                label: "Configurar escala y unidades...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::SetScale),
            },
            PaletteCommand {
                category: "Medición",
                label: "Columnas personalizadas y fórmulas...",
                shortcut: "",
                action: UiAction::OpenDialog(DialogRequest::ManageCustomColumns),
            },
            PaletteCommand {
                category: "Vista",
                label: "Alternar resaltado de Espacios / Recintos",
                shortcut: "",
                action: UiAction::ToggleHighlightSpaces,
            },
            PaletteCommand {
                category: "Escala",
                label: "Aplicar escala rápida 1:50 (Métrico Chile)",
                shortcut: "",
                action: UiAction::ApplyPageScale {
                    scale: PageScale::from_metric_ratio(50.0),
                    apply_all: false,
                },
            },
            PaletteCommand {
                category: "Escala",
                label: "Aplicar escala rápida 1:100 (Métrico Chile)",
                shortcut: "",
                action: UiAction::ApplyPageScale {
                    scale: PageScale::from_metric_ratio(100.0),
                    apply_all: false,
                },
            },
            PaletteCommand {
                category: "Escala",
                label: "Aplicar escala rápida 1:200 (Métrico Chile)",
                shortcut: "",
                action: UiAction::ApplyPageScale {
                    scale: PageScale::from_metric_ratio(200.0),
                    apply_all: false,
                },
            },
            PaletteCommand {
                category: "Vista",
                label: "Ajustar página a ventana",
                shortcut: "Ctrl+0",
                action: UiAction::FitPage,
            },
            PaletteCommand {
                category: "Vista",
                label: "Vista dividida vertical",
                shortcut: "",
                action: UiAction::ToggleSplitVertical,
            },
        ]);

        for (cat, tool, sc) in [
            ("Marcado", ActiveTool::Note, "N"),
            ("Marcado", ActiveTool::Callout, "Q"),
            ("Marcado", ActiveTool::Cloud, "C"),
            ("Marcado", ActiveTool::CloudPlus, "K"),
            ("Marcado", ActiveTool::TextBox, "T"),
            ("Marcado", ActiveTool::Arrow, "A"),
            ("Marcado", ActiveTool::Rectangle, "R"),
            ("Marcado", ActiveTool::Polygon, "P"),
            ("Marcado", ActiveTool::Highlight, "H"),
            ("Marcado", ActiveTool::Stamp, "S"),
            ("Marcado", ActiveTool::Snapshot, "G"),
            ("Marcado", ActiveTool::Hyperlink, ""),
            ("Marcado", ActiveTool::Redaction, ""),
            ("Medición", ActiveTool::Space, ""),
            ("Medición", ActiveTool::VisualSearch, ""),
            ("Medición", ActiveTool::MeasureLength, "Shift+L"),
            ("Medición", ActiveTool::MeasurePerimeter, "Shift+P"),
            ("Medición", ActiveTool::MeasureArea, "Shift+A"),
            ("Medición", ActiveTool::MeasureVolume, "Shift+V"),
            ("Medición", ActiveTool::MeasureCount, "Shift+O"),
            ("Medición", ActiveTool::CutoutPoly, ""),
        ] {
            out.push(PaletteCommand {
                category: cat,
                label: tool.label_es(),
                shortcut: sc,
                action: UiAction::SetTool(tool),
            });
        }
    }

    let next_theme = if theme_mode == ThemeMode::Dark {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    out.push(PaletteCommand {
        category: "Vista",
        label: if theme_mode == ThemeMode::Dark {
            "Cambiar a tema Claro"
        } else {
            "Cambiar a tema Oscuro"
        },
        shortcut: "",
        action: UiAction::SetTheme(next_theme),
    });
    out.push(PaletteCommand {
        category: "Preferencias",
        label: "Abrir Preferencias...",
        shortcut: "",
        action: UiAction::OpenDialog(DialogRequest::Preferences),
    });
    out
}

fn draw_scale_scope_selector(
    ui: &mut egui::Ui,
    id_prefix: &str,
    scope: &mut ScalePageScope,
    range_str: &mut String,
    total_pages: usize,
) {
    ui.horizontal(|ui| {
        ui.label("Aplicar a:");
        ui.radio_value(scope, ScalePageScope::Current, "Página actual");
        ui.radio_value(scope, ScalePageScope::Range, "Rango");
        ui.radio_value(scope, ScalePageScope::All, "Todas");
    });
    if *scope == ScalePageScope::Range {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(range_str)
                    .id(egui::Id::unique(format!("{id_prefix}_range_edit")))
                    .desired_width(130.0)
                    .hint_text("Ej: 1, 3-5"),
            );
            let parsed = parse_page_range(range_str, total_pages);
            let summary = if parsed.is_empty() {
                format!("Sin páginas válidas (1..{total_pages})")
            } else {
                format!("{} página(s) seleccionada(s)", parsed.len())
            };
            ui.label(RichText::new(summary).size(11.0));
        });
    }
}

fn emit_scale_action(
    scale: PageScale,
    scope: ScalePageScope,
    range_str: &str,
    total_pages: usize,
    actions: &mut Vec<UiAction>,
) {
    match scope {
        ScalePageScope::Current => actions.push(UiAction::ApplyPageScale {
            scale,
            apply_all: false,
        }),
        ScalePageScope::All => actions.push(UiAction::ApplyPageScale {
            scale,
            apply_all: true,
        }),
        ScalePageScope::Range => {
            let pages = parse_page_range(range_str, total_pages);
            if pages.is_empty() {
                actions.push(UiAction::ShowToast(
                    "Rango de páginas inválido; se aplicó a página actual.".to_string(),
                ));
                actions.push(UiAction::ApplyPageScale {
                    scale,
                    apply_all: false,
                });
            } else {
                actions.push(UiAction::ApplyScaleToRange { scale, pages });
            }
        }
    }
}

fn draw_scale_presets_and_custom(
    ui: &mut egui::Ui,
    id_prefix: &str,
    draft: &mut PageScale,
    custom_paper_str: &mut String,
    custom_real_str: &mut String,
) {
    ui.horizontal_wrapped(|ui| {
        for p in METRIC_PRESETS {
            let sel = draft.is_set
                && draft.paper_unit.is_metric()
                && (draft.ratio_x() - p.real_value).abs() < 0.05;
            if ui.selectable_label(sel, p.label).clicked() {
                let keep_disp = draft.display_unit;
                let keep_area = draft.area_unit;
                let keep_vol = draft.volume_unit;
                *draft = PageScale::from_preset(p);
                if keep_disp.is_metric() {
                    draft.display_unit = keep_disp;
                    draft.area_unit = keep_area;
                    draft.volume_unit = keep_vol;
                }
                *custom_paper_str = format!("{}", draft.paper_value);
                *custom_real_str = format!("{}", draft.real_value);
            }
        }
    });

    ui.add_space(4.0);
    egui::ComboBox::from_id_salt(format!("{id_prefix}_imp_presets"))
        .selected_text("Escalas Imperiales (EE.UU.)...")
        .width(260.0)
        .show_ui(ui, |ui| {
            for p in IMPERIAL_PRESETS {
                if ui.button(p.label).clicked() {
                    *draft = PageScale::from_preset(p);
                    *custom_paper_str = format!("{}", draft.paper_value);
                    *custom_real_str = format!("{}", draft.real_value);
                }
            }
        });

    ui.separator();
    ui.label(RichText::new("Escala personalizada").strong());
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(custom_paper_str).desired_width(60.0));
        egui::ComboBox::from_id_salt(format!("{id_prefix}_paper_u"))
            .selected_text(draft.paper_unit.suffix())
            .width(75.0)
            .show_ui(ui, |ui| {
                for &u in LengthUnit::ALL {
                    ui.selectable_value(&mut draft.paper_unit, u, u.suffix());
                }
            });
        ui.label("=");
        ui.add(egui::TextEdit::singleline(custom_real_str).desired_width(70.0));
        egui::ComboBox::from_id_salt(format!("{id_prefix}_real_u"))
            .selected_text(draft.real_unit.suffix())
            .width(75.0)
            .show_ui(ui, |ui| {
                for &u in LengthUnit::ALL {
                    ui.selectable_value(&mut draft.real_unit, u, u.suffix());
                }
            });
    });
}

fn apply_custom_strings_to_draft(
    draft: &mut PageScale,
    custom_paper_str: &str,
    custom_real_str: &str,
    y_pair: Option<(&str, &str)>,
) {
    if let (Ok(pv), Ok(rv)) = (
        custom_paper_str.trim().replace(',', ".").parse::<f64>(),
        custom_real_str.trim().replace(',', ".").parse::<f64>(),
    ) && pv > 0.0
        && rv > 0.0
    {
        draft.paper_value = pv;
        draft.real_value = rv;
        draft.is_set = true;
    }
    if draft.separate_xy
        && let Some((yp_str, yr_str)) = y_pair
        && let (Ok(ypv), Ok(yrv)) = (
            yp_str.trim().replace(',', ".").parse::<f64>(),
            yr_str.trim().replace(',', ".").parse::<f64>(),
        )
        && ypv > 0.0
        && yrv > 0.0
    {
        draft.y_paper_value = ypv;
        draft.y_real_value = yrv;
    }
}
