use crate::model::document::DocumentTab;
use crate::model::markup::MarkupStatus;
use crate::model::settings::CsvDelimiter;
use crate::model::units::{format_area, format_length, format_volume};
use crate::ui::actions::UiAction;
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortColumn {
    #[default]
    Page,
    Type,
    Subject,
    Author,
    Status,
    Measurement,
    Total,
}

struct MarkupRow {
    id: Uuid,
    page: usize,
    kind: String,
    subject: String,
    label: String,
    author: String,
    status: MarkupStatus,
    layer: String,
    comment: String,
    length_m: Option<f64>,
    area_m2: Option<f64>,
    volume_m3: Option<f64>,
    count: Option<usize>,
    total_cost: Option<f64>,
}

#[derive(Debug, Default)]
pub struct MarkupsListState {
    pub filter_text: String,
    pub status_filter: Option<MarkupStatus>,
    pub current_page_only: bool,
    pub sort_col: SortColumn,
    pub sort_asc: bool,
}

pub fn draw_markups_list_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    state: &mut MarkupsListState,
    bottom_open: &mut bool,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Lista de marcas vacía").color(theme.text_muted));
        return;
    };

    // Toolbar header (responsive wrap so controls never collide on narrower viewports)
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
        ui.label(RichText::new("Lista de marcas").strong().size(12.5));
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut state.filter_text)
                .hint_text("Filtrar...")
                .desired_width(160.0),
        );
        egui::ComboBox::from_id_salt("ml_status_filter")
            .selected_text(
                state
                    .status_filter
                    .map(|s| s.label())
                    .unwrap_or("Todos los estados"),
            )
            .width(125.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.status_filter, None, "Todos los estados");
                for &st in MarkupStatus::ALL {
                    ui.selectable_value(&mut state.status_filter, Some(st), st.label());
                }
            });
        ui.checkbox(&mut state.current_page_only, "Solo pág. actual");

        ui.separator();
        if ui
            .small_button("Leyenda")
            .on_hover_text("Insertar tabla de leyenda de marcas en la página actual")
            .clicked()
            && let Some(p) = tab.active_page()
        {
            actions.push(UiAction::InsertLegendOnCurrentPage(
                crate::geometry::math::Point2::new(p.width_pt * 0.08, p.height_pt * 0.12),
            ));
        }
        if ui
            .small_button("Exportar CSV")
            .on_hover_text("Exportar Lista de marcas a CSV (separador ; para Excel Chile)")
            .clicked()
        {
            actions.push(UiAction::ExportCsv(CsvDelimiter::Semicolon));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("×").size(13.0).color(theme.text_secondary))
                        .frame(false)
                        .min_size(egui::vec2(20.0, 20.0)),
                )
                .on_hover_text("Ocultar Lista de marcas (F8)")
                .clicked()
            {
                *bottom_open = false;
            }
        });
    });

    ui.separator();

    let cur_page = tab.active_page_index();
    let q = state.filter_text.trim().to_lowercase();

    let mut rows: Vec<MarkupRow> = Vec::new();

    let mut sum_len_m = 0.0;
    let mut sum_area_m2 = 0.0;
    let mut sum_vol_m3 = 0.0;
    let mut sum_count = 0usize;
    let mut sum_cost = 0.0;

    for m in &tab.markups {
        if state.current_page_only && m.page_index != cur_page {
            continue;
        }
        if let Some(sf) = state.status_filter
            && m.status != sf
        {
            continue;
        }
        if !q.is_empty() {
            let hay = format!(
                "{} {} {} {} {} {}",
                m.type_name_es(),
                m.subject,
                m.label,
                m.author,
                m.layer,
                m.comment
            )
            .to_lowercase();
            if !hay.contains(&q) {
                continue;
            }
        }
        let Some(page) = tab.pages.get(m.page_index) else {
            continue;
        };
        let metrics = m.compute_metrics(&page.scale, &page.viewports);
        if let Some(v) = metrics.length_m {
            sum_len_m += v;
        }
        if let Some(v) = metrics.area_m2 {
            sum_area_m2 += v;
        }
        if let Some(v) = metrics.volume_m3 {
            sum_vol_m3 += v;
        }
        if let Some(c) = metrics.count {
            sum_count += c;
        }
        let row_cost = m.total_cost(&page.scale, &page.viewports);
        if let Some(tc) = row_cost {
            sum_cost += tc;
        }

        rows.push(MarkupRow {
            id: m.id,
            page: m.page_index,
            kind: m.type_name_es().to_string(),
            subject: m.subject.clone(),
            label: m.label.clone(),
            author: m.author.clone(),
            status: m.status,
            layer: m.layer.clone(),
            comment: m.comment.clone(),
            length_m: metrics.length_m,
            area_m2: metrics.area_m2,
            volume_m3: metrics.volume_m3,
            count: metrics.count,
            total_cost: row_cost,
        });
    }

    rows.sort_by(|a, b| {
        let ord = match state.sort_col {
            SortColumn::Page => a.page.cmp(&b.page),
            SortColumn::Type => a.kind.cmp(&b.kind),
            SortColumn::Subject => a.subject.cmp(&b.subject),
            SortColumn::Author => a.author.cmp(&b.author),
            SortColumn::Status => a.status.label().cmp(b.status.label()),
            SortColumn::Measurement => a
                .length_m
                .or(a.area_m2)
                .or(a.volume_m3)
                .unwrap_or(0.0)
                .partial_cmp(&b.length_m.or(b.area_m2).or(b.volume_m3).unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal),
            SortColumn::Total => a
                .total_cost
                .unwrap_or(0.0)
                .partial_cmp(&b.total_cost.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal),
        };
        if state.sort_asc { ord } else { ord.reverse() }
    });

    let mut status_update: Option<(Uuid, MarkupStatus)> = None;
    let selected_ids = tab.selected_ids.clone();

    // Header row
    ui.horizontal(|ui| {
        for (col, title, w) in [
            (SortColumn::Type, "Tipo", 96.0),
            (SortColumn::Subject, "Asunto", 130.0),
            (SortColumn::Page, "Pág.", 42.0),
            (SortColumn::Author, "Autor", 95.0),
            (SortColumn::Status, "Estado", 100.0),
            (SortColumn::Measurement, "Medición", 115.0),
            (SortColumn::Total, "Total ($)", 85.0),
        ] {
            let arrow = if state.sort_col == col {
                if state.sort_asc { " ▲" } else { " ▼" }
            } else {
                ""
            };
            if ui
                .add_sized(
                    [w, 20.0],
                    egui::Button::new(RichText::new(format!("{title}{arrow}")).strong().size(11.5))
                        .frame(false),
                )
                .clicked()
            {
                if state.sort_col == col {
                    state.sort_asc = !state.sort_asc;
                } else {
                    state.sort_col = col;
                    state.sort_asc = true;
                }
            }
        }
        ui.label(RichText::new("Capa / Comentario").strong().size(11.5));
    });
    ui.separator();

    let scroll_h = (ui.available_height() - 26.0).max(40.0);
    egui::ScrollArea::vertical()
        .id_salt("markups_list_rows")
        .max_height(scroll_h)
        .show(ui, |ui| {
            for row in &rows {
                let is_sel = selected_ids.contains(&row.id);
                let page_scale = &tab.pages[row.page].scale;
                let meas_txt = if let Some(v) = row.volume_m3 {
                    format_volume(v, page_scale.volume_unit, page_scale.precision)
                } else if let Some(v) = row.area_m2 {
                    format_area(v, page_scale.area_unit, page_scale.precision)
                } else if let Some(v) = row.length_m {
                    format_length(v, page_scale.display_unit, page_scale.precision)
                } else if let Some(c) = row.count {
                    format!("{c} und")
                } else {
                    "—".to_string()
                };

                ui.horizontal(|ui| {
                    if ui
                        .add_sized(
                            [96.0, 20.0],
                            egui::Button::new(RichText::new(&row.kind).size(11.5))
                                .selected(is_sel)
                                .frame(is_sel),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::JumpToMarkup(row.id));
                    }

                    let subj_disp = if row.label.is_empty() {
                        row.subject.clone()
                    } else {
                        format!("{} ({})", row.subject, row.label)
                    };
                    if ui
                        .add_sized(
                            [130.0, 20.0],
                            egui::Label::new(RichText::new(subj_disp).size(11.5))
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::JumpToMarkup(row.id));
                    }

                    ui.add_sized(
                        [42.0, 20.0],
                        egui::Label::new(RichText::new(format!("{}", row.page + 1)).size(11.5)),
                    );
                    ui.add_sized(
                        [95.0, 20.0],
                        egui::Label::new(RichText::new(&row.author).size(11.5)).truncate(),
                    );

                    let mut st_cur = row.status;
                    let bc = st_cur.badge_color();
                    egui::ComboBox::from_id_salt(("ml_st", row.id))
                        .selected_text(
                            RichText::new(st_cur.label())
                                .size(11.0)
                                .color(Color32::from_rgb(bc[0], bc[1], bc[2])),
                        )
                        .width(96.0)
                        .show_ui(ui, |ui| {
                            for &opt in MarkupStatus::ALL {
                                if ui
                                    .selectable_value(&mut st_cur, opt, opt.label())
                                    .changed()
                                {
                                    status_update = Some((row.id, st_cur));
                                }
                            }
                        });

                    ui.add_sized(
                        [115.0, 20.0],
                        egui::Label::new(
                            RichText::new(meas_txt)
                                .size(11.5)
                                .strong()
                                .color(theme.accent_cyan),
                        ),
                    );

                    let cost_txt = row
                        .total_cost
                        .map(|c| format!("$ {:.0}", c))
                        .unwrap_or_else(|| "—".to_string());
                    ui.add_sized(
                        [85.0, 20.0],
                        egui::Label::new(
                            RichText::new(cost_txt)
                                .size(11.5)
                                .color(if row.total_cost.is_some() {
                                    theme.accent_green
                                } else {
                                    theme.text_muted
                                }),
                        ),
                    );

                    let text_preview =
                        format!("[{}] {}", row.layer, row.comment.replace('\n', " "));
                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new(text_preview)
                                    .size(11.5)
                                    .color(theme.text_secondary),
                            )
                            .truncate()
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::JumpToMarkup(row.id));
                    }
                });
            }
        });

    if let Some((id, new_st)) = status_update {
        tab.push_undo();
        if let Some(m) = tab.markups.iter_mut().find(|m| m.id == id) {
            m.status = new_st;
            m.modified_at = chrono::Utc::now();
        }
    }

    // Sticky Totals Footer
    ui.separator();
    if let Some(page) = tab.active_page() {
        let s = &page.scale;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("TOTALES ({} marcas):", rows.len()))
                    .strong()
                    .size(11.5),
            );
            let cost_footer = if sum_cost > 0.0 {
                format!("   ·   Total: $ {:.0}", sum_cost)
            } else {
                String::new()
            };
            ui.label(
                RichText::new(format!(
                    "Long: {}   ·   Área: {}   ·   Vol: {}   ·   Conteo: {} und{}",
                    format_length(sum_len_m, s.display_unit, s.precision),
                    format_area(sum_area_m2, s.area_unit, s.precision),
                    format_volume(sum_vol_m3, s.volume_unit, s.precision),
                    sum_count,
                    cost_footer
                ))
                .strong()
                .size(11.5)
                .color(theme.accent_cyan),
            );
        });
    }
}
