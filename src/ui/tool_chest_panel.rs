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
        ui.spacing_mut().item_spacing.x = 4.0;
        let search_w = (ui.available_width() - 56.0).max(80.0);
        ui.add(
            egui::TextEdit::singleline(search)
                .hint_text("Buscar símbolo...")
                .desired_width(search_w),
        );
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
        if ui
            .small_button("↺")
            .on_hover_text("Restaurar conjuntos predeterminados de RedBIM")
            .clicked()
        {
            *chest = ToolChest::default_chilean_sets();
            chest.save();
        }
    });
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
                                } else if resp.hovered() {
                                    Stroke::new(1.2, theme.border_strong)
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

                            let quick_key = if is_my && *real_idx < 9 {
                                Some(real_idx + 1)
                            } else {
                                None
                            };
                            let resp = resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_ui(|ui| {
                                    draw_tool_hover_card(ui, item, quick_key, theme);
                                });
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
    compact_toggled
}

fn draw_tool_hover_card(
    ui: &mut egui::Ui,
    item: &ToolChestItem,
    quick_key: Option<usize>,
    theme: &RedBimTheme,
) {
    ui.set_min_width(220.0);
    ui.set_max_width(290.0);

    let [r, g, b, _] = item.style.stroke_color;
    let swatch_col = Color32::from_rgb(r, g, b);

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // Header: color dot + tool name + optional quick keycap (1..9)
        ui.horizontal(|ui| {
            let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
            ui.painter().circle_filled(dot_rect.center(), 4.5, swatch_col);
            ui.label(
                RichText::new(&item.name)
                    .strong()
                    .size(13.0)
                    .color(theme.text_primary),
            );
            if let Some(key) = quick_key {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_keycap_pill(ui, &format!("Tecla {key}"), theme);
                });
            }
        });

        // Base tool type + tool shortcut keycap
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(item.tool.name_es())
                    .size(11.5)
                    .color(theme.text_secondary),
            );
            if let Some(sc) = item.tool.shortcut_hint() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_keycap_pill(ui, sc, theme);
                });
            }
        });

        // Subject / default text if informative
        if !item.subject.is_empty() && item.subject != item.name {
            ui.label(
                RichText::new(format!("Asunto: {}", item.subject))
                    .size(11.0)
                    .color(theme.text_muted),
            );
        }
        if !item.default_text.is_empty() {
            ui.label(
                RichText::new(format!("Texto: \"{}\"", item.default_text))
                    .size(11.0)
                    .color(theme.text_muted),
            );
        }

        // Mode & stroke metadata pills
        ui.horizontal(|ui| {
            let (mode_lbl, mode_col) = if item.properties_mode {
                ("Modo Propiedades", theme.accent_cyan)
            } else {
                ("Modo Dibujo", theme.accent_amber)
            };
            egui::Frame::NONE
                .fill(mode_col.gamma_multiply(0.16))
                .stroke(Stroke::new(0.8, mode_col.gamma_multiply(0.55)))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new(mode_lbl).size(10.0).strong().color(mode_col));
                });

            ui.label(
                RichText::new(format!("Trazo {:.1} pt", item.style.line_width))
                    .size(10.5)
                    .color(theme.text_muted),
            );
        });

        ui.separator();
        ui.label(
            RichText::new("Clic: activar  •  Clic derecho: modo / quitar")
                .size(10.0)
                .color(theme.text_muted),
        );
    });
}

fn draw_keycap_pill(ui: &mut egui::Ui, text: &str, theme: &RedBimTheme) {
    egui::Frame::NONE
        .fill(theme.control_bg)
        .stroke(Stroke::new(0.8, theme.border_strong))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(10.0)
                    .monospace()
                    .color(theme.text_secondary),
            );
        });
}
