use crate::model::tool_chest::{ToolChest, ToolChestItem};
use crate::ui::actions::UiAction;
use crate::ui::icons::paint_tool_chest_preview;
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

pub fn draw_tool_chest_panel(
    ui: &mut egui::Ui,
    chest: &mut ToolChest,
    search: &mut String,
    active_item_id: Option<uuid::Uuid>,
    compact: &mut bool,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) -> bool {
    let mut compact_toggled = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(RichText::new("Caja de herramientas").strong().size(12.5)).truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("↺")
                .on_hover_text("Restaurar conjuntos predeterminados de RedBIM")
                .clicked()
            {
                *chest = ToolChest::default_chilean_sets();
                chest.save();
            }
            let lbl = if *compact { "⊞" } else { "⊟" };
            if ui
                .small_button(lbl)
                .on_hover_text(if *compact {
                    "Cambiar a vista normal de símbolos"
                } else {
                    "Cambiar a vista compacta de símbolos"
                })
                .clicked()
            {
                *compact = !*compact;
                compact_toggled = true;
            }
        });
    });

    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text("Buscar herramienta o símbolo...")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);

    let q = search.trim().to_lowercase();
    let mut changed = false;

    egui::ScrollArea::vertical()
        .id_salt("tool_chest_scroll")
        .show(ui, |ui| {
            for set_idx in 0..chest.sets.len() {
                let title = chest.sets[set_idx].title.clone();
                let is_my = chest.sets[set_idx].is_my_tools;
                let collapsed = chest.sets[set_idx].collapsed;
                let item_count = chest.sets[set_idx].items.len();

                let header_rect = ui
                    .horizontal(|ui| {
                        let arrow = if collapsed { "▶" } else { "▼" };
                        let btn = ui.selectable_label(
                            false,
                            RichText::new(format!("{arrow}  {title} ({item_count})"))
                                .strong()
                                .size(11.8)
                                .color(theme.text_primary),
                        );
                        if btn.clicked() {
                            chest.sets[set_idx].collapsed = !collapsed;
                            changed = true;
                        }
                    })
                    .response
                    .rect;
                let _ = header_rect;

                if chest.sets[set_idx].collapsed && q.is_empty() {
                    continue;
                }

                let items_snapshot: Vec<(usize, ToolChestItem)> = chest.sets[set_idx]
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| {
                        q.is_empty()
                            || it.name.to_lowercase().contains(&q)
                            || it.subject.to_lowercase().contains(&q)
                            || it.tool.label_es().to_lowercase().contains(&q)
                    })
                    .map(|(i, it)| (i, it.clone()))
                    .collect();

                if items_snapshot.is_empty() {
                    if chest.sets[set_idx].is_recents && q.is_empty() {
                        ui.label(
                            RichText::new("  Sin herramientas recientes aún")
                                .size(11.0)
                                .color(theme.text_muted),
                        );
                    }
                    continue;
                }

                let cell_px = if *compact { 36.0 } else { 44.0 };
                let step_px = cell_px + 7.0;
                let cols = ((ui.available_width() / step_px).floor() as usize).clamp(3, 7);
                let mut remove_idx: Option<usize> = None;
                let mut toggle_mode_idx: Option<usize> = None;

                egui::Grid::new(format!("tc_grid_{set_idx}"))
                    .num_columns(cols)
                    .spacing([5.0, 5.0])
                    .show(ui, |ui| {
                        for (grid_i, (real_idx, item)) in items_snapshot.iter().enumerate() {
                            let is_active = active_item_id == Some(item.id);
                            let card_size = Vec2::splat(cell_px);
                            let (rect, resp) = ui.allocate_exact_size(card_size, Sense::click());

                            if ui.is_rect_visible(rect) {
                                let bg = if is_active {
                                    theme.accent_red_soft
                                } else if resp.hovered() {
                                    theme.control_hover
                                } else {
                                    theme.card_bg
                                };
                                let border = if is_active {
                                    Stroke::new(1.5, theme.accent_red)
                                } else {
                                    Stroke::new(1.0, theme.border)
                                };
                                ui.painter().rect(
                                    rect,
                                    CornerRadius::same(5),
                                    bg,
                                    border,
                                    egui::StrokeKind::Inside,
                                );

                                paint_tool_chest_preview(ui.painter(), rect, item, theme);

                                // Number badge 1..8 on Mis herramientas (matches Bluebeam screenshot!)
                                if is_my && *real_idx < 9 {
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 4.0, rect.top() + 3.0),
                                        egui::Align2::RIGHT_TOP,
                                        format!("{}", real_idx + 1),
                                        egui::FontId::proportional(9.5),
                                        theme.text_muted,
                                    );
                                }
                                // Properties mode indicator badge
                                if item.properties_mode {
                                    ui.painter().text(
                                        egui::pos2(rect.left() + 4.0, rect.bottom() - 3.0),
                                        egui::Align2::LEFT_BOTTOM,
                                        "P",
                                        egui::FontId::proportional(9.0),
                                        theme.accent_cyan,
                                    );
                                }
                            }

                            let tip = if is_my && *real_idx < 9 {
                                format!(
                                    "{} (Atajo {})\n{}\nClic derecho: Modo Dibujo / Propiedades",
                                    item.name,
                                    real_idx + 1,
                                    item.tool.label_es()
                                )
                            } else {
                                format!(
                                    "{}\n{}\nClic derecho: Modo Dibujo / Propiedades",
                                    item.name,
                                    item.tool.label_es()
                                )
                            };

                            let resp = resp.on_hover_text(tip);
                            if resp.clicked() {
                                actions.push(UiAction::ActivateToolChestItem(item.clone()));
                            }
                            resp.context_menu(|ui| {
                                let mode_lbl = if item.properties_mode {
                                    "Cambiar a Modo Dibujo"
                                } else {
                                    "Cambiar a Modo Propiedades"
                                };
                                if ui.button(mode_lbl).clicked() {
                                    toggle_mode_idx = Some(*real_idx);
                                    ui.close();
                                }
                                if ui.button("Eliminar de la caja").clicked() {
                                    remove_idx = Some(*real_idx);
                                    ui.close();
                                }
                            });

                            if (grid_i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });

                if let Some(idx) = toggle_mode_idx
                    && let Some(it) = chest.sets[set_idx].items.get_mut(idx)
                {
                    it.properties_mode = !it.properties_mode;
                    changed = true;
                }
                if let Some(idx) = remove_idx
                    && idx < chest.sets[set_idx].items.len()
                {
                    chest.sets[set_idx].items.remove(idx);
                    changed = true;
                }

                ui.add_space(6.0);
            }
        });

    if changed {
        chest.save();
    }
    let _ = Color32::TRANSPARENT;
    compact_toggled
}
