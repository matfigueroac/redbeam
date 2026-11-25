use crate::model::custom_columns::{
    CustomColumnKind, CustomColumnValue, evaluate_column_cell,
};
use crate::model::document::DocumentTab;
use crate::model::markup::{
    BoxShape, CommentReply, CountStyle, HatchPattern, HyperlinkTarget, LineEnd, LineStyle, Markup,
    MarkupKind, MarkupStatus, NoteIcon,
};
use crate::model::units::{LengthUnit, Precision, SlopeFormat, format_area};
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::canvas::fit_callout_text;
use crate::ui::theme::RedBimTheme;
use chrono::Utc;
use eframe::egui::{self, Color32, CornerRadius, Pos2, RichText, Sense, Stroke, TextureHandle, Vec2};
use std::collections::HashMap;
use uuid::Uuid;

const QUICK_COLORS: &[[u8; 4]] = &[
    [229, 57, 53, 255],   // RedBIM Red
    [37, 99, 235, 255],   // Engineering Blue
    [22, 163, 74, 255],   // Measurement Green
    [245, 158, 11, 255],  // Contractor Amber
    [6, 182, 212, 255],   // Cyan
    [147, 51, 234, 255],  // Purple
    [24, 24, 27, 255],    // Dark Charcoal
];

const QUICK_LINE_ENDS: &[(LineEnd, &str)] = &[
    (LineEnd::Slash, "Diagonal (/) — Arquitectura"),
    (LineEnd::ClosedArrow, "Flecha rellena — Ingeniería"),
    (LineEnd::OpenArrow, "Flecha abierta"),
    (LineEnd::Butt, "Tope (|)"),
    (LineEnd::Circle, "Punto / Círculo (●)"),
    (LineEnd::None, "Sin punta (—)"),
];

fn supports_line_ends(kind: &MarkupKind) -> bool {
    matches!(
        kind,
        MarkupKind::MeasureLength { .. }
            | MarkupKind::Line { .. }
            | MarkupKind::Arrow { .. }
            | MarkupKind::Polyline { .. }
            | MarkupKind::Callout { .. }
            | MarkupKind::CloudPlus { .. }
    )
}

fn is_symmetric_dimension(kind: &MarkupKind) -> bool {
    matches!(kind, MarkupKind::MeasureLength { .. })
}

fn supports_fill(kind: &MarkupKind) -> bool {
    matches!(
        kind,
        MarkupKind::Rectangle { .. }
            | MarkupKind::Ellipse { .. }
            | MarkupKind::Polygon { .. }
            | MarkupKind::Cloud { .. }
            | MarkupKind::CloudPlus { .. }
            | MarkupKind::TextBox { .. }
            | MarkupKind::Callout { .. }
            | MarkupKind::Highlight { .. }
            | MarkupKind::MeasureArea { .. }
            | MarkupKind::MeasureVolume { .. }
            | MarkupKind::SketchRect { .. }
            | MarkupKind::SketchPoly { .. }
            | MarkupKind::Stamp { .. }
            | MarkupKind::Legend { .. }
            | MarkupKind::Hyperlink { .. }
            | MarkupKind::Redaction { .. }
    )
}

fn supports_hatch(kind: &MarkupKind) -> bool {
    matches!(
        kind,
        MarkupKind::Rectangle { .. }
            | MarkupKind::Ellipse { .. }
            | MarkupKind::Polygon { .. }
            | MarkupKind::MeasureArea { .. }
            | MarkupKind::MeasureVolume { .. }
            | MarkupKind::SketchRect { .. }
            | MarkupKind::SketchPoly { .. }
    )
}

fn supports_line_style(kind: &MarkupKind) -> bool {
    !matches!(
        kind,
        MarkupKind::Note { .. }
            | MarkupKind::Stamp { .. }
            | MarkupKind::MeasureCount { .. }
            | MarkupKind::Image { .. }
            | MarkupKind::Typewriter { .. }
    )
}

fn default_details_open(m: &Markup) -> bool {
    matches!(
        m.kind,
        MarkupKind::Note { .. }
            | MarkupKind::TextBox { .. }
            | MarkupKind::Typewriter { .. }
            | MarkupKind::Callout { .. }
            | MarkupKind::CloudPlus { .. }
            | MarkupKind::Stamp { .. }
    ) || !m.comment.trim().is_empty()
        || !m.replies.is_empty()
}

/// Visual mini-button that paints a crisp vector preview of a dimension/line end.
fn line_end_icon_button(
    ui: &mut egui::Ui,
    end: LineEnd,
    selected: bool,
    tooltip: &str,
    theme: &RedBimTheme,
) -> egui::Response {
    let size = Vec2::new(34.0, 26.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let bg = if selected {
        theme.accent_red.gamma_multiply(0.22)
    } else if resp.hovered() {
        theme.control_hover
    } else {
        theme.control_bg
    };
    let border_col = if selected {
        theme.accent_red
    } else if resp.hovered() {
        theme.border_strong
    } else {
        theme.border
    };
    let fg = if selected {
        theme.text_primary
    } else {
        theme.text_secondary
    };

    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(5),
        bg,
        Stroke::new(if selected { 1.4 } else { 1.0 }, border_col),
        egui::StrokeKind::Inside,
    );

    let cy = rect.center().y;
    let x0 = rect.left() + 6.0;
    let x1 = rect.right() - 7.0;
    let stroke = Stroke::new(1.6, fg);

    // Base horizontal shaft
    p.line_segment([Pos2::new(x0, cy), Pos2::new(x1, cy)], stroke);

    // Paint the end symbol at x1 (pointing right)
    match end {
        LineEnd::Slash => {
            p.line_segment(
                [Pos2::new(x1 - 4.0, cy + 5.0), Pos2::new(x1 + 2.5, cy - 5.0)],
                Stroke::new(1.8, fg),
            );
            // Tiny extension tick
            p.line_segment(
                [Pos2::new(x1 - 0.5, cy - 6.0), Pos2::new(x1 - 0.5, cy + 6.0)],
                Stroke::new(0.8, fg.gamma_multiply(0.45)),
            );
        }
        LineEnd::ClosedArrow => {
            let tip = Pos2::new(x1 + 1.5, cy);
            let pts = vec![
                tip,
                Pos2::new(x1 - 6.5, cy - 4.2),
                Pos2::new(x1 - 6.5, cy + 4.2),
            ];
            p.add(egui::Shape::convex_polygon(pts, fg, Stroke::NONE));
        }
        LineEnd::OpenArrow => {
            let tip = Pos2::new(x1 + 1.0, cy);
            p.line_segment([Pos2::new(x1 - 5.5, cy - 4.2), tip], stroke);
            p.line_segment([Pos2::new(x1 - 5.5, cy + 4.2), tip], stroke);
        }
        LineEnd::Butt => {
            p.line_segment(
                [Pos2::new(x1, cy - 5.5), Pos2::new(x1, cy + 5.5)],
                Stroke::new(2.0, fg),
            );
        }
        LineEnd::Circle => {
            p.circle_filled(Pos2::new(x1 - 1.0, cy), 3.3, fg);
        }
        _ => {}
    }

    resp.on_hover_text(tooltip)
}

fn segmented_pill(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    tooltip: &str,
    theme: &RedBimTheme,
) -> egui::Response {
    let fill = if selected {
        theme.accent_red.gamma_multiply(0.24)
    } else {
        theme.control_bg
    };
    let stroke = Stroke::new(
        if selected { 1.2 } else { 0.8 },
        if selected {
            theme.accent_red
        } else {
            theme.border
        },
    );
    let txt_col = if selected {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    ui.add(
        egui::Button::new(RichText::new(label).size(11.0).color(txt_col))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(4))
            .min_size(Vec2::new(0.0, 22.0)),
    )
    .on_hover_text(tooltip)
}

pub fn draw_properties_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    author_name: &str,
    reply_draft: &mut String,
    image_textures: &HashMap<Uuid, TextureHandle>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento activo").color(theme.text_muted));
        return;
    };

    if tab.selected_ids.is_empty() {
        egui::Frame::new()
            .fill(theme.card_bg)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Ninguna marca seleccionada")
                        .strong()
                        .size(12.0)
                        .color(theme.text_primary),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Haz clic en una cota o marca del plano para cambiar puntas, unidades, estilo, capa o comentarios.",
                    )
                    .size(11.0)
                    .color(theme.text_muted),
                );
            });
        return;
    }

    let first_id = tab.selected_ids[0];
    let Some(idx) = tab.markups.iter().position(|m| m.id == first_id) else {
        return;
    };
    let page_idx = tab.markups[idx].page_index;
    let page = tab.pages.get(page_idx).cloned();
    let layer_names: Vec<String> = tab.layers.iter().map(|l| l.name.clone()).collect();
    let page_labels: Vec<(usize, String)> = tab
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.display_label()))
        .collect();
    let custom_columns = tab.custom_columns.clone();

    let orig = tab.markups[idx].clone();
    let enclosing_space = tab.resolve_space_for_markup(&orig);
    let layer_locked = tab.is_layer_locked(&orig.layer);
    let was_locked = orig.locked || layer_locked;
    let mut m = orig.clone();
    let mut changed = false;

    egui::ScrollArea::vertical()
        .id_salt("props_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;

            // ── 1. Compact Summary Card (always visible at top) ──────────────
            egui::Frame::new()
                .fill(theme.card_bg)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(m.type_name_es())
                                .strong()
                                .size(12.5)
                                .color(theme.accent_cyan),
                        );
                        if tab.selected_ids.len() > 1 {
                            ui.label(
                                RichText::new(format!("(+{})", tab.selected_ids.len() - 1))
                                    .size(11.0)
                                    .color(theme.text_muted),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let lock_lbl = if m.locked { "🔒 Bloq." } else { "🔓 Libre" };
                            if segmented_pill(
                                ui,
                                lock_lbl,
                                m.locked,
                                "Bloquear o desbloquear edición/movimiento",
                                theme,
                            )
                            .clicked()
                            {
                                m.locked = !m.locked;
                                changed = true;
                            }
                        });
                    });

                    if m.is_measurement()
                        && let Some(p) = &page
                    {
                        let metrics = m.compute_metrics(&p.scale, &p.viewports);
                        if let Some(disp) = &metrics.primary_display {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(disp)
                                    .strong()
                                    .size(18.0)
                                    .color(theme.text_primary),
                            );
                        }
                        if let Some(wa) = metrics.wall_area_m2 {
                            ui.label(
                                RichText::new(format!(
                                    "Área muro: {}",
                                    format_area(wa, p.scale.area_unit, p.scale.precision)
                                ))
                                .size(11.5)
                                .color(theme.accent_green),
                            );
                        }
                        if let Some(total) = m.total_cost(&p.scale, &p.viewports) {
                            ui.label(
                                RichText::new(format!("Total cubicación: $ {:.0}", total))
                                    .strong()
                                    .size(11.5)
                                    .color(theme.accent_green),
                            );
                        }
                    }

                    if let Some(ref sp_name) = enclosing_space {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!("Espacio: {sp_name}"))
                                .size(11.0)
                                .color(theme.accent_amber),
                        );
                    }

                    // Quick action bar
                    if !m.locked && m.is_measurement() {
                        ui.add_space(4.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 24.0],
                                egui::Button::new(
                                    RichText::new("⌖ Ajustar puntos a esquinas").size(11.5),
                                ),
                            )
                            .on_hover_text(
                                "Re-ancla automáticamente los puntos de la cota a las esquinas o líneas más cercanas del plano",
                            )
                            .clicked()
                        {
                            actions.push(UiAction::SnapSelectedToContent);
                        }
                    }
                });

            ui.push_id(m.id, |ui| {
                let label = match &m.kind {
                    MarkupKind::Stamp { .. } | MarkupKind::Legend { .. } => "Título",
                    MarkupKind::Hyperlink { .. } => "Etiqueta visible",
                    _ if m.text_content().is_some() => "Texto",
                    _ => "Comentario",
                };
                ui.label(RichText::new(label).strong().size(12.0));
                let mut text = m.text_content().unwrap_or(&m.comment).to_owned();
                if ui.add_enabled(
                    !was_locked,
                    egui::TextEdit::multiline(&mut text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                ).changed() {
                    m.set_text_content(text);
                    changed = true;
                }
                if let MarkupKind::Stamp { subtitle, .. } = &mut m.kind {
                    ui.label(RichText::new("Subtítulo").size(11.0).color(theme.text_secondary));
                    changed |= ui.add_enabled(
                        !was_locked,
                        egui::TextEdit::singleline(subtitle).desired_width(f32::INFINITY),
                    ).changed();
                }
            });

            // ── 1b. Hyperlink destination editor ────────────────────────────
            if let MarkupKind::Hyperlink {
                target,
                ..
            } = &mut m.kind
            {
                egui::Frame::new()
                    .fill(theme.card_bg)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Destino del Hipervínculo")
                                .strong()
                                .size(12.0)
                                .color(theme.text_primary),
                        );
                        ui.horizontal(|ui| {
                            let is_page = matches!(target, HyperlinkTarget::Page { .. });
                            if segmented_pill(ui, "Lámina", is_page, "Saltar a otra página del PDF", theme).clicked()
                                && !is_page
                            {
                                *target = HyperlinkTarget::Page { page_index: 0 };
                                changed = true;
                            }
                            if segmented_pill(ui, "URL externa", !is_page, "Abrir sitio web http/https", theme).clicked()
                                && is_page
                            {
                                *target = HyperlinkTarget::Url("https://".to_string());
                                changed = true;
                            }
                        });
                        match target {
                            HyperlinkTarget::Page { page_index } => {
                                let cur_lbl = page_labels
                                    .iter()
                                    .find(|(i, _)| *i == *page_index)
                                    .map(|(_, l)| l.clone())
                                    .unwrap_or_else(|| format!("Página {}", *page_index + 1));
                                egui::ComboBox::from_id_salt("props_hl_page")
                                    .selected_text(cur_lbl)
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for (pi, lbl) in &page_labels {
                                            if ui.selectable_value(page_index, *pi, lbl).changed() {
                                                changed = true;
                                            }
                                        }
                                    });
                            }
                            HyperlinkTarget::Url(url) => {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(url)
                                            .hint_text("https://ejemplo.cl/especificacion")
                                            .desired_width(f32::INFINITY),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        }
                        if ui
                            .add_sized(
                                [ui.available_width(), 24.0],
                                egui::Button::new(RichText::new("🔗 Abrir destino →").size(11.5)),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::FollowHyperlink(target.clone()));
                        }
                    });
            }

            // ── 1c. Redaction overlay editor & Apply CTA ────────────────────
            if let MarkupKind::Redaction { overlay_text, .. } = &mut m.kind {
                egui::Frame::new()
                    .fill(theme.card_bg)
                    .stroke(Stroke::new(1.2, theme.accent_red))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Redacción pendiente (Censura)")
                                .strong()
                                .size(12.0)
                                .color(theme.accent_red),
                        );
                        ui.label(
                            RichText::new("Texto superpuesto al aplicar:")
                                .size(11.0)
                                .color(theme.text_secondary),
                        );
                        if ui
                            .add(
                                egui::TextEdit::singleline(overlay_text)
                                    .hint_text("Ej: CENSURADO, CONFIDENCIAL, o vacío")
                                    .desired_width(f32::INFINITY),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.add_space(4.0);
                        if ui
                            .add_sized(
                                [ui.available_width(), 24.0],
                                egui::Button::new(
                                    RichText::new("Aplicar redacciones (Irreversible)...")
                                        .strong()
                                        .color(theme.accent_red),
                                ),
                            )
                            .clicked()
                        {
                            actions.push(UiAction::OpenDialog(
                                DialogRequest::ApplyRedactionsConfirm,
                            ));
                        }
                    });
            }

            // ── 2. Cota y Medición (or Puntas de línea) ──────────────────────
            let show_ends = supports_line_ends(&m.kind);
            let is_meas = m.is_measurement();
            if is_meas || show_ends {
                let sec_title = if is_meas {
                    "Cota y Medición"
                } else {
                    "Puntas de línea"
                };
                egui::CollapsingHeader::new(RichText::new(sec_title).strong().size(12.0))
                    .id_salt("props_sec_dimension")
                    .default_open(true)
                    .show(ui, |ui| {
                        // Visual dimension end picker (1 click!)
                        if show_ends {
                            let sym = is_symmetric_dimension(&m.kind);
                            ui.label(
                                RichText::new(if sym {
                                    "Tipo de cota (ambos extremos)"
                                } else {
                                    "Terminación / Punta"
                                })
                                .size(11.0)
                                .color(theme.text_secondary),
                            );
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                                for &(end, tip) in QUICK_LINE_ENDS {
                                    let is_sel = if sym {
                                        m.style.end_end == end && m.style.start_end == end
                                    } else {
                                        m.style.end_end == end
                                    };
                                    if line_end_icon_button(ui, end, is_sel, tip, theme).clicked() {
                                        m.style.end_end = end;
                                        if sym {
                                            m.style.start_end = end;
                                        }
                                        changed = true;
                                    }
                                }
                            });

                            // Advanced / asymmetric end selector (collapsed by default)
                            egui::CollapsingHeader::new(
                                RichText::new("Inicio / fin independiente")
                                    .size(10.8)
                                    .color(theme.text_muted),
                            )
                            .id_salt("props_asym_ends")
                            .default_open(false)
                            .show(ui, |ui| {
                                egui::Grid::new("props_asym_grid")
                                    .num_columns(2)
                                    .spacing([8.0, 4.0])
                                    .show(ui, |ui| {
                                        ui.label(RichText::new("Inicio:").size(11.0));
                                        egui::ComboBox::from_id_salt("prop_start_end")
                                            .selected_text(m.style.start_end.label())
                                            .width(130.0)
                                            .show_ui(ui, |ui| {
                                                for &le in LineEnd::ALL {
                                                    if ui
                                                        .selectable_value(
                                                            &mut m.style.start_end,
                                                            le,
                                                            le.label(),
                                                        )
                                                        .changed()
                                                    {
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        ui.end_row();

                                        ui.label(RichText::new("Fin:").size(11.0));
                                        egui::ComboBox::from_id_salt("prop_end_end")
                                            .selected_text(m.style.end_end.label())
                                            .width(130.0)
                                            .show_ui(ui, |ui| {
                                                for &le in LineEnd::ALL {
                                                    if ui
                                                        .selectable_value(
                                                            &mut m.style.end_end,
                                                            le,
                                                            le.label(),
                                                        )
                                                        .changed()
                                                    {
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        ui.end_row();
                                    });
                            });
                            ui.add_space(4.0);
                        }

                        // Measurement options
                        if is_meas
                            && let Some(p) = &page
                        {
                            let is_vol = matches!(m.kind, MarkupKind::MeasureVolume { .. });
                            let is_perim = matches!(m.kind, MarkupKind::MeasurePerimeter { .. });
                            let is_len = matches!(m.kind, MarkupKind::MeasureLength { .. });

                            let mut unit_cost = m.unit_cost;
                            if let Some(opts) = m.measurement_opts_mut() {
                                // Segmented unit pills (Pág | mm | cm | m + dropdown for others)
                                ui.label(
                                    RichText::new("Unidad de medida")
                                        .size(11.0)
                                        .color(theme.text_secondary),
                                );
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                                    for (val, short, tip) in [
                                        (None, "Pág", "Usar unidad de la página"),
                                        (Some(LengthUnit::Millimeter), "mm", "Milímetros"),
                                        (Some(LengthUnit::Centimeter), "cm", "Centímetros"),
                                        (Some(LengthUnit::Meter), "m", "Metros"),
                                    ] {
                                        if segmented_pill(
                                            ui,
                                            short,
                                            opts.length_unit_override == val,
                                            tip,
                                            theme,
                                        )
                                        .clicked()
                                        {
                                            opts.length_unit_override = val;
                                            changed = true;
                                        }
                                    }
                                    let other_lbl = match opts.length_unit_override {
                                        Some(
                                            u @ (LengthUnit::Kilometer
                                            | LengthUnit::Foot
                                            | LengthUnit::Inch
                                            | LengthUnit::FootInch
                                            | LengthUnit::Yard),
                                        ) => u.suffix(),
                                        _ => "Otras",
                                    };
                                    egui::ComboBox::from_id_salt("prop_unit_other")
                                        .selected_text(RichText::new(other_lbl).size(11.0))
                                        .width(58.0)
                                        .show_ui(ui, |ui| {
                                            for &lu in [
                                                LengthUnit::Kilometer,
                                                LengthUnit::Foot,
                                                LengthUnit::Inch,
                                                LengthUnit::FootInch,
                                                LengthUnit::Yard,
                                            ]
                                            .iter()
                                            {
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
                                });

                                // Quick decimal precision pills (Pág | 0 | 0.0 | 0.00 | 0.000)
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                                    ui.label(
                                        RichText::new("Dec:")
                                            .size(11.0)
                                            .color(theme.text_secondary),
                                    );
                                    for (val, lbl, tip) in [
                                        (None, "Pág", "Usar precisión de la página"),
                                        (Some(Precision::Dec0), "0", "Sin decimales"),
                                        (Some(Precision::Dec1), "0.0", "1 decimal"),
                                        (Some(Precision::Dec2), "0.00", "2 decimales"),
                                        (Some(Precision::Dec3), "0.000", "3 decimales"),
                                    ] {
                                        if segmented_pill(
                                            ui,
                                            lbl,
                                            opts.precision_override == val,
                                            tip,
                                            theme,
                                        )
                                        .clicked()
                                        {
                                            opts.precision_override = val;
                                            changed = true;
                                        }
                                    }
                                });

                                // Compact display checkboxes on 1 line
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .checkbox(&mut opts.show_caption, "Etiqueta")
                                        .on_hover_text("Mostrar etiqueta con resultado en el plano")
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                    if ui
                                        .checkbox(&mut opts.show_segments, "Segmentos")
                                        .on_hover_text("Mostrar cotas parciales por segmento")
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                });

                                if is_vol {
                                    ui.horizontal(|ui| {
                                        ui.label("Profundidad:");
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
                                        ui.label("Altura muro:");
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

                                // Optional Slope / Cost sub-section (collapsed unless already active)
                                let extra_open = (is_len && opts.rise_drop_meters.abs() > 1e-6)
                                    || unit_cost.is_some();
                                let extra_title = if is_len {
                                    "Pendiente y precio unitario"
                                } else {
                                    "Precio unitario ($)"
                                };
                                egui::CollapsingHeader::new(
                                    RichText::new(extra_title)
                                        .size(11.0)
                                        .color(theme.text_secondary),
                                )
                                .id_salt("props_meas_extra")
                                .default_open(extra_open)
                                .show(ui, |ui| {
                                    if is_len {
                                        ui.horizontal(|ui| {
                                            ui.label("Desnivel:");
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
                                            .width(ui.available_width())
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
                                        ui.add_space(3.0);
                                    }

                                    ui.horizontal(|ui| {
                                        let mut has_cost = unit_cost.is_some();
                                        if ui.checkbox(&mut has_cost, "Precio unit.:").changed() {
                                            unit_cost = if has_cost { Some(0.0) } else { None };
                                            changed = true;
                                        }
                                        if let Some(uc) = &mut unit_cost
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
                                });
                            }
                            m.unit_cost = unit_cost;
                        }
                    });
            }

            // ── 3. Apariencia (compact, context-aware) ───────────────────────
            egui::CollapsingHeader::new(RichText::new("Apariencia").strong().size(12.0))
                .id_salt("props_sec_style")
                .default_open(true)
                .show(ui, |ui| {
                    // Quick color swatches + custom picker in one row
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        let mut rgb = [
                            m.style.stroke_color[0],
                            m.style.stroke_color[1],
                            m.style.stroke_color[2],
                        ];
                        if ui
                            .color_edit_button_srgb(&mut rgb)
                            .on_hover_text("Color personalizado de trazo y texto")
                            .changed()
                        {
                            m.style.stroke_color = [rgb[0], rgb[1], rgb[2], 255];
                            m.style.text_color = m.style.stroke_color;
                            changed = true;
                        }
                        for &c in QUICK_COLORS {
                            let col = Color32::from_rgb(c[0], c[1], c[2]);
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                            let is_cur = m.style.stroke_color[0..3] == c[0..3];
                            ui.painter().rect(
                                r,
                                CornerRadius::same(3),
                                col,
                                Stroke::new(
                                    if is_cur { 1.8 } else { 0.8 },
                                    if is_cur { Color32::WHITE } else { theme.border },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            if resp.clicked() {
                                m.style.stroke_color = c;
                                m.style.text_color = c;
                                changed = true;
                            }
                        }
                    });

                    ui.add_space(4.0);

                    // Grosor & Tamaño texto side-by-side
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Grosor:").size(11.0).color(theme.text_secondary));
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
                        ui.add_space(6.0);
                        ui.label(RichText::new("Texto:").size(11.0).color(theme.text_secondary));
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
                    });

                    // Line style pills (when applicable)
                    if supports_line_style(&m.kind) {
                        ui.add_space(2.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                            for (ls, sym, tip) in [
                                (LineStyle::Solid, "───", "Línea continua"),
                                (LineStyle::Dashed, "- - -", "Línea discontinua"),
                                (LineStyle::Dotted, "····", "Línea punteada"),
                                (LineStyle::DashDot, "·-·-", "Trazo y punto"),
                            ] {
                                if segmented_pill(ui, sym, m.style.line_style == ls, tip, theme)
                                    .clicked()
                                {
                                    m.style.line_style = ls;
                                    changed = true;
                                }
                            }
                        });
                    }

                    // Fill & Hatch ONLY for fillable/hatchable markups
                    let show_fill = supports_fill(&m.kind);
                    let show_hatch = supports_hatch(&m.kind);
                    let show_box = matches!(m.kind, MarkupKind::TextBox { .. });
                    let show_note = matches!(m.kind, MarkupKind::Note { .. });
                    let show_count = matches!(m.kind, MarkupKind::MeasureCount { .. });

                    if show_fill || show_hatch || show_box || show_note || show_count {
                        ui.add_space(4.0);
                        egui::Grid::new("props_style_extra_grid")
                            .num_columns(2)
                            .spacing([8.0, 5.0])
                            .show(ui, |ui| {
                                if show_fill {
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

                                    if m.style.fill_color.is_some() {
                                        ui.label("Opacidad:");
                                        if ui
                                            .add(egui::Slider::new(
                                                &mut m.style.fill_opacity,
                                                0.05..=1.0,
                                            ))
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                        ui.end_row();
                                    }
                                }

                                if show_hatch {
                                    ui.label("Achurado:");
                                    egui::ComboBox::from_id_salt("prop_hatch")
                                        .selected_text(m.style.hatch.label())
                                        .show_ui(ui, |ui| {
                                            for &h in HatchPattern::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut m.style.hatch,
                                                        h,
                                                        h.label(),
                                                    )
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                            }
                                        });
                                    ui.end_row();
                                }

                                if show_box {
                                    ui.label("Forma caja:");
                                    egui::ComboBox::from_id_salt("prop_box_shape")
                                        .selected_text(m.style.box_shape.label())
                                        .show_ui(ui, |ui| {
                                            for &bs in BoxShape::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut m.style.box_shape,
                                                        bs,
                                                        bs.label(),
                                                    )
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                            }
                                        });
                                    ui.end_row();
                                }

                                if show_note {
                                    ui.label("Ícono nota:");
                                    egui::ComboBox::from_id_salt("prop_note_icon")
                                        .selected_text(m.style.note_icon.label())
                                        .show_ui(ui, |ui| {
                                            for &ni in NoteIcon::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut m.style.note_icon,
                                                        ni,
                                                        ni.label(),
                                                    )
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                            }
                                        });
                                    ui.end_row();
                                }

                                if show_count {
                                    ui.label("Símbolo:");
                                    egui::ComboBox::from_id_salt("prop_count_style")
                                        .selected_text(m.style.count_style.label())
                                        .show_ui(ui, |ui| {
                                            for &cs in CountStyle::ALL {
                                                if ui
                                                    .selectable_value(
                                                        &mut m.style.count_style,
                                                        cs,
                                                        cs.label(),
                                                    )
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                            }
                                        });
                                    ui.end_row();
                                }
                            });
                    }
                });

            // ── 4. Detalles y Comentarios (collapsed by default for measurements) ──
            egui::CollapsingHeader::new(RichText::new("Detalles y Comentarios").strong().size(12.0))
                .id_salt("props_sec_details")
                .default_open(default_details_open(&m))
                .show(ui, |ui| {
                    let field_w = (ui.available_width() - 68.0).max(90.0);
                    egui::Grid::new("props_meta_grid")
                        .num_columns(2)
                        .spacing([8.0, 5.0])
                        .show(ui, |ui| {
                            ui.label("Asunto:");
                            if ui
                                .add(egui::TextEdit::singleline(&mut m.subject).desired_width(field_w))
                                .changed()
                            {
                                changed = true;
                            }
                            ui.end_row();

                            ui.label("Etiqueta:");
                            if ui
                                .add(egui::TextEdit::singleline(&mut m.label).desired_width(field_w))
                                .changed()
                            {
                                changed = true;
                            }
                            ui.end_row();

                            ui.label("Autor:");
                            if ui
                                .add(egui::TextEdit::singleline(&mut m.author).desired_width(field_w))
                                .changed()
                            {
                                changed = true;
                            }
                            ui.end_row();

                            ui.label("Estado:");
                            egui::ComboBox::from_id_salt("prop_status")
                                .selected_text(m.status.label())
                                .width(field_w)
                                .show_ui(ui, |ui| {
                                    for &st in MarkupStatus::ALL {
                                        if ui.selectable_value(&mut m.status, st, st.label()).changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            ui.end_row();

                            ui.label("Capa:");
                            egui::ComboBox::from_id_salt("prop_layer")
                                .selected_text(&m.layer)
                                .width(field_w)
                                .show_ui(ui, |ui| {
                                    for ln in &layer_names {
                                        if ui
                                            .selectable_value(&mut m.layer, ln.clone(), ln)
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                            ui.end_row();
                        });

                    ui.add_space(4.0);
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
                        let rw = (ui.available_width() - 78.0).max(70.0);
                        ui.add(
                            egui::TextEdit::singleline(reply_draft)
                                .hint_text("Agregar respuesta...")
                                .desired_width(rw),
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
                });

            // ── 4b. Columnas personalizadas y fórmulas (Bluebeam-style) ─────
            egui::CollapsingHeader::new(
                RichText::new(format!(
                    "Columnas y Fórmulas ({})",
                    custom_columns.len()
                ))
                .strong()
                .size(12.0),
            )
            .id_salt("props_sec_custom_cols")
            .default_open(!custom_columns.is_empty())
            .show(ui, |ui| {
                let metrics_opt = page
                    .as_ref()
                    .map(|p| m.compute_metrics(&p.scale, &p.viewports));
                let default_metrics = crate::model::markup::ComputedMetrics::default();
                let metrics_ref = metrics_opt.as_ref().unwrap_or(&default_metrics);
                let default_scale = crate::model::scale::PageScale::default();
                let (scale_ref, viewports_ref) = page
                    .as_ref()
                    .map(|p| (&p.scale, p.viewports.as_slice()))
                    .unwrap_or((&default_scale, &[]));

                if custom_columns.is_empty() {
                    ui.label(
                        RichText::new("Sin columnas personalizadas definidas.")
                            .size(11.0)
                            .color(theme.text_muted),
                    );
                } else {
                    egui::Grid::new("props_custom_cols_grid")
                        .num_columns(2)
                        .spacing([8.0, 5.0])
                        .show(ui, |ui| {
                            for col in &custom_columns {
                                ui.label(
                                    RichText::new(&col.name)
                                        .size(11.0)
                                        .color(theme.text_secondary),
                                );
                                match &col.kind {
                                    CustomColumnKind::Text { default_value } => {
                                        let mut s = match m.custom_values.get(&col.id) {
                                            Some(CustomColumnValue::Text(v)) => v.clone(),
                                            _ => default_value.clone(),
                                        };
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut s)
                                                    .desired_width(135.0),
                                            )
                                            .changed()
                                        {
                                            m.custom_values
                                                .insert(col.id, CustomColumnValue::Text(s));
                                            changed = true;
                                        }
                                    }
                                    CustomColumnKind::Number {
                                        default_value,
                                        prefix,
                                        suffix,
                                        decimals,
                                        ..
                                    } => {
                                        let mut v = match m.custom_values.get(&col.id) {
                                            Some(CustomColumnValue::Number(n)) => *n,
                                            _ => *default_value,
                                        };
                                        let mut dv = egui::DragValue::new(&mut v)
                                            .speed(0.1)
                                            .max_decimals(*decimals);
                                        if !prefix.is_empty() {
                                            dv = dv.prefix(prefix);
                                        }
                                        if !suffix.is_empty() {
                                            dv = dv.suffix(suffix);
                                        }
                                        if ui.add(dv).changed() {
                                            m.custom_values
                                                .insert(col.id, CustomColumnValue::Number(v));
                                            changed = true;
                                        }
                                    }
                                    CustomColumnKind::Choice { items, .. } => {
                                        let mut cur = match m.custom_values.get(&col.id) {
                                            Some(CustomColumnValue::Text(s)) => s.clone(),
                                            _ => String::new(),
                                        };
                                        let sel_lbl = if cur.is_empty() {
                                            "—"
                                        } else {
                                            cur.as_str()
                                        };
                                        egui::ComboBox::from_id_salt(("props_cc_ch", col.id))
                                            .selected_text(sel_lbl)
                                            .width(135.0)
                                            .show_ui(ui, |ui| {
                                                if ui
                                                    .selectable_value(
                                                        &mut cur,
                                                        String::new(),
                                                        "—",
                                                    )
                                                    .changed()
                                                {
                                                    m.custom_values.remove(&col.id);
                                                    changed = true;
                                                }
                                                for opt in items {
                                                    let disp = if let Some(nv) = opt.numeric_value {
                                                        format!("{} ({nv})", opt.label)
                                                    } else {
                                                        opt.label.clone()
                                                    };
                                                    if ui
                                                        .selectable_value(
                                                            &mut cur,
                                                            opt.label.clone(),
                                                            disp,
                                                        )
                                                        .changed()
                                                    {
                                                        m.custom_values.insert(
                                                            col.id,
                                                            CustomColumnValue::Text(
                                                                opt.label.clone(),
                                                            ),
                                                        );
                                                        changed = true;
                                                    }
                                                }
                                            });
                                    }
                                    CustomColumnKind::Checkmark { default_value } => {
                                        let mut b = match m.custom_values.get(&col.id) {
                                            Some(CustomColumnValue::Bool(v)) => *v,
                                            _ => *default_value,
                                        };
                                        if ui.checkbox(&mut b, "").changed() {
                                            m.custom_values
                                                .insert(col.id, CustomColumnValue::Bool(b));
                                            changed = true;
                                        }
                                    }
                                    CustomColumnKind::Formula { expression, .. } => {
                                        let cell = evaluate_column_cell(
                                            col,
                                            &custom_columns,
                                            &m,
                                            metrics_ref,
                                            scale_ref,
                                            viewports_ref,
                                        );
                                        let color = if cell.error.is_some() {
                                            theme.accent_amber
                                        } else {
                                            theme.accent_green
                                        };
                                        let tip = if let Some(err) = cell.error {
                                            format!("Fórmula: {expression}\nError: {err}")
                                        } else {
                                            format!("Fórmula: {expression}")
                                        };
                                        ui.label(
                                            RichText::new(if cell.display.is_empty() {
                                                "—".to_string()
                                            } else {
                                                cell.display
                                            })
                                            .strong()
                                            .size(11.5)
                                            .color(color),
                                        )
                                        .on_hover_text(tip);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                }
                ui.add_space(4.0);
                if ui
                    .small_button("⚙ Gestionar columnas y fórmulas...")
                    .clicked()
                {
                    actions.push(UiAction::OpenDialog(DialogRequest::ManageCustomColumns));
                }
            });

            // ── 4c. Fotos de terreno / Capturas (Markup Capture) ────────────
            egui::CollapsingHeader::new(
                RichText::new(format!("Fotos de terreno ({})", m.attachments.len()))
                    .strong()
                    .size(12.0),
            )
            .id_salt("props_sec_photos")
            .default_open(!m.attachments.is_empty())
            .show(ui, |ui| {
                if ui
                    .add_enabled(
                        !was_locked,
                        egui::Button::new(RichText::new("📷 + Adjuntar foto local...").size(11.5)),
                    )
                    .on_hover_text("Adjuntar foto de obra (PNG/JPG) a esta marca")
                    .clicked()
                {
                    actions.push(UiAction::AttachPhotoToSelected);
                }
                if m.attachments.is_empty() {
                    ui.label(
                        RichText::new("Sin fotos adjuntas.")
                            .size(11.0)
                            .color(theme.text_muted),
                    );
                } else {
                    ui.add_space(4.0);
                    let mut del_att: Option<Uuid> = None;
                    for att in &mut m.attachments {
                        egui::Frame::new()
                            .fill(theme.card_bg)
                            .stroke(Stroke::new(1.0, theme.border))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(egui::Margin::same(5))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(tex) = image_textures.get(&att.id) {
                                        let sz = tex.size_vec2();
                                        let thumb_h = 44.0_f32;
                                        let thumb_w = (thumb_h * sz.x / sz.y.max(1.0)).clamp(36.0, 72.0);
                                        let img_resp = ui
                                            .add(
                                                egui::Image::new((
                                                    tex.id(),
                                                    Vec2::new(thumb_w, thumb_h),
                                                ))
                                                .sense(Sense::click()),
                                            )
                                            .on_hover_text("Clic para ampliar foto");
                                        if img_resp.clicked() {
                                            actions.push(UiAction::OpenDialog(
                                                DialogRequest::PhotoPreview {
                                                    attachment_id: att.id,
                                                    caption: att.caption.clone(),
                                                },
                                            ));
                                        }
                                    }
                                    ui.vertical(|ui| {
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut att.caption)
                                                    .desired_width(120.0),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(
                                                    att.captured_at
                                                        .format("%d/%m/%Y %H:%M")
                                                        .to_string(),
                                                )
                                                .size(10.0)
                                                .color(theme.text_muted),
                                            );
                                            if !was_locked
                                                && ui
                                                    .small_button("✕")
                                                    .on_hover_text("Quitar foto")
                                                    .clicked()
                                            {
                                                del_att = Some(att.id);
                                            }
                                        });
                                    });
                                });
                            });
                        ui.add_space(3.0);
                    }
                    if let Some(aid) = del_att {
                        actions.push(UiAction::DeleteAttachment {
                            markup_id: m.id,
                            attachment_id: aid,
                        });
                    }
                }
            });

            // ── 5. Compact Footer Actions ────────────────────────────────────
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                if ui.small_button("↑ Frente").clicked() {
                    actions.push(UiAction::BringToFront);
                }
                if ui.small_button("↓ Atrás").clicked() {
                    actions.push(UiAction::SendToBack);
                }
                if ui
                    .small_button("+ Caja")
                    .on_hover_text("Guardar estilo en Caja de herramientas")
                    .clicked()
                {
                    actions.push(UiAction::AddSelectedToToolChest);
                }
                if ui
                    .small_button("Usar pred.")
                    .on_hover_text("Usar este estilo como predeterminado para futuras marcas")
                    .clicked()
                {
                    actions.push(UiAction::SetCurrentStyleAsDefault);
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Eliminar")
                                .size(11.0)
                                .color(theme.accent_red),
                        )
                        .small(),
                    )
                    .clicked()
                {
                    actions.push(UiAction::DeleteSelected);
                }
            });
        });

    if changed {
        let locked_changed = m.locked != orig.locked;
        if was_locked && !locked_changed {
            // Markup or its layer is locked; ignore edits other than toggling lock
            return;
        }
        let now = Utc::now();
        m.modified_at = now;
        tab.push_undo();
        tab.ensure_layer(&m.layer);
        let selected_ids = tab.selected_ids.clone();
        let locked_layers: std::collections::HashSet<String> = tab
            .layers
            .iter()
            .filter(|l| l.locked)
            .map(|l| l.name.clone())
            .collect();
        if was_locked {
            // Only lock flag toggled
            tab.markups[idx].locked = m.locked;
            tab.markups[idx].modified_at = now;
        } else {
            if m.text_content() != orig.text_content() || m.style.font_size != orig.style.font_size {
                fit_callout_text(ui.ctx(), &mut m, false);
            }
            tab.markups[idx] = m.clone();
        }

        // Propagate only the fields that actually changed to any additional selected markups
        if selected_ids.len() > 1 {
            let subject_changed = !was_locked && m.subject != orig.subject;
            let label_changed = !was_locked && m.label != orig.label;
            let author_changed = !was_locked && m.author != orig.author;
            let status_changed = !was_locked && m.status != orig.status;
            let layer_changed = !was_locked && m.layer != orig.layer;
            let unit_cost_changed = !was_locked && m.unit_cost != orig.unit_cost;

            let s_new = &m.style;
            let s_old = &orig.style;
            let o_new = m.measurement_opts();
            let o_old = orig.measurement_opts();

            for other in &mut tab.markups {
                if other.id == first_id || !selected_ids.contains(&other.id) {
                    continue;
                }
                let other_layer_locked = locked_layers.contains(&other.layer);
                let mut touched = false;
                if locked_changed && !other_layer_locked {
                    other.locked = m.locked;
                    touched = true;
                }
                if other.locked || other_layer_locked {
                    if touched {
                        other.modified_at = now;
                    }
                    continue;
                }
                if subject_changed {
                    other.subject = m.subject.clone();
                    touched = true;
                }
                if label_changed {
                    other.label = m.label.clone();
                    touched = true;
                }
                if author_changed {
                    other.author = m.author.clone();
                    touched = true;
                }
                if status_changed {
                    other.status = m.status;
                    touched = true;
                }
                if layer_changed {
                    other.layer = m.layer.clone();
                    touched = true;
                }
                if unit_cost_changed && other.is_measurement() {
                    other.unit_cost = m.unit_cost;
                    touched = true;
                }

                // Diff style fields
                if s_new.stroke_color != s_old.stroke_color {
                    other.style.stroke_color = s_new.stroke_color;
                    touched = true;
                }
                if s_new.text_color != s_old.text_color {
                    other.style.text_color = s_new.text_color;
                    touched = true;
                }
                if s_new.fill_color != s_old.fill_color {
                    other.style.fill_color = s_new.fill_color;
                    touched = true;
                }
                if (s_new.fill_opacity - s_old.fill_opacity).abs() > f32::EPSILON {
                    other.style.fill_opacity = s_new.fill_opacity;
                    touched = true;
                }
                if (s_new.line_width - s_old.line_width).abs() > f32::EPSILON {
                    other.style.line_width = s_new.line_width;
                    touched = true;
                }
                if s_new.line_style != s_old.line_style {
                    other.style.line_style = s_new.line_style;
                    touched = true;
                }
                if s_new.hatch != s_old.hatch {
                    other.style.hatch = s_new.hatch;
                    touched = true;
                }
                if s_new.start_end != s_old.start_end {
                    other.style.start_end = s_new.start_end;
                    touched = true;
                }
                if s_new.end_end != s_old.end_end {
                    other.style.end_end = s_new.end_end;
                    touched = true;
                }
                if (s_new.font_size - s_old.font_size).abs() > f32::EPSILON {
                    other.style.font_size = s_new.font_size;
                    fit_callout_text(ui.ctx(), other, false);
                    touched = true;
                }
                if s_new.box_shape != s_old.box_shape {
                    other.style.box_shape = s_new.box_shape;
                    touched = true;
                }
                if s_new.note_icon != s_old.note_icon {
                    other.style.note_icon = s_new.note_icon;
                    touched = true;
                }
                if s_new.count_style != s_old.count_style {
                    other.style.count_style = s_new.count_style;
                    touched = true;
                }

                // Diff measurement options
                if let (Some(n), Some(o)) = (o_new, o_old)
                    && let Some(target_opts) = other.measurement_opts_mut()
                {
                    if n.show_caption != o.show_caption {
                        target_opts.show_caption = n.show_caption;
                        touched = true;
                    }
                    if n.show_segments != o.show_segments {
                        target_opts.show_segments = n.show_segments;
                        touched = true;
                    }
                    if (n.depth_meters - o.depth_meters).abs() > 1e-9 {
                        target_opts.depth_meters = n.depth_meters;
                        touched = true;
                    }
                    if (n.rise_drop_meters - o.rise_drop_meters).abs() > 1e-9 {
                        target_opts.rise_drop_meters = n.rise_drop_meters;
                        touched = true;
                    }
                    if n.length_unit_override != o.length_unit_override {
                        target_opts.length_unit_override = n.length_unit_override;
                        touched = true;
                    }
                    if n.precision_override != o.precision_override {
                        target_opts.precision_override = n.precision_override;
                        touched = true;
                    }
                    if n.slope_format_override != o.slope_format_override {
                        target_opts.slope_format_override = n.slope_format_override;
                        touched = true;
                    }
                }

                if touched {
                    other.modified_at = now;
                }
            }
        }
    }
}
