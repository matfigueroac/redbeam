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
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Lista de marcas vacía").color(theme.text_muted));
        return;
    };

    // Toolbar header
    ui.horizontal(|ui| {
        ui.label(RichText::new("Lista de marcas").strong().size(12.5));
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut state.filter_text)
                .hint_text("Filtrar por asunto, comentario, autor...")
                .desired_width(210.0),
        );
        egui::ComboBox::from_id_salt("ml_status_filter")
            .selected_text(
                state
                    .status_filter
                    .map(|s| s.label())
                    .unwrap_or("Todos los estados"),
            )
            .width(135.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.status_filter, None, "Todos los estados");
                for &st in MarkupStatus::ALL {
                    ui.selectable_value(&mut state.status_filter, Some(st), st.label());
                }
            });
        ui.checkbox(&mut state.current_page_only, "Solo página actual");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Exportar CSV (Chile ;)").clicked() {
                actions.push(UiAction::ExportCsv(CsvDelimiter::Semicolon));
            }
            if ui.button("Insertar Leyenda").clicked() {
                if let Some(p) = tab.active_page() {
                    actions.push(UiAction::InsertLegendOnCurrentPage(
                        crate::geometry::math::Point2::new(p.width_pt * 0.08, p.height_pt * 0.12),
                    ));
                }
            }
        });
    });

    ui.separator();

    let cur_page = tab.active_page_index();
    let q = state.filter_text.trim().to_lowercase();

    let mut rows: Vec<(Uuid, usize, String, String, String, String, MarkupStatus, String, String, Option<f64>, Option<f64>, Option<f64>, Option<usize>)> =
        Vec::new();

    let mut sum_len_m = 0.0;
    let mut sum_area_m2 = 0.0;
    let mut sum_vol_m3 = 0.0;
    let mut sum_count = 0usize;

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

        let meas_str = metrics.primary_display.unwrap_or_default();
        rows.push((
            m.id,
            m.page_index,
            m.type_name_es().to_string(),
            m.subject.clone(),
            m.label.clone(),
            m.author.clone(),
            m.status,
            m.layer.clone(),
            m.comment.clone(),
            metrics.length_m,
            metrics.area_m2,
            metrics.volume_m3,
            metrics.count,
        ));
        let _ = meas_str;
    }

    rows.sort_by(|a, b| {
        let ord = match state.sort_col {
            SortColumn::Page => a.1.cmp(&b.1),
            SortColumn::Type => a.2.cmp(&b.2),
            SortColumn::Subject => a.3.cmp(&b.3),
            SortColumn::Author => a.5.cmp(&b.5),
            SortColumn::Status => a.6.label().cmp(b.6.label()),
            SortColumn::Measurement => a
                .9
                .or(a.10)
                .or(a.11)
                .unwrap_or(0.0)
                .partial_cmp(&b.9.or(b.10).or(b.11).unwrap_or(0.0))
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
            (SortColumn::Measurement, "Medición", 120.0),
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
            for (id, pg, kind_s, subj, label, author, st, layer, comment, lm, am2, vm3, cnt) in
                &rows
            {
                let is_sel = selected_ids.contains(id);
                let page_scale = &tab.pages[*pg].scale;
                let meas_txt = if let Some(v) = vm3 {
                    format_volume(*v, page_scale.volume_unit, page_scale.precision)
                } else if let Some(v) = am2 {
                    format_area(*v, page_scale.area_unit, page_scale.precision)
                } else if let Some(v) = lm {
                    format_length(*v, page_scale.display_unit, page_scale.precision)
                } else if let Some(c) = cnt {
                    format!("{c} und")
                } else {
                    "—".to_string()
                };

                ui.horizontal(|ui| {
                    let row_lbl = format!("{kind_s}");
                    if ui
                        .add_sized(
                            [96.0, 20.0],
                            egui::Button::new(RichText::new(row_lbl).size(11.5))
                                .selected(is_sel)
                                .frame(is_sel),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::JumpToMarkup(*id));
                    }

                    let subj_disp = if label.is_empty() {
                        subj.clone()
                    } else {
                        format!("{subj} ({label})")
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
                        actions.push(UiAction::JumpToMarkup(*id));
                    }

                    ui.add_sized(
                        [42.0, 20.0],
                        egui::Label::new(RichText::new(format!("{}", pg + 1)).size(11.5)),
                    );
                    ui.add_sized(
                        [95.0, 20.0],
                        egui::Label::new(RichText::new(author).size(11.5)).truncate(),
                    );

                    let mut st_cur = *st;
                    let bc = st_cur.badge_color();
                    egui::ComboBox::from_id_salt(("ml_st", *id))
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
                                    status_update = Some((*id, st_cur));
                                }
                            }
                        });

                    ui.add_sized(
                        [120.0, 20.0],
                        egui::Label::new(
                            RichText::new(meas_txt)
                                .size(11.5)
                                .strong()
                                .color(theme.accent_cyan),
                        ),
                    );

                    let text_preview = format!("[{layer}] {}", comment.replace('\n', " "));
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
                        actions.push(UiAction::JumpToMarkup(*id));
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
            ui.label(
                RichText::new(format!(
                    "Long: {}   ·   Área: {}   ·   Vol: {}   ·   Conteo: {} und",
                    format_length(sum_len_m, s.display_unit, s.precision),
                    format_area(sum_area_m2, s.area_unit, s.precision),
                    format_volume(sum_vol_m3, s.volume_unit, s.precision),
                    sum_count
                ))
                .strong()
                .size(11.5)
                .color(theme.accent_cyan),
            );
        });
    }
}
