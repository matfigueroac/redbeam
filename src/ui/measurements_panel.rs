use crate::model::document::DocumentTab;
use crate::model::scale::{IMPERIAL_PRESETS, METRIC_PRESETS, PageScale};
use crate::model::tool_chest::ActiveTool;
use crate::model::units::{
    AreaUnit, LengthUnit, Precision, SlopeFormat, UnitSystem, VolumeUnit, format_area,
    format_length, format_volume,
};
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

pub fn draw_measurements_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    active_tool: ActiveTool,
    apply_all_pages: &mut bool,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    let page_idx = tab.active_page_index();
    let mut scale = tab
        .pages
        .get(page_idx)
        .map(|p| p.scale.clone())
        .unwrap_or_default();
    let viewports = tab
        .pages
        .get(page_idx)
        .map(|p| p.viewports.clone())
        .unwrap_or_default();
    let totals = tab.compute_totals(Some(page_idx));

    let mut scale_changed = false;

    egui::ScrollArea::vertical()
        .id_salt("meas_panel_scroll")
        .show(ui, |ui| {
            // Scale Header Card
            let card_border = if scale.is_set {
                theme.accent_green
            } else {
                theme.accent_amber
            };
            egui::Frame::new()
                .fill(theme.card_bg)
                .stroke(Stroke::new(1.2, card_border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Escala de lámina:").size(11.5).color(theme.text_muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(scale.label())
                                    .strong()
                                    .size(12.5)
                                    .color(card_border),
                            );
                        });
                    });
                    ui.add_space(5.0);
                    let cal_btn = egui::Button::new(
                        RichText::new("Calibrar escala (2 puntos)")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent_red)
                    .min_size(egui::vec2(ui.available_width(), 28.0));
                    if ui.add(cal_btn).clicked() {
                        actions.push(UiAction::StartTwoPointCalibration);
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Configurar X/Y...").clicked() {
                            actions.push(UiAction::OpenDialog(DialogRequest::SetScale));
                        }
                        ui.checkbox(apply_all_pages, "Todas las páginas");
                    });
                });

            ui.add_space(8.0);
            ui.label(RichText::new("Escalas métricas rápidas (Chile)").strong().size(12.0));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for preset in METRIC_PRESETS
                    .iter()
                    .filter(|p| ["1:10", "1:20", "1:25", "1:50", "1:75", "1:100", "1:125", "1:200", "1:500", "1:1000"].contains(&p.label))
                {
                    let is_cur = scale.is_set
                        && scale.paper_unit.is_metric()
                        && (scale.ratio_x() - preset.real_value).abs() < 0.1;
                    let btn = ui.selectable_label(is_cur, RichText::new(preset.label).size(11.5));
                    if btn.clicked() {
                        let mut s = PageScale::from_preset(preset);
                        s.display_unit = scale.display_unit;
                        s.area_unit = scale.area_unit;
                        s.volume_unit = scale.volume_unit;
                        s.precision = scale.precision;
                        actions.push(UiAction::ApplyPageScale {
                            scale: s,
                            apply_all: *apply_all_pages,
                        });
                    }
                }
            });

            ui.add_space(4.0);
            egui::ComboBox::from_id_salt("meas_imp_preset")
                .selected_text("Escalas imperiales (EE.UU.)...")
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for preset in IMPERIAL_PRESETS {
                        if ui.button(preset.label).clicked() {
                            actions.push(UiAction::ApplyPageScale {
                                scale: PageScale::from_preset(preset),
                                apply_all: *apply_all_pages,
                            });
                        }
                    }
                });

            ui.separator();

            // Chilean Metric vs American Imperial quick toggle
            ui.label(RichText::new("Sistema y unidades").strong().size(12.0));
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(scale.display_unit.is_metric(), "Métrico Chile (m / m²)")
                    .clicked()
                {
                    scale.set_unit_system(UnitSystem::Metric);
                    scale_changed = true;
                }
                if ui
                    .selectable_label(!scale.display_unit.is_metric(), "Imperial EE.UU. (ft-in)")
                    .clicked()
                {
                    scale.set_unit_system(UnitSystem::Imperial);
                    scale_changed = true;
                }
            });

            egui::Grid::new("meas_units_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Longitud:");
                    egui::ComboBox::from_id_salt("mu_len")
                        .selected_text(scale.display_unit.display_name())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &u in LengthUnit::ALL {
                                if ui
                                    .selectable_value(&mut scale.display_unit, u, u.display_name())
                                    .changed()
                                {
                                    scale_changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Área:");
                    egui::ComboBox::from_id_salt("mu_area")
                        .selected_text(scale.area_unit.display_name())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &u in AreaUnit::ALL {
                                if ui
                                    .selectable_value(&mut scale.area_unit, u, u.display_name())
                                    .changed()
                                {
                                    scale_changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Volumen:");
                    egui::ComboBox::from_id_salt("mu_vol")
                        .selected_text(scale.volume_unit.display_name())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &u in VolumeUnit::ALL {
                                if ui
                                    .selectable_value(&mut scale.volume_unit, u, u.display_name())
                                    .changed()
                                {
                                    scale_changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Precisión:");
                    egui::ComboBox::from_id_salt("mu_prec")
                        .selected_text(scale.precision.label())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &pr in Precision::ALL {
                                if ui
                                    .selectable_value(&mut scale.precision, pr, pr.label())
                                    .changed()
                                {
                                    scale_changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Pendiente:");
                    egui::ComboBox::from_id_salt("mu_slope")
                        .selected_text(scale.slope_format.display_name())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &sf in SlopeFormat::ALL {
                                if ui
                                    .selectable_value(&mut scale.slope_format, sf, sf.display_name())
                                    .changed()
                                {
                                    scale_changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });

            ui.separator();

            // Measurement tools quick grid
            ui.label(RichText::new("Herramientas de medición").strong().size(12.0));
            ui.horizontal_wrapped(|ui| {
                for tool in [
                    ActiveTool::MeasureLength,
                    ActiveTool::MeasurePolylength,
                    ActiveTool::MeasurePerimeter,
                    ActiveTool::MeasureArea,
                    ActiveTool::MeasureVolume,
                    ActiveTool::MeasureAngle,
                    ActiveTool::MeasureRadius,
                    ActiveTool::MeasureDiameter,
                    ActiveTool::MeasureCount,
                    ActiveTool::CutoutPoly,
                    ActiveTool::SketchRect,
                ] {
                    if icon_button(
                        ui,
                        30.0,
                        IconKind::from_tool(tool),
                        active_tool == tool,
                        true,
                        tool.label_es(),
                        theme,
                    )
                    .clicked()
                    {
                        if tool == ActiveTool::SketchRect {
                            actions.push(UiAction::OpenDialog(DialogRequest::SketchToScale));
                        } else {
                            actions.push(UiAction::SetTool(tool));
                        }
                    }
                }
            });

            ui.separator();

            // Viewports section
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Viewports ({})", viewports.len())).strong().size(12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("+ Dibujar Viewport").clicked() {
                        actions.push(UiAction::SetTool(ActiveTool::ViewportRegion));
                    }
                });
            });
            for vp in &viewports {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} · {}", vp.name, vp.scale.label()))
                            .size(11.5)
                            .color(theme.accent_cyan),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Eliminar Viewport").clicked() {
                            actions.push(UiAction::DeleteViewport(vp.id));
                        }
                        if ui.small_button("Editar").on_hover_text("Editar nombre y escala del Viewport").clicked() {
                            actions.push(UiAction::OpenDialog(DialogRequest::EditViewport {
                                viewport_id: vp.id,
                            }));
                        }
                    });
                });
            }

            ui.separator();

            // Live Totals Card
            egui::Frame::new()
                .fill(theme.card_bg)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Totales en esta lámina").strong().size(12.0));
                    ui.add_space(4.0);
                    egui::Grid::new("meas_totals_grid")
                        .num_columns(2)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            ui.label("Longitud:");
                            ui.label(
                                RichText::new(format_length(
                                    totals.total_length_m,
                                    scale.display_unit,
                                    scale.precision,
                                ))
                                .strong(),
                            );
                            ui.end_row();

                            ui.label("Perímetro:");
                            ui.label(
                                RichText::new(format_length(
                                    totals.total_perimeter_m,
                                    scale.display_unit,
                                    scale.precision,
                                ))
                                .strong(),
                            );
                            ui.end_row();

                            ui.label("Área neta:");
                            ui.label(
                                RichText::new(format_area(
                                    totals.total_area_m2,
                                    scale.area_unit,
                                    scale.precision,
                                ))
                                .strong(),
                            );
                            ui.end_row();

                            ui.label("Volumen:");
                            ui.label(
                                RichText::new(format_volume(
                                    totals.total_volume_m3,
                                    scale.volume_unit,
                                    scale.precision,
                                ))
                                .strong(),
                            );
                            ui.end_row();

                            ui.label("Conteo:");
                            ui.label(RichText::new(format!("{} und", totals.total_count)).strong());
                            ui.end_row();

                            if totals.total_cost > 0.0 {
                                ui.label("Costo total:");
                                ui.label(
                                    RichText::new(format!("$ {:.0}", totals.total_cost))
                                        .strong()
                                        .color(theme.accent_green),
                                );
                                ui.end_row();
                            }
                        });
                });
        });

    if scale_changed {
        actions.push(UiAction::ApplyPageScale {
            scale,
            apply_all: *apply_all_pages,
        });
    }
}
