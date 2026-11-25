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
use eframe::egui::{self, CornerRadius, RichText, Stroke};

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
        .auto_shrink([false, false])
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
                            ui.add(
                                egui::Label::new(
                                    RichText::new(scale.label())
                                        .strong()
                                        .size(12.5)
                                        .color(card_border),
                                )
                                .wrap(),
                            );
                        });
                    });
                    ui.add_space(5.0);
                    let cal_btn = egui::Button::new(
                        RichText::new("Calibrar escala (2 puntos)")
                            .strong()
                            .color(theme.text_on_accent),
                    )
                    .fill(theme.accent_red)
                    .min_size(egui::vec2(ui.available_width(), 28.0));
                    if ui.add(cal_btn).clicked() {
                        actions.push(UiAction::StartTwoPointCalibration);
                    }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
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
                .wrap_mode(egui::TextWrapMode::Wrap)
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
            ui.horizontal_wrapped(|ui| {
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

            scale_changed |= unit_combo(
                ui, "Longitud:", &mut scale.display_unit, LengthUnit::ALL, LengthUnit::display_name,
            );
            scale_changed |= unit_combo(
                ui, "Área:", &mut scale.area_unit, AreaUnit::ALL, AreaUnit::display_name,
            );
            scale_changed |= unit_combo(
                ui, "Volumen:", &mut scale.volume_unit, VolumeUnit::ALL, VolumeUnit::display_name,
            );
            scale_changed |= unit_combo(
                ui, "Precisión:", &mut scale.precision, Precision::ALL, Precision::label,
            );
            scale_changed |= unit_combo(
                ui, "Pendiente:", &mut scale.slope_format, SlopeFormat::ALL, SlopeFormat::display_name,
            );

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
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("Viewports ({})", viewports.len())).strong().size(12.0));
                if ui.small_button("+ Dibujar Viewport").clicked() {
                    actions.push(UiAction::SetTool(ActiveTool::ViewportRegion));
                }
            });
            for vp in &viewports {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {}", vp.name, vp.scale.label()))
                            .size(11.5)
                            .color(theme.accent_cyan),
                    )
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("Editar").on_hover_text("Editar nombre y escala del Viewport").clicked() {
                        actions.push(UiAction::OpenDialog(DialogRequest::EditViewport {
                            viewport_id: vp.id,
                        }));
                    }
                    if ui.small_button("✕").on_hover_text("Eliminar Viewport").clicked() {
                        actions.push(UiAction::DeleteViewport(vp.id));
                    }
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

fn unit_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[T],
    name: impl Fn(T) -> &'static str,
) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, 0.0],
            egui::Label::new(label).halign(egui::Align::Min),
        );
        egui::ComboBox::from_id_salt(label)
            .selected_text(name(*value))
            .width(ui.available_width().min(155.0))
            .wrap_mode(egui::TextWrapMode::Wrap)
            .show_ui(ui, |ui| {
                let mut changed = false;
                for &option in options {
                    changed |= ui.selectable_value(value, option, name(option)).changed();
                }
                changed
            })
            .inner
            .unwrap_or(false)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::math::Rect2;
    use crate::model::scale::ViewportScale;
    use crate::model::settings::ThemeMode;
    use crate::pdf::io::open_demo_chile_document;

    #[test]
    fn measurements_fit_the_right_sidebar() {
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        let mut tab = open_demo_chile_document().unwrap();
        tab.pages[0].viewports.push(ViewportScale::new(
            "Detalle de la estructura de montaje fotovoltaico en la cubierta principal",
            Rect2::from_xywh(10.0, 10.0, 50.0, 50.0),
            PageScale::from_metric_ratio(20.0),
        ));
        for (width, calibrated, system) in [
            (230.0, false, UnitSystem::Imperial),
            (275.0, true, UnitSystem::Metric),
            (400.0, true, UnitSystem::Metric),
        ] {
            tab.pages[0].scale.is_set = calibrated;
            tab.pages[0].scale.set_unit_system(system);
            let ctx = egui::Context::default();
            for _ in 0..3 {
                let mut right = 0.0;
                let mut panel_right = 0.0;
                let mut canvas_right = 0.0;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(960.0, 620.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        right = ui.available_rect_before_wrap().right();
                        let panel = egui::Panel::right("test_measurements")
                            .exact_size(width)
                            .frame(
                                egui::Frame::new()
                                    .inner_margin(egui::Margin::same(8))
                                    .stroke(Stroke::new(1.0, theme.border)),
                            )
                            .show(ui, |ui| {
                                draw_measurements_panel(
                                    ui,
                                    Some(&mut tab),
                                    ActiveTool::MeasureLength,
                                    &mut false,
                                    &theme,
                                    &mut Vec::new(),
                                );
                            });
                        panel_right = panel.response.rect.right();
                        canvas_right = ui.available_rect_before_wrap().right();
                    },
                );
                output.textures_delta.clear();
                assert!(
                    (panel_right - right).abs() < 0.1,
                    "width {width}: panel right edge {panel_right}, expected {right}"
                );
                assert!((canvas_right - (right - width)).abs() < 0.1);
            }
        }
    }
}
