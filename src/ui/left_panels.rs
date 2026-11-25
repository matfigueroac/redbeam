use crate::model::document::{BookmarkNode, DocumentTab, LayerItem};
use crate::model::settings::AppSettings;
use crate::model::tool_chest::{ActiveTool, ToolChest};
use crate::model::units::{SheetDisplayUnit, format_page_dimensions};
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::theme::RedBimTheme;
use crate::ui::tool_chest_panel::draw_tool_chest_panel;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, TextureHandle};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPanelTab {
    #[default]
    ToolChest,
    Thumbnails,
    Spaces,
    Layers,
    Bookmarks,
}

impl LeftPanelTab {
    const TABS: [(Self, IconKind); 5] = [
        (Self::ToolChest, IconKind::ToolChest),
        (Self::Thumbnails, IconKind::Thumbnails),
        (Self::Spaces, IconKind::Space),
        (Self::Layers, IconKind::Layers),
        (Self::Bookmarks, IconKind::Bookmarks),
    ];

    pub fn title_es(self) -> &'static str {
        match self {
            Self::ToolChest => "Caja de herramientas",
            Self::Thumbnails => "Miniaturas",
            Self::Spaces => "Espacios / Recintos",
            Self::Layers => "Capas",
            Self::Bookmarks => "Marcadores",
        }
    }
}

pub fn draw_left_rail(
    ui: &mut egui::Ui,
    active_tab: &mut LeftPanelTab,
    left_open: &mut bool,
    theme: &RedBimTheme,
) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (tab, icon) in LeftPanelTab::TABS {
            let selected = *left_open && *active_tab == tab;
            let title = tab.title_es();
            let button = icon_button(ui, 28.0, icon, selected, true, title, theme);
            button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
            if button.clicked() {
                *active_tab = tab;
                *left_open = !selected;
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            let (arrow, tip) = if *left_open {
                ("«", "Plegar panel izquierdo (F9)")
            } else {
                ("»", "Abrir panel izquierdo (F9)")
            };
            let button = ui
                .add(
                    egui::Button::new(RichText::new(arrow).size(14.0).color(theme.text_secondary))
                        .frame(false)
                        .min_size(egui::vec2(28.0, 26.0)),
                )
                .on_hover_text(tip);
            button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
            if button.clicked() {
                *left_open = !*left_open;
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
pub fn draw_left_panel(
    ui: &mut egui::Ui,
    active_tab: LeftPanelTab,
    tab: Option<&mut DocumentTab>,
    chest: &mut ToolChest,
    chest_search: &mut String,
    active_chest_item: Option<Uuid>,
    new_layer_name: &mut String,
    thumb_textures: &HashMap<(Uuid, usize), TextureHandle>,
    settings: &mut AppSettings,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    ui.label(
        RichText::new(active_tab.title_es())
            .strong()
            .size(12.5)
            .color(theme.text_primary),
    );
    ui.separator();

    match active_tab {
        LeftPanelTab::ToolChest => {
            if draw_tool_chest_panel(
                ui,
                chest,
                chest_search,
                active_chest_item,
                &mut settings.compact_tool_chest,
                theme,
                actions,
            ) {
                settings.save();
            }
        }
        LeftPanelTab::Thumbnails => {
            draw_thumbnails_panel(
                ui,
                tab,
                thumb_textures,
                settings.sheet_display_unit,
                theme,
                actions,
            );
        }
        LeftPanelTab::Spaces => {
            draw_spaces_panel(ui, tab, theme, actions);
        }
        LeftPanelTab::Layers => {
            draw_layers_panel(ui, tab, new_layer_name, theme);
        }
        LeftPanelTab::Bookmarks => {
            draw_bookmarks_panel(ui, tab, theme, actions);
        }
    }
}

fn draw_thumbnails_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    thumb_textures: &HashMap<(Uuid, usize), TextureHandle>,
    sheet_unit: SheetDisplayUnit,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    let active_idx = tab.active_page_index();
    let doc_id = tab.id;
    let total = tab.pages.len();

    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Páginas ({total})")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ Pág.").on_hover_text("Insertar página en blanco").clicked() {
                actions.push(UiAction::InsertBlankPage(active_idx + 1));
            }
            if ui
                .small_button("↻ 90°")
                .on_hover_text("Rotar página actual +90°")
                .clicked()
            {
                actions.push(UiAction::RotatePage(active_idx, 90));
            }
        });
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("thumbs_scroll")
        .show(ui, |ui| {
            for (idx, p) in tab.pages.iter().enumerate() {
                let selected = idx == active_idx;
                let frame = egui::Frame::new()
                    .fill(if selected {
                        theme.accent_red_soft
                    } else {
                        theme.card_bg
                    })
                    .stroke(Stroke::new(
                        if selected { 1.6 } else { 1.0 },
                        if selected {
                            theme.accent_red
                        } else {
                            theme.border
                        },
                    ))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::same(6));

                let resp = frame
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            if let Some(tex) = thumb_textures.get(&(doc_id, idx)) {
                                let sz = tex.size_vec2();
                                let max_w = (ui.available_width() - 8.0).clamp(80.0, 175.0);
                                let aspect = (sz.y / sz.x.max(1.0)).clamp(0.3, 2.0);
                                let draw_sz = egui::vec2(max_w, max_w * aspect);
                                ui.image(egui::load::SizedTexture::new(tex.id(), draw_sz));
                            } else {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(140.0, 96.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(r, CornerRadius::same(3), theme.control_bg);
                                ui.painter().text(
                                    r.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Renderizando...",
                                    egui::FontId::proportional(11.0),
                                    theme.text_muted,
                                );
                            }
                            ui.add_space(4.0);
                            let dims = format_page_dimensions(p.width_pt, p.height_pt, sheet_unit);
                            ui.label(
                                RichText::new(p.display_label())
                                    .size(11.2)
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(dims)
                                    .size(10.0)
                                    .color(theme.text_muted),
                            );
                        });
                    })
                    .response;

                let click_resp = resp.interact(egui::Sense::click());
                if click_resp.clicked() {
                    actions.push(UiAction::GoToPage(idx));
                }
                let total_pages = tab.pages.len();
                click_resp.context_menu(|ui| {
                    if ui.button("Ir a esta lámina").clicked() {
                        actions.push(UiAction::GoToPage(idx));
                        ui.close();
                    }
                    if ui.button("Renombrar etiqueta de lámina...").clicked() {
                        actions.push(UiAction::OpenDialog(DialogRequest::EditPageLabel {
                            page_index: idx,
                        }));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rotar +90° (Horario)").clicked() {
                        actions.push(UiAction::RotatePage(idx, 90));
                        ui.close();
                    }
                    if ui.button("Rotar -90° (Antihorario)").clicked() {
                        actions.push(UiAction::RotatePage(idx, -90));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Insertar página en blanco después").clicked() {
                        actions.push(UiAction::InsertBlankPage(idx + 1));
                        ui.close();
                    }
                    if ui
                        .add_enabled(total_pages > 1, egui::Button::new("Eliminar esta lámina"))
                        .clicked()
                    {
                        actions.push(UiAction::DeletePage(idx));
                        ui.close();
                    }
                });
                ui.add_space(6.0);
            }
        });
}

fn draw_layers_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    new_layer_name: &mut String,
    theme: &RedBimTheme,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    ui.label(RichText::new("Capas de marcas").strong());
    ui.horizontal(|ui| {
        let te = ui.add(
            egui::TextEdit::singleline(new_layer_name)
                .hint_text("Nueva capa...")
                .desired_width(ui.available_width() - 56.0),
        );
        let enter_create =
            te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Crear").clicked() || enter_create) && !new_layer_name.trim().is_empty() {
            let name = new_layer_name.trim().to_string();
            if !tab.layers.iter().any(|l| l.name == name) {
                tab.push_undo();
                tab.layers.push(LayerItem::new(name));
            }
            new_layer_name.clear();
        }
    });
    ui.separator();

    let mut toggle_vis_idx: Option<usize> = None;
    let mut toggle_lock_idx: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("layers_scroll")
        .show(ui, |ui| {
            for (li, layer) in tab.layers.iter().enumerate() {
                let count = tab.markups.iter().filter(|m| m.layer == layer.name).count();
                ui.horizontal(|ui| {
                    let mut vis = layer.visible;
                    if ui.checkbox(&mut vis, "").changed() {
                        toggle_vis_idx = Some(li);
                    }
                    ui.label(RichText::new(&layer.name).strong().size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{count}"))
                                .size(11.0)
                                .color(theme.text_muted),
                        );
                        let lock_txt = if layer.locked { "🔒 Bloq." } else { "Libre" };
                        if ui.small_button(lock_txt).clicked() {
                            toggle_lock_idx = Some(li);
                        }
                    });
                });
            }
        });
    if let Some(li) = toggle_vis_idx {
        tab.push_undo();
        if let Some(l) = tab.layers.get_mut(li) {
            l.visible = !l.visible;
        }
    }
    if let Some(li) = toggle_lock_idx {
        tab.push_undo();
        if let Some(l) = tab.layers.get_mut(li) {
            l.locked = !l.locked;
        }
    }
}

fn draw_bookmarks_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    ui.label(RichText::new("Marcadores del plano").strong());
    ui.separator();
    if tab.bookmarks.is_empty() {
        ui.label(
            RichText::new("Este PDF no tiene marcadores/índices incorporados.")
                .size(11.5)
                .color(theme.text_muted),
        );
        return;
    }

    let nodes = tab.bookmarks.clone();
    egui::ScrollArea::vertical()
        .id_salt("bookmarks_scroll")
        .show(ui, |ui| {
            for n in &nodes {
                draw_bookmark_tree(ui, n, theme, actions);
            }
        });
}

fn draw_bookmark_tree(
    ui: &mut egui::Ui,
    node: &BookmarkNode,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    if node.children.is_empty() {
        let lbl = if let Some(p) = node.page_index {
            format!("{}  (p. {})", node.title, p + 1)
        } else {
            node.title.clone()
        };
        if ui
            .selectable_label(false, RichText::new(lbl).size(12.0))
            .clicked()
            && let Some(p) = node.page_index
        {
            actions.push(UiAction::GoToPage(p));
        }
    } else {
        let header = egui::CollapsingHeader::new(
            RichText::new(&node.title)
                .size(12.0)
                .color(theme.text_primary),
        )
        .default_open(true)
        .show(ui, |ui| {
            for child in &node.children {
                draw_bookmark_tree(ui, child, theme, actions);
            }
        });
        if header.header_response.clicked()
            && let Some(p) = node.page_index
        {
            actions.push(UiAction::GoToPage(p));
        }
    }
}

fn draw_spaces_panel(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let Some(tab) = tab else {
        ui.label(RichText::new("Sin documento abierto").color(theme.text_muted));
        return;
    };

    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut tab.highlight_spaces, "Resaltar espacios")
            .on_hover_text("Mostrar polígonos de recintos/espacios sobre el plano")
            .changed()
        {
            tab.dirty = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(RichText::new("+ Dibujar").strong().color(theme.accent_red))
                .on_hover_text("Dibujar un nuevo Espacio/Recinto poligonal en la lámina actual")
                .clicked()
            {
                actions.push(UiAction::SetTool(ActiveTool::Space));
            }
        });
    });

    ui.label(
        RichText::new(
            "Las marcas dentro de un espacio heredan automáticamente su nombre en la Lista de Marcas y CSV.",
        )
        .size(10.5)
        .color(theme.text_muted),
    );
    ui.separator();

    if tab.spaces.is_empty() {
        ui.add_space(10.0);
        ui.label(
            RichText::new("No hay espacios definidos aún.")
                .size(12.0)
                .color(theme.text_muted),
        );
        return;
    }

    // Snapshot info to avoid borrow conflicts while mutating titles/parents
    let space_rows: Vec<(Uuid, usize, String, String, [u8; 4], usize)> = tab
        .spaces
        .iter()
        .map(|s| {
            let full = tab.space_full_title(s.id);
            let page_lbl = tab.page_display_label(s.page_index);
            let count = tab
                .markups
                .iter()
                .filter(|m| {
                    m.page_index == s.page_index
                        && tab
                            .resolve_space_for_markup(m)
                            .is_some_and(|resolved| resolved == full)
                })
                .count();
            (s.id, s.page_index, full, page_lbl, s.fill_color, count)
        })
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("spaces_scroll")
        .show(ui, |ui| {
            for (sid, page_idx, full_title, page_lbl, color, count) in space_rows {
                let is_cur_page = page_idx == tab.active_page_index();
                egui::Frame::new()
                    .fill(if is_cur_page {
                        theme.card_bg
                    } else {
                        theme.panel_bg
                    })
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (sw, _) = ui.allocate_exact_size(
                                egui::vec2(12.0, 12.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                sw,
                                CornerRadius::same(3),
                                Color32::from_rgba_unmultiplied(
                                    color[0], color[1], color[2], 220,
                                ),
                            );
                            if let Some(sp) = tab.spaces.iter_mut().find(|s| s.id == sid) {
                                let te = ui.add(
                                    egui::TextEdit::singleline(&mut sp.title)
                                        .desired_width((ui.available_width() - 32.0).max(80.0)),
                                );
                                if te.changed() {
                                    tab.dirty = true;
                                }
                            }
                            if ui
                                .small_button("✕")
                                .on_hover_text("Eliminar espacio")
                                .clicked()
                            {
                                actions.push(UiAction::DeleteSpace(sid));
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    false,
                                    RichText::new(format!("{page_lbl}  ·  {count} marcas"))
                                        .size(10.5)
                                        .color(theme.text_secondary),
                                )
                                .on_hover_text(format!("Ir a {full_title}"))
                                .clicked()
                            {
                                actions.push(UiAction::GoToPage(page_idx));
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        });
}
