use crate::model::document::DocumentTab;
use crate::ui::actions::UiAction;
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, RichText, Stroke};
use uuid::Uuid;

pub fn draw_document_tabs(
    ui: &mut egui::Ui,
    tabs: &[DocumentTab],
    active: usize,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(current) = tabs.get(active) else {
        return;
    };
    egui::Panel::top("redbim_doc_tabs")
        .frame(
            egui::Frame::new()
                .fill(theme.window_bg)
                .inner_margin(egui::Margin::symmetric(8, 3))
                .stroke(Stroke::new(1.0, theme.border)),
        )
        .show(ui, |ui| {
            let selection_id = ui.scope_id().with("last_active_document");
            let changed = ui.data_mut(|data| {
                let previous = data.get_temp::<Uuid>(selection_id);
                data.insert_temp(selection_id, current.id);
                previous != Some(current.id)
            });
            ui.horizontal(|ui| {
                if ui
                    .button("+")
                    .on_hover_text("Abrir otro PDF (Ctrl+O)")
                    .clicked()
                {
                    actions.push(UiAction::OpenPdfDialog);
                }
                egui::ScrollArea::horizontal()
                    .id_salt("document_tabs_scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (index, tab) in tabs.iter().enumerate() {
                                let selected = index == active;
                                let group = ui.push_id(tab.id, |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    ui.set_max_width(210.0);
                                    if tab.dirty {
                                        ui.label(RichText::new("●").color(theme.accent_amber))
                                            .on_hover_text("Cambios sin guardar");
                                    }
                                    let label = RichText::new(&tab.title).size(12.0);
                                    let response = ui
                                        .add(egui::Button::selectable(selected, label).truncate());
                                    let detail = tab.file_path.as_ref().map_or_else(
                                        || tab.title.clone(),
                                        |path| path.display().to_string(),
                                    );
                                    let detail = if tab.dirty {
                                        format!("{detail}\nCambios sin guardar")
                                    } else {
                                        detail
                                    };
                                    if response.on_hover_text(detail).clicked() {
                                        actions.push(UiAction::SelectTab(index));
                                    }
                                    let close = ui.add(egui::Button::new("×").frame(false));
                                    let close_label = || format!("Cerrar {}", tab.title);
                                    close.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            close_label(),
                                        )
                                    });
                                    if close
                                        .on_hover_ui(|ui| {
                                            ui.label(close_label());
                                        })
                                        .clicked()
                                    {
                                        actions.push(UiAction::CloseTab(index));
                                    }
                                });
                                if selected {
                                    let rect = group.response.rect;
                                    ui.painter().hline(
                                        rect.x_range(),
                                        rect.bottom(),
                                        Stroke::new(2.0, theme.accent_red),
                                    );
                                    if changed {
                                        group.response.scroll_to_me(None);
                                    }
                                }
                            }
                        });
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::settings::ThemeMode;

    #[test]
    fn last_selected_tab_and_open_button_stay_visible_on_a_narrow_strip() {
        let ctx = egui::Context::default();
        let theme = RedBimTheme::from_mode(ThemeMode::Dark);
        let tabs: Vec<_> = (0..16)
            .map(|i| {
                DocumentTab::new_with_pages(format!("Lámina técnica {i}"), None, vec![], vec![])
            })
            .collect();
        let frame = |active, time| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 160.0),
                    )),
                    ..Default::default()
                },
                |ui| draw_document_tabs(ui, &tabs, active, &theme, &mut Vec::new()),
            );
            output.textures_delta.clear();
            output.shapes
        };
        let _ = frame(0, 0.0);
        for i in 1..20 {
            frame(15, i as f64 * 0.1);
        }
        let shapes = frame(15, 2.0);
        for label in ["+", "Lámina técnica 15"] {
            assert!(shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == label && shape.clip_rect.contains(text.pos + text.galley.size() * 0.5))
            }), "Control fuera de la ventana: {label}");
        }
    }
}
