use crate::model::document::DocumentTab;
use crate::model::markup::{
    BoxShape, CommentReply, CountStyle, HatchPattern, LineEnd, LineStyle, MarkupKind,
    MarkupStatus, NoteIcon,
};
use crate::model::units::{LengthUnit, SlopeFormat, format_area};
use crate::ui::actions::UiAction;
use crate::ui::theme::RedBimTheme;
use chrono::Utc;
use eframe::egui::{self, CornerRadius, RichText, Stroke};
use uuid::Uuid;

pub fn draw_properties_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    author_name: &str,
    reply_draft: &mut String,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento activo").color(theme.text_muted));
        return;
    };

    if tab.selected_ids.is_empty() {
        ui.label(RichText::new("Propiedades").strong().size(13.0));
        ui.separator();
        ui.label(
            RichText::new("Seleccione una marca o medición en el plano para editar sus propiedades, comentarios, estilo y capa.")
                .size(11.5)
                .color(theme.text_muted),
        );
        return;
    }

    let first_id = tab.selected_ids[0];
    let Some(idx) = tab.markups.iter().position(|m| m.id == first_id) else {
        return;
    };
    let page_idx = tab.markups[idx].page_index;
    let page = tab.pages.get(page_idx).cloned();
    let layer_names: Vec<String> = tab.layers.iter().map(|l| l.name.clone()).collect();

    let mut m = tab.markups[idx].clone();
    let mut changed = false;

    egui::ScrollArea::vertical()
        .id_salt("props_scroll")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(m.type_name_es())
                        .strong()
                        .size(13.5)
                        .color(theme.accent_cyan),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.checkbox(&mut m.locked, "Bloquear").changed() {
                        changed = true;
                    }
                });
            });
            ui.separator();

            // General metadata
            egui::Grid::new("props_meta_grid")
                .num_columns(2)
                .spacing([8.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Asunto:");
                    if ui
                        .add(egui::TextEdit::singleline(&mut m.subject).desired_width(160.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Etiqueta:");
                    if ui
                        .add(egui::TextEdit::singleline(&mut m.label).desired_width(160.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Autor:");
                    if ui
                        .add(egui::TextEdit::singleline(&mut m.author).desired_width(160.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Estado:");
                    egui::ComboBox::from_id_salt("prop_status")
                        .selected_text(m.status.label())
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for &st in MarkupStatus::ALL {
                                if ui.selectable_value(&mut m.status, st, st.label()).changed() {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Capa:");
                    egui::ComboBox::from_id_salt("prop_layer")
                        .selected_text(&m.layer)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for ln in &layer_names {
                                if ui.selectable_value(&mut m.layer, ln.clone(), ln).changed() {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(RichText::new("Comentario / Texto").strong().size(12.0));
            let mut txt = m
                .text_content()
                .map(|s| s.to_string())
                .unwrap_or_else(|| m.comment.clone());
            if ui
                .add(
                    egui::TextEdit::multiline(&mut txt)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                m.set_text_content(txt);
                changed = true;
            }

            // Replies thread
            if !m.replies.is_empty() {
                ui.add_space(4.0);
                for rep in &m.replies {
                    egui::Frame::new()
                        .fill(theme.card_bg)
                        .stroke(Stroke::new(1.0, theme.border))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    rep.author,
                                    rep.created_at.format("%d/%m %H:%M")
                                ))
                                .size(10.5)
                                .color(theme.text_muted),
                            );
                            ui.label(RichText::new(&rep.text).size(11.5));
                        });
                }
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(reply_draft)
                        .hint_text("Agregar respuesta...")
                        .desired_width(ui.available_width() - 78.0),
                );
                if ui.button("Responder").clicked() && !reply_draft.trim().is_empty() {
                    m.replies.push(CommentReply {
                        id: Uuid::new_v4(),
                        author: author_name.to_string(),
                        text: reply_draft.trim().to_string(),
                        created_at: Utc::now(),
                    });
                    reply_draft.clear();
                    changed = true;
                }
            });

            // Live Measurement results & parameters
            if m.is_measurement()
                && let Some(p) = &page
            {
                ui.separator();
                ui.label(RichText::new("Medición").strong().size(12.0));
                let metrics = m.compute_metrics(&p.scale, &p.viewports);
                if let Some(disp) = &metrics.primary_display {
                    ui.label(
                        RichText::new(format!("Resultado: {disp}"))
                            .strong()
                            .size(13.0)
                            .color(theme.accent_cyan),
                    );
                }
                if let Some(wa) = metrics.wall_area_m2 {
                    ui.label(
                        RichText::new(format!(
                            "Área muro: {}",
                            format_area(wa, p.scale.area_unit, p.scale.precision)
                        ))
                        .size(12.0)
                        .color(theme.accent_green),
                    );
                }
                let is_vol = matches!(m.kind, MarkupKind::MeasureVolume { .. });
                let is_perim = matches!(m.kind, MarkupKind::MeasurePerimeter { .. });
                let is_len = matches!(m.kind, MarkupKind::MeasureLength { .. });
                if let Some(opts) = m.measurement_opts_mut() {
                    if ui
                        .checkbox(&mut opts.show_caption, "Mostrar etiqueta en plano")
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .checkbox(&mut opts.show_segments, "Mostrar cotas por segmento")
                        .changed()
                    {
                        changed = true;
                    }
                    if is_vol {
                        ui.horizontal(|ui| {
                            ui.label("Profundidad (m):");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut opts.depth_meters)
                                        .range(0.01..=500.0)
                                        .speed(0.05)
                                        .suffix(" m"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });
                    }
                    if is_perim {
                        ui.horizontal(|ui| {
                            ui.label("Altura muro (m):");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut opts.depth_meters)
                                        .range(0.0..=100.0)
                                        .speed(0.05)
                                        .suffix(" m"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });
                    }
                    if is_len {
                        ui.horizontal(|ui| {
                            ui.label("Desnivel (m):");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut opts.rise_drop_meters)
                                        .range(-500.0..=500.0)
                                        .speed(0.05)
                                        .suffix(" m"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        egui::ComboBox::from_id_salt("prop_slope_fmt")
                            .selected_text(
                                opts.slope_format_override
                                    .unwrap_or(p.scale.slope_format)
                                    .display_name(),
                            )
                            .show_ui(ui, |ui| {
                                for &sf in SlopeFormat::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut opts.slope_format_override,
                                            Some(sf),
                                            sf.display_name(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                    }
                    egui::ComboBox::from_id_salt("prop_unit_ovr")
                        .selected_text(format!(
                            "Unidad: {}",
                            opts.length_unit_override
                                .map(|u| u.display_name())
                                .unwrap_or("Según página")
                        ))
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut opts.length_unit_override,
                                    None,
                                    "Según página",
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            for &lu in LengthUnit::ALL {
                                if ui
                                    .selectable_value(
                                        &mut opts.length_unit_override,
                                        Some(lu),
                                        lu.display_name(),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                }
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let mut has_cost = m.unit_cost.is_some();
                    if ui.checkbox(&mut has_cost, "Precio unit. ($):").changed() {
                        m.unit_cost = if has_cost { Some(0.0) } else { None };
                        changed = true;
                    }
                    if let Some(uc) = &mut m.unit_cost
                        && ui
                            .add(
                                egui::DragValue::new(uc)
                                    .range(0.0..=100_000_000.0)
                                    .speed(100.0)
                                    .prefix("$ "),
                            )
                            .changed()
                    {
                        changed = true;
                    }
                });
                if let Some(total) = m.total_cost(&p.scale, &p.viewports) {
                    ui.label(
                        RichText::new(format!("Total cubicación: $ {:.0}", total))
                            .strong()
                            .size(12.0)
                            .color(theme.accent_green),
                    );
                }
            }

            ui.separator();
            ui.label(RichText::new("Apariencia").strong().size(12.0));
            egui::Grid::new("props_style_grid")
                .num_columns(2)
                .spacing([8.0, 5.0])
                .show(ui, |ui| {
                    ui.label("Color trazo:");
                    let mut rgb = [
                        m.style.stroke_color[0],
                        m.style.stroke_color[1],
                        m.style.stroke_color[2],
                    ];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        m.style.stroke_color = [rgb[0], rgb[1], rgb[2], 255];
                        m.style.text_color = m.style.stroke_color;
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Relleno:");
                    ui.horizontal(|ui| {
                        let mut has_fill = m.style.fill_color.is_some();
                        if ui.checkbox(&mut has_fill, "").changed() {
                            m.style.fill_color = if has_fill {
                                Some(m.style.stroke_color)
                            } else {
                                None
                            };
                            changed = true;
                        }
                        if let Some(fc) = &mut m.style.fill_color {
                            let mut frgb = [fc[0], fc[1], fc[2]];
                            if ui.color_edit_button_srgb(&mut frgb).changed() {
                                *fc = [frgb[0], frgb[1], frgb[2], 255];
                                changed = true;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Opacidad relleno:");
                    if ui
                        .add(egui::Slider::new(&mut m.style.fill_opacity, 0.05..=1.0))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Grosor:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut m.style.line_width)
                                .range(0.5..=20.0)
                                .speed(0.1)
                                .suffix(" pt"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Estilo línea:");
                    egui::ComboBox::from_id_salt("prop_line_style")
                        .selected_text(m.style.line_style.label())
                        .show_ui(ui, |ui| {
                            for &ls in LineStyle::ALL {
                                if ui
                                    .selectable_value(&mut m.style.line_style, ls, ls.label())
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Achurado:");
                    egui::ComboBox::from_id_salt("prop_hatch")
                        .selected_text(m.style.hatch.label())
                        .show_ui(ui, |ui| {
                            for &h in HatchPattern::ALL {
                                if ui.selectable_value(&mut m.style.hatch, h, h.label()).changed() {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Punta fin:");
                    egui::ComboBox::from_id_salt("prop_end")
                        .selected_text(m.style.end_end.label())
                        .show_ui(ui, |ui| {
                            for &le in LineEnd::ALL {
                                if ui
                                    .selectable_value(&mut m.style.end_end, le, le.label())
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Tamaño texto:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut m.style.font_size)
                                .range(6.0..=48.0)
                                .speed(0.5)
                                .suffix(" pt"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    if matches!(m.kind, MarkupKind::TextBox { .. }) {
                        ui.label("Forma caja:");
                        egui::ComboBox::from_id_salt("prop_box_shape")
                            .selected_text(m.style.box_shape.label())
                            .show_ui(ui, |ui| {
                                for &bs in BoxShape::ALL {
                                    if ui
                                        .selectable_value(&mut m.style.box_shape, bs, bs.label())
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    }

                    if matches!(m.kind, MarkupKind::Note { .. }) {
                        ui.label("Ícono nota:");
                        egui::ComboBox::from_id_salt("prop_note_icon")
                            .selected_text(m.style.note_icon.label())
                            .show_ui(ui, |ui| {
                                for &ni in NoteIcon::ALL {
                                    if ui
                                        .selectable_value(&mut m.style.note_icon, ni, ni.label())
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    }

                    if matches!(m.kind, MarkupKind::MeasureCount { .. }) {
                        ui.label("Símbolo conteo:");
                        egui::ComboBox::from_id_salt("prop_count_style")
                            .selected_text(m.style.count_style.label())
                            .show_ui(ui, |ui| {
                                for &cs in CountStyle::ALL {
                                    if ui
                                        .selectable_value(&mut m.style.count_style, cs, cs.label())
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Traer al frente").clicked() {
                    actions.push(UiAction::BringToFront);
                }
                if ui.button("Enviar atrás").clicked() {
                    actions.push(UiAction::SendToBack);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("+ Caja herramientas").clicked() {
                    actions.push(UiAction::AddSelectedToToolChest);
                }
                if ui.button("Eliminar").clicked() {
                    actions.push(UiAction::DeleteSelected);
                }
            });
        });

    if changed {
        m.modified_at = Utc::now();
        tab.push_undo();
        tab.ensure_layer(&m.layer);
        tab.markups[idx] = m;
    }
}
