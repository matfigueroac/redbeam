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
    ui.label(RichText::new("Buscar en plano y marcas").strong().size(13.0));
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
                if txt.to_lowercase().contains(&q) {
                    hits += 1;
                    let snippet = extract_snippet(txt, &q);
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
                            ui.label(RichText::new(snippet).size(11.0).color(theme.text_secondary));
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

fn extract_snippet(text: &str, q_lower: &str) -> String {
    let lower = text.to_lowercase();
    if let Some(pos) = lower.find(q_lower) {
        let start = text[..pos]
            .char_indices()
            .rev()
            .nth(24)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end = text[pos..]
            .char_indices()
            .nth(64)
            .map(|(i, _)| pos + i)
            .unwrap_or(text.len());
        text[start..end].replace(['\n', '\r'], " ").trim().to_string()
    } else {
        String::new()
    }
}
