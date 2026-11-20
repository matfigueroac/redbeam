use crate::geometry::math::{Point2, Rect2};
use crate::model::document::DocumentTab;
use crate::model::markup::{Markup, MarkupKind, MarkupStyle, MeasurementOptions};
use crate::model::scale::{IMPERIAL_PRESETS, METRIC_PRESETS, PageScale, parse_page_range};
use crate::model::settings::{AppSettings, CsvDelimiter, ThemeMode};
use crate::model::units::{LengthUnit, PT_PER_MM, Precision, SheetDisplayUnit, UnitSystem};
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, RichText};
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
    Preferences,
    Shortcuts,
    CommandPalette {
        query: String,
        selected: usize,
        focus_requested: bool,
    },
    About,
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
                    custom_paper_str: format!("{:.3}", draft.paper_value),
                    custom_real_str: format!("{:.3}", draft.real_value),
                    custom_y_paper_str: format!("{:.3}", draft.y_paper_value),
                    custom_y_real_str: format!("{:.3}", draft.y_real_value),
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
                    custom_paper_str: format!("{:.3}", draft.paper_value),
                    custom_real_str: format!("{:.3}", draft.real_value),
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
                subtitle: "RedBIM Chile".to_string(),
                color: [229, 57, 53],
                show_date: true,
                show_author: true,
            },
            DialogRequest::Preferences => Self::Preferences,
            DialogRequest::Shortcuts => Self::Shortcuts,
            DialogRequest::CommandPalette => Self::CommandPalette {
                query: String::new(),
                selected: 0,
                focus_requested: true,
            },
            DialogRequest::About => Self::About,
        }
    }
}

pub fn draw_active_modal(
    ctx: &egui::Context,
    modal_slot: &mut Option<ActiveModal>,
    tab: Option<&mut DocumentTab>,
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
                            .add(egui::Button::new("Aplicar escala").fill(theme.accent_red))
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
                            .add(egui::Button::new("Aceptar").fill(theme.accent_red))
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
                            .add(egui::Button::new("Guardar Viewport").fill(theme.accent_red))
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
                            .add(egui::Button::new("Insertar en lámina").fill(theme.accent_red))
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
                            .add(egui::Button::new("Insertar sello").fill(theme.accent_red))
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
            egui::Window::new("Preferencias de RedBIM")
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
            egui::Window::new("Atajos de teclado RedBIM")
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
                        ("Doble clic en texto", "Editar texto directamente en el plano"),
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
            egui::Window::new("Acerca de RedBIM")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut keep_open)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("RedBIM v0.2.0")
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
    }

    if keep_open && !close_clicked {
        *modal_slot = Some(modal);
    }
    let _ = Point2::default();
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
