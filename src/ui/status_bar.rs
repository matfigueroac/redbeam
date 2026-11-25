use crate::model::document::{DocumentTab, SplitViewMode};
use crate::model::settings::AppSettings;
use crate::model::tool_chest::ActiveTool;
use crate::model::units::format_page_dimensions;
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

pub fn draw_status_bar(
    ui: &mut egui::Ui,
    tab: Option<&DocumentTab>,
    active_tool: ActiveTool,
    settings: &mut AppSettings,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        if let Some(tab) = tab {
            let split = tab.split_mode;
            if icon_button(
                ui,
                22.0,
                IconKind::SplitVertical,
                split == SplitViewMode::Vertical,
                true,
                "Dividir vista verticalmente",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::ToggleSplitVertical);
            }
            if icon_button(
                ui,
                22.0,
                IconKind::SplitHorizontal,
                split == SplitViewMode::Horizontal,
                true,
                "Dividir vista horizontalmente",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::ToggleSplitHorizontal);
            }
            if split != SplitViewMode::None {
                let sync_btn = ui
                    .selectable_label(
                        tab.sync_split_views,
                        RichText::new("🔗 Sinc.").size(11.0),
                    )
                    .on_hover_text("Sincronizar desplazamiento y zoom entre paneles divididos");
                if sync_btn.clicked() {
                    actions.push(UiAction::ToggleSyncSplitViews);
                }
            }
            if icon_button(
                ui,
                22.0,
                IconKind::FitPage,
                false,
                true,
                "Ajustar página (Ctrl+0)",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::FitPage);
            }
            if icon_button(
                ui,
                22.0,
                IconKind::FitWidth,
                false,
                true,
                "Ajustar al ancho",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::FitWidth);
            }

            let zoom_pct = (tab.active_camera().zoom * 100.0).round() as i32;
            if ui
                .button(RichText::new(format!("{zoom_pct}%")).size(11.5))
                .on_hover_text("Clic para volver a 100%")
                .clicked()
            {
                actions.push(UiAction::Zoom100);
            }

            ui.separator();

            if icon_button(
                ui,
                22.0,
                IconKind::Pan,
                active_tool == ActiveTool::Pan,
                true,
                "Desplazar / Mano",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::SetTool(ActiveTool::Pan));
            }
            if icon_button(
                ui,
                22.0,
                IconKind::Select,
                active_tool == ActiveTool::Select,
                true,
                "Seleccionar (V)",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::SetTool(ActiveTool::Select));
            }
            if icon_button(
                ui,
                22.0,
                IconKind::ZoomWindow,
                active_tool == ActiveTool::ZoomWindow,
                true,
                "Zoom de ventana (Z)",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::SetTool(ActiveTool::ZoomWindow));
            }

            ui.separator();

            // Page navigation (direct sheet jump + prev/next)
            let cur_page = tab.active_page_index();
            let total_pages = tab.pages.len();
            if ui
                .add_enabled(cur_page > 0, egui::Button::new("◀"))
                .on_hover_text("Página anterior")
                .clicked()
            {
                actions.push(UiAction::GoToPage(cur_page - 1));
            }
            let page_input_id = ui.make_persistent_id(("sb_page_input", tab.id));
            let mut page_buf: String = ui
                .ctx()
                .data_mut(|d| d.get_temp::<String>(page_input_id))
                .unwrap_or_else(|| (cur_page + 1).to_string());
            let mut is_focused = false;
            let resp = ui
                .add(
                    egui::TextEdit::singleline(&mut page_buf)
                        .id(page_input_id)
                        .desired_width(34.0),
                )
                .on_hover_text("Escriba número de lámina y presione Enter");
            if resp.has_focus() {
                is_focused = true;
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Ok(p) = page_buf.trim().parse::<usize>()
                    && p >= 1
                    && p <= total_pages
                {
                    actions.push(UiAction::GoToPage(p - 1));
                }
                page_buf = (cur_page + 1).to_string();
            } else if !is_focused {
                page_buf = (cur_page + 1).to_string();
            }
            ui.ctx().data_mut(|d| d.insert_temp(page_input_id, page_buf));
            ui.label(
                RichText::new(format!("/ {total_pages}"))
                    .size(11.5)
                    .color(theme.text_secondary),
            );
            if ui
                .add_enabled(cur_page + 1 < total_pages, egui::Button::new("▶"))
                .on_hover_text("Página siguiente")
                .clicked()
            {
                actions.push(UiAction::GoToPage(cur_page + 1));
            }
            if let Some(page) = tab.active_page() {
                let lbl_text = page
                    .label
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Etiqueta...");
                let lbl_color = if page.label.as_deref().is_some_and(|s| !s.trim().is_empty()) {
                    theme.text_primary
                } else {
                    theme.text_muted
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new(lbl_text).size(11.0).color(lbl_color))
                            .fill(theme.control_bg)
                            .corner_radius(CornerRadius::same(4)),
                    )
                    .on_hover_text("Etiqueta de lámina (clic para editar, ej: ARQ-01 · Planta Cubierta)")
                    .clicked()
                {
                    actions.push(UiAction::OpenDialog(DialogRequest::EditPageLabel {
                        page_index: cur_page,
                    }));
                }
            }

            // Right-aligned Chilean scale, sheet dimensions in mm, snaps & reuse
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(page) = tab.active_page() {
                    let dim_str = format_page_dimensions(
                        page.width_pt,
                        page.height_pt,
                        settings.sheet_display_unit,
                    );
                    if ui
                        .button(RichText::new(dim_str).size(11.5).color(theme.text_secondary))
                        .on_hover_text("Tamaño de lámina (clic para alternar mm / cm / in)")
                        .clicked()
                    {
                        settings.sheet_display_unit = settings.sheet_display_unit.next();
                        settings.save();
                    }

                    ui.separator();

                    let scale_set = page.scale.is_set;
                    let scale_text = page.scale.label();
                    let pill_bg = if scale_set {
                        Color32::from_rgba_unmultiplied(34, 197, 94, 34)
                    } else {
                        Color32::from_rgba_unmultiplied(245, 158, 11, 36)
                    };
                    let pill_fg = if scale_set {
                        theme.accent_green
                    } else {
                        theme.accent_amber
                    };
                    let scale_caption = if scale_set {
                        format!("Escala: {scale_text}")
                    } else {
                        "⚠ Sin escala · Calibrar".to_string()
                    };
                    let resp = egui::Frame::new()
                        .fill(pill_bg)
                        .stroke(Stroke::new(1.0, pill_fg))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(8, 2))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(scale_caption)
                                        .size(11.5)
                                        .strong()
                                        .color(pill_fg),
                                )
                                .sense(egui::Sense::click()),
                            )
                        })
                        .inner;
                    if resp
                        .on_hover_text("Clic para configurar o calibrar escala (métrico Chile / imperial)")
                        .clicked()
                    {
                        actions.push(UiAction::OpenDialog(DialogRequest::SetScale));
                    }
                }

                ui.separator();

                if icon_button(
                    ui,
                    22.0,
                    IconKind::Reuse,
                    settings.reuse_tool,
                    true,
                    "Reutilizar herramienta tras dibujar",
                    theme,
                )
                .clicked()
                {
                    settings.reuse_tool = !settings.reuse_tool;
                    settings.save();
                }

                if icon_button(
                    ui,
                    22.0,
                    IconKind::Dimmer,
                    settings.dimmer_enabled,
                    true,
                    "Atenuar plano de fondo (Dimmer)",
                    theme,
                )
                .clicked()
                {
                    settings.dimmer_enabled = !settings.dimmer_enabled;
                    settings.save();
                }

                if icon_button(
                    ui,
                    22.0,
                    IconKind::SnapGrid,
                    settings.snap.snap_to_grid,
                    true,
                    "Snap a grilla",
                    theme,
                )
                .clicked()
                {
                    settings.snap.snap_to_grid = !settings.snap.snap_to_grid;
                    settings.save();
                }

                if icon_button(
                    ui,
                    22.0,
                    IconKind::SnapMarkup,
                    settings.snap.snap_to_markup,
                    true,
                    "Snap a marcas existentes",
                    theme,
                )
                .clicked()
                {
                    settings.snap.snap_to_markup = !settings.snap.snap_to_markup;
                    settings.save();
                }

                if icon_button(
                    ui,
                    22.0,
                    IconKind::SnapContent,
                    settings.snap.snap_to_content,
                    true,
                    "Snap al contenido vectorial del PDF",
                    theme,
                )
                .clicked()
                {
                    settings.snap.snap_to_content = !settings.snap.snap_to_content;
                    settings.save();
                }
            });
        } else {
            ui.label(
                RichText::new("Arrastre uno o más PDF a esta ventana para abrirlos.")
                    .size(11.5)
                    .color(theme.text_muted),
            );
        }
    });
}
