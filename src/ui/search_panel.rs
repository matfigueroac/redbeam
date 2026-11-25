use crate::model::document::DocumentTab;
use crate::ui::actions::UiAction;
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, CornerRadius, RichText, Stroke};

pub fn draw_search_panel(
    ui: &mut egui::Ui,
    tab: Option<&DocumentTab>,
    query: &mut String,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    ui.label(
        RichText::new("Buscar en plano y marcas")
            .strong()
            .size(13.0),
    );
    ui.add(
        egui::TextEdit::singleline(query)
            .hint_text("Texto en plano, comentario o etiqueta...")
            .desired_width(f32::INFINITY),
    );
    ui.separator();

    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    let q = query.trim().to_lowercase();
    if q.is_empty() {
        ui.label(
            RichText::new("Escriba una palabra o medida (ej. \"PV-01\", \"59.50\", \"faldón\").")
                .size(11.5)
                .color(theme.text_muted),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("search_results_scroll")
        .show(ui, |ui| {
            let mut hits = 0usize;

            // Search markups & comments
            for m in &tab.markups {
                let hay = format!(
                    "{} {} {} {} {}",
                    m.subject,
                    m.label,
                    m.comment,
                    m.author,
                    m.text_content().unwrap_or("")
                )
                .to_lowercase();
                if hay.contains(&q) {
                    hits += 1;
                    let resp = egui::Frame::new()
                        .fill(theme.card_bg)
                        .stroke(Stroke::new(1.0, theme.border))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(format!(
                                    "[Marca · Pág. {}] {}",
                                    m.page_index + 1,
                                    m.subject
                                ))
                                .strong()
                                .size(11.5)
                                .color(theme.accent_cyan),
                            );
                            if !m.comment.is_empty() {
                                ui.label(RichText::new(&m.comment).size(11.0));
                            }
                        })
                        .response;
                    if resp.interact(egui::Sense::click()).clicked() {
                        actions.push(UiAction::JumpToMarkup(m.id));
                    }
                    ui.add_space(4.0);
                }
            }

            // Search extracted PDF page text
            for (idx, txt) in tab.search_text_by_page.iter().enumerate() {
                if let Some(snippet) = extract_snippet(txt, &q) {
                    hits += 1;
                    let resp = egui::Frame::new()
                        .fill(theme.card_bg)
                        .stroke(Stroke::new(1.0, theme.border))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(format!("[Texto PDF · Lámina {}]", idx + 1))
                                    .strong()
                                    .size(11.5)
                                    .color(theme.accent_green),
                            );
                            ui.label(
                                RichText::new(snippet)
                                    .size(11.0)
                                    .color(theme.text_secondary),
                            );
                        })
                        .response;
                    if resp.interact(egui::Sense::click()).clicked() {
                        actions.push(UiAction::JumpToPageText(idx));
                    }
                    ui.add_space(4.0);
                }
            }

            if hits == 0 {
                ui.label(
                    RichText::new("Sin coincidencias encontradas.")
                        .size(11.5)
                        .color(theme.text_muted),
                );
            }
        });
}

fn extract_snippet(text: &str, q_lower: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let pos = lower.find(q_lower)?;
    // Some letters expand in lowercase. Map the hit back to its original character.
    let mut folded_offset = 0;
    let (original, _) = text.char_indices().find(|(_, c)| {
        folded_offset += c.to_lowercase().map(char::len_utf8).sum::<usize>();
        folded_offset > pos
    })?;
    let start = text[..original]
        .char_indices()
        .rev()
        .nth(23)
        .map_or(0, |(i, _)| i);
    let end = text[original..]
        .char_indices()
        .nth(64)
        .map_or(text.len(), |(i, _)| original + i);
    Some(
        text[start..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_preserve_drawing_codes_and_unicode() {
        let text = "İZMİR — ÁREA CUBIERTA\nPV-01";
        assert_eq!(
            extract_snippet(text, "pv-01").as_deref(),
            Some("İZMİR — ÁREA CUBIERTA PV-01")
        );
        assert!(extract_snippet(text, "cimentación").is_none());
    }
}
