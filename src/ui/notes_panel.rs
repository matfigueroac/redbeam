use crate::model::document::DocumentTab;
use crate::model::markup::{Markup, MarkupKind};
use crate::ui::actions::UiAction;
use crate::ui::theme::RedBimTheme;
use eframe::egui;

fn text_entries(tab: &DocumentTab) -> Vec<(&Markup, String)> {
    let mut entries = Vec::new();
    for markup in &tab.markups {
        let extra = match &markup.kind {
            MarkupKind::Stamp { subtitle, .. } => Some(subtitle.as_str()),
            MarkupKind::Image { caption, .. } => Some(caption.as_str()),
            _ => None,
        };
        let mut parts = Vec::new();
        for text in markup
            .text_content()
            .into_iter()
            .chain(extra)
            .chain([markup.comment.as_str(), markup.label.as_str()])
            .chain(markup.replies.iter().map(|reply| reply.text.as_str()))
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            if !parts.contains(&text) {
                parts.push(text);
            }
        }
        if tab.pages.get(markup.page_index).is_some()
            && (!parts.is_empty() || matches!(markup.kind, MarkupKind::Note { .. }))
        {
            entries.push((markup, parts.join("\n")));
        }
    }
    entries.sort_by_key(|(markup, _)| markup.created_at);
    entries
}

pub fn draw_notes_panel(
    ui: &mut egui::Ui,
    tab: Option<&DocumentTab>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label("Abre un PDF para ver sus notas y textos.");
        return;
    };
    let entries = text_entries(tab);
    if entries.is_empty() {
        ui.label(
            egui::RichText::new("Las notas y marcas con texto aparecerán aquí.")
                .color(theme.text_muted),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(("notes_and_text", tab.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            for (index, (markup, text)) in entries.iter().enumerate() {
                let hidden = !tab.is_layer_visible(&markup.layer);
                let page = tab.pages[markup.page_index].display_label();
                let suffix = if hidden { " · Capa oculta" } else { "" };
                let title = format!(
                    "{}. {} · {page}{suffix}\n",
                    index + 1,
                    markup.type_name_es()
                );
                let preview = if text.is_empty() {
                    "Nota vacía".into()
                } else {
                    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let mut chars = compact.chars();
                    let mut preview: String = chars.by_ref().take(120).collect();
                    if chars.next().is_some() {
                        preview.push('…');
                    }
                    preview
                };
                let mut row = egui::text::LayoutJob::default();
                row.append(
                    &title,
                    0.0,
                    egui::TextFormat::simple(egui::FontId::proportional(11.5), theme.accent_cyan),
                );
                row.append(
                    &preview,
                    0.0,
                    egui::TextFormat::simple(egui::FontId::proportional(13.0), theme.text_primary),
                );
                let response = ui
                    .push_id(markup.id, |ui| {
                        ui.add(
                            egui::Button::new(row)
                                .wrap()
                                .min_size(egui::vec2(ui.available_width(), 44.0))
                                .selected(tab.selected_ids.contains(&markup.id)),
                        )
                    })
                    .inner;
                let clicked = response.clicked();
                if !text.is_empty() {
                    response.on_hover_ui(|ui| {
                        ui.set_max_width(320.0);
                        ui.label(text);
                    });
                }
                if clicked {
                    actions.push(UiAction::LocateTextMarkup(markup.id));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::math::{Point2, Rect2};
    use crate::model::document::PageModel;
    use crate::model::markup::MarkupStyle;

    #[test]
    fn lists_text_marks_in_creation_order_across_pages() {
        let mut tab = DocumentTab::new_with_pages(
            "Prueba",
            None,
            Vec::new(),
            vec![
                PageModel::new(0, 600.0, 400.0),
                PageModel::new(1, 600.0, 400.0),
            ],
        );
        let rect = Rect2::from_xywh(50.0, 50.0, 80.0, 30.0);
        let mut note = Markup::new(
            1,
            "Nota",
            "",
            MarkupStyle::default(),
            MarkupKind::Note {
                pos: Point2::new(100.0, 100.0),
            },
        );
        note.comment = "Revisar fijaciones".into();
        let textbox = Markup::new(
            0,
            "Texto",
            "",
            MarkupStyle::default(),
            MarkupKind::TextBox {
                rect,
                text: "Sala técnica".into(),
            },
        );
        note.created_at = textbox.created_at - chrono::Duration::seconds(1);
        let mut rectangle = Markup::new(
            0,
            "Figura",
            "",
            MarkupStyle::default(),
            MarkupKind::Rectangle { rect },
        );
        rectangle.label = "Sector norte".into();
        tab.markups = vec![
            textbox,
            rectangle.clone(),
            note,
            Markup::new(
                0,
                "Sin texto",
                "",
                MarkupStyle::default(),
                MarkupKind::Rectangle { rect },
            ),
        ];
        let texts: Vec<_> = text_entries(&tab)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(
            texts,
            ["Revisar fijaciones", "Sala técnica", "Sector norte"]
        );
    }
}
