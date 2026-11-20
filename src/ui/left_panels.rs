use crate::model::document::{BookmarkNode, DocumentTab, LayerItem};
use crate::model::tool_chest::ToolChest;
use crate::model::units::{SheetDisplayUnit, format_page_dimensions};
use crate::ui::actions::UiAction;
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::theme::RedBimTheme;
use crate::ui::tool_chest_panel::draw_tool_chest_panel;
use eframe::egui::{self, CornerRadius, RichText, Stroke, TextureHandle};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeftPanelTab {
    #[default]
    ToolChest,
    Thumbnails,
    Layers,
    Bookmarks,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_left_panel(
    ui: &mut egui::Ui,
    active_tab: &mut LeftPanelTab,
    tab: Option<&mut DocumentTab>,
    chest: &mut ToolChest,
    chest_search: &mut String,
    active_chest_item: Option<Uuid>,
    new_layer_name: &mut String,
    thumb_textures: &HashMap<(Uuid, usize), TextureHandle>,
    sheet_unit: SheetDisplayUnit,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        for (kind, icon, tip) in [
            (LeftPanelTab::ToolChest, IconKind::ToolChest, "Caja de herramientas"),
            (LeftPanelTab::Thumbnails, IconKind::Thumbnails, "Miniaturas de páginas"),
            (LeftPanelTab::Layers, IconKind::Layers, "Capas"),
            (LeftPanelTab::Bookmarks, IconKind::Bookmarks, "Marcadores"),
        ] {
            if icon_button(ui, 28.0, icon, *active_tab == kind, true, tip, theme).clicked() {
                *active_tab = kind;
            }
        }
    });
    ui.separator();

    match active_tab {
        LeftPanelTab::ToolChest => {
            draw_tool_chest_panel(ui, chest, chest_search, active_chest_item, theme, actions);
        }
        LeftPanelTab::Thumbnails => {
            draw_thumbnails_panel(ui, tab, thumb_textures, sheet_unit, theme, actions);
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
                                RichText::new(format!("Lámina {}  ·  {}", idx + 1, dims))
                                    .size(11.0)
                                    .strong(),
                            );
                        });
                    })
                    .response;

                if resp.interact(egui::Sense::click()).clicked() {
                    actions.push(UiAction::GoToPage(idx));
                }
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
        ui.add(
            egui::TextEdit::singleline(new_layer_name)
                .hint_text("Nueva capa...")
                .desired_width(ui.available_width() - 56.0),
        );
        if ui.button("Crear").clicked() && !new_layer_name.trim().is_empty() {
            let name = new_layer_name.trim().to_string();
            if !tab.layers.iter().any(|l| l.name == name) {
                tab.push_undo();
                tab.layers.push(LayerItem::new(name));
            }
            new_layer_name.clear();
        }
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("layers_scroll")
        .show(ui, |ui| {
            for layer in &mut tab.layers {
                let count = tab.markups.iter().filter(|m| m.layer == layer.name).count();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut layer.visible, "");
                    ui.label(RichText::new(&layer.name).strong().size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{count}"))
                                .size(11.0)
                                .color(theme.text_muted),
                        );
                        let lock_txt = if layer.locked { "Bloq." } else { "Libre" };
                        if ui.small_button(lock_txt).clicked() {
                            layer.locked = !layer.locked;
                        }
                    });
                });
            }
        });
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
        egui::CollapsingHeader::new(RichText::new(&node.title).size(12.0).color(theme.text_primary))
            .default_open(true)
            .show(ui, |ui| {
                for child in &node.children {
                    draw_bookmark_tree(ui, child, theme, actions);
                }
            });
    }
}
