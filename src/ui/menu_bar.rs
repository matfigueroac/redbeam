use crate::model::document::{AlignKind, DocumentTab};
use crate::model::scale::{IMPERIAL_PRESETS, METRIC_PRESETS, PageScale};
use crate::model::settings::{AppSettings, CsvDelimiter, ThemeMode};
use crate::model::tool_chest::ActiveTool;
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelVisibility {
    pub left_open: bool,
    pub right_open: bool,
    pub markups_open: bool,
}

pub fn draw_menu_bar(
    ui: &mut egui::Ui,
    tab: Option<&DocumentTab>,
    settings: &mut AppSettings,
    panels: &mut PanelVisibility,
    logo_tex: Option<egui::TextureId>,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let has_doc = tab.is_some();
    egui::MenuBar::new().ui(ui, |ui| {
        // Brand emblem + title on far left
        if let Some(tid) = logo_tex {
            ui.add(egui::Image::new((tid, egui::vec2(18.0, 18.0))));
        }
        ui.label(
            RichText::new("RedBeam")
                .strong()
                .size(13.5)
                .color(theme.accent_red),
        );
        ui.separator();

        ui.menu_button("Archivo", |ui| {
            if ui.button("Nuevo plano en blanco...").clicked() {
                actions.push(UiAction::NewBlankDocument);
                ui.close();
            }
            if ui.button("Abrir PDF...  (Ctrl+O)").clicked() {
                actions.push(UiAction::OpenPdfDialog);
                ui.close();
            }
            if ui.button("Abrir plano de ejemplo (Chile 1:100)").clicked() {
                actions.push(UiAction::OpenDemoChile);
                ui.close();
            }
            if !settings.recent_files.is_empty() {
                ui.menu_button("Recientes", |ui| {
                    for p in settings.recent_files.clone() {
                        let name = p
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Archivo.pdf");
                        if ui.button(name).clicked() {
                            actions.push(UiAction::OpenPdfPath(p));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Guardar  (Ctrl+S)"))
                .clicked()
            {
                actions.push(UiAction::SaveDocument);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Guardar como..."))
                .clicked()
            {
                actions.push(UiAction::SaveDocumentAs);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Exportar PDF aplanado..."))
                .clicked()
            {
                actions.push(UiAction::ExportFlattenedPdf);
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Exportar Lista de Marcas (CSV Chile ;)"),
                )
                .clicked()
            {
                actions.push(UiAction::ExportCsv(CsvDelimiter::Semicolon));
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Exportar Lista de Marcas (CSV ,)"),
                )
                .clicked()
            {
                actions.push(UiAction::ExportCsv(CsvDelimiter::Comma));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Exportar marcas (.redbim.json)..."))
                .clicked()
            {
                actions.push(UiAction::ExportMarkupsJson);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Importar marcas (.redbim.json)..."))
                .clicked()
            {
                actions.push(UiAction::ImportMarkupsJson);
                ui.close();
            }
            if ui.button("Combinar PDFs...").clicked() {
                actions.push(UiAction::CombinePdfs);
                ui.close();
            }
            if ui.button("Procesamiento por lotes (Batch local)...").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::BatchProcess));
                ui.close();
            }
            ui.separator();
            if ui.button("Salir").clicked() {
                actions.push(UiAction::ExitApp);
                ui.close();
            }
        });

        ui.menu_button("Editar", |ui| {
            let can_undo = tab.is_some_and(|t| !t.undo_stack.is_empty());
            let can_redo = tab.is_some_and(|t| !t.redo_stack.is_empty());
            let has_sel = tab.is_some_and(|t| !t.selected_ids.is_empty());
            let multi_sel = tab.is_some_and(|t| t.selected_ids.len() >= 2);

            if ui
                .add_enabled(can_undo, egui::Button::new("Deshacer  (Ctrl+Z)"))
                .clicked()
            {
                actions.push(UiAction::Undo);
                ui.close();
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Rehacer  (Ctrl+Y)"))
                .clicked()
            {
                actions.push(UiAction::Redo);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_sel, egui::Button::new("Copiar  (Ctrl+C)"))
                .clicked()
            {
                actions.push(UiAction::CopySelected);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Pegar  (Ctrl+V)"))
                .clicked()
            {
                actions.push(UiAction::PasteClipboard);
                ui.close();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Duplicar  (Ctrl+D)"))
                .clicked()
            {
                actions.push(UiAction::DuplicateSelected);
                ui.close();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Eliminar  (Supr)"))
                .clicked()
            {
                actions.push(UiAction::DeleteSelected);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(multi_sel, egui::Button::new("Agrupar  (Ctrl+G)"))
                .clicked()
            {
                actions.push(UiAction::GroupSelected);
                ui.close();
            }
            if ui
                .add_enabled(has_sel, egui::Button::new("Desagrupar"))
                .clicked()
            {
                actions.push(UiAction::UngroupSelected);
                ui.close();
            }
            ui.add_enabled_ui(multi_sel, |ui| {
                ui.menu_button("Alinear y distribuir", |ui| {
                    for (kind, label) in [
                        (AlignKind::Left, "Alinear a la izquierda"),
                        (AlignKind::CenterX, "Centrar horizontalmente"),
                        (AlignKind::Right, "Alinear a la derecha"),
                        (AlignKind::Top, "Alinear arriba"),
                        (AlignKind::CenterY, "Centrar verticalmente"),
                        (AlignKind::Bottom, "Alinear abajo"),
                        (AlignKind::DistributeH, "Distribuir horizontalmente"),
                        (AlignKind::DistributeV, "Distribuir verticalmente"),
                    ] {
                        if ui.button(label).clicked() {
                            actions.push(UiAction::AlignSelected(kind));
                            ui.close();
                        }
                    }
                });
            });
            if ui
                .add_enabled(has_doc, egui::Button::new("Seleccionar todo  (Ctrl+A)"))
                .clicked()
            {
                actions.push(UiAction::SelectAllOnPage);
                ui.close();
            }
            ui.separator();
            if ui.button("Preferencias...").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::Preferences));
                ui.close();
            }
        });

        ui.menu_button("Vista", |ui| {
            if ui
                .add_enabled(has_doc, egui::Button::new("Ajustar página  (Ctrl+0)"))
                .clicked()
            {
                actions.push(UiAction::FitPage);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Ajustar al ancho"))
                .clicked()
            {
                actions.push(UiAction::FitWidth);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Tamaño real 100%  (Ctrl+1)"))
                .clicked()
            {
                actions.push(UiAction::Zoom100);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Vista dividida vertical"))
                .clicked()
            {
                actions.push(UiAction::ToggleSplitVertical);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Vista dividida horizontal"))
                .clicked()
            {
                actions.push(UiAction::ToggleSplitHorizontal);
                ui.close();
            }
            let is_split = tab.is_some_and(|t| t.split_mode != crate::model::document::SplitViewMode::None);
            let is_synced = tab.is_some_and(|t| t.sync_split_views);
            if ui
                .add_enabled(
                    is_split,
                    egui::Button::new(if is_synced {
                        "✓ Sincronizar vistas divididas"
                    } else {
                        "Sincronizar vistas divididas"
                    }),
                )
                .clicked()
            {
                actions.push(UiAction::ToggleSyncSplitViews);
                ui.close();
            }
            ui.separator();
            let mut view_changed = false;
            view_changed |= ui.checkbox(&mut settings.show_grid, "Mostrar grilla").changed();
            view_changed |= ui
                .checkbox(&mut settings.dimmer_enabled, "Atenuar plano (Dimmer)")
                .changed();
            view_changed |= ui
                .checkbox(&mut settings.snap.snap_to_content, "Snap al contenido PDF")
                .changed();
            view_changed |= ui
                .checkbox(&mut settings.snap.snap_to_markup, "Snap a marcas")
                .changed();
            view_changed |= ui
                .checkbox(&mut settings.snap.snap_to_grid, "Snap a grilla")
                .changed();
            if view_changed {
                settings.save();
            }
            let hl_spaces = tab.is_some_and(|t| t.highlight_spaces);
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new(if hl_spaces {
                        "✓ Resaltar espacios / recintos"
                    } else {
                        "Resaltar espacios / recintos"
                    }),
                )
                .clicked()
            {
                actions.push(UiAction::ToggleHighlightSpaces);
                ui.close();
            }
            ui.separator();
            ui.checkbox(&mut panels.left_open, "Panel izquierdo — Herramientas  (F9)");
            ui.checkbox(&mut panels.right_open, "Panel derecho — Propiedades / Medición  (F10)");
            if ui
                .add_enabled(
                    has_doc,
                    egui::Checkbox::new(&mut panels.markups_open, "Lista de marcas  (F8)"),
                )
                .clicked()
            {
                ui.close();
            }
            ui.separator();
            let next_mode = if settings.theme == ThemeMode::Dark {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            };
            let theme_lbl = if settings.theme == ThemeMode::Dark {
                "Cambiar a tema Claro"
            } else {
                "Cambiar a tema Oscuro"
            };
            if ui.button(theme_lbl).clicked() {
                actions.push(UiAction::SetTheme(next_mode));
                ui.close();
            }
        });

        ui.menu_button("Documento", |ui| {
            let page_idx = tab.map(|t| t.active_page_index()).unwrap_or(0);
            let page_count = tab.map(|t| t.pages.len()).unwrap_or(0);
            if ui
                .add_enabled(has_doc, egui::Button::new("Rotar página +90° (Horario)"))
                .clicked()
            {
                actions.push(UiAction::RotatePage(page_idx, 90));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Rotar página -90° (Antihorario)"))
                .clicked()
            {
                actions.push(UiAction::RotatePage(page_idx, -90));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Etiqueta de lámina actual..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::EditPageLabel {
                    page_index: page_idx,
                }));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Encabezados y pies de página..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::HeadersAndFooters));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Quitar encabezados y pies"))
                .clicked()
            {
                actions.push(UiAction::RemoveHeadersFooters);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Insertar página en blanco"))
                .clicked()
            {
                actions.push(UiAction::InsertBlankPage(page_idx + 1));
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc && page_count > 1,
                    egui::Button::new("Eliminar página actual"),
                )
                .clicked()
            {
                actions.push(UiAction::DeletePage(page_idx));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Extraer página actual a PDF..."))
                .clicked()
            {
                actions.push(UiAction::ExtractCurrentPage);
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Reemplazar contenido de página actual..."),
                )
                .clicked()
            {
                actions.push(UiAction::ReplaceCurrentPageContent);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Aplanar marcas en documento (Recuperable)"),
                )
                .clicked()
            {
                actions.push(UiAction::FlattenInPlace { recoverable: true });
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Desaplanar marcas (Unflatten)"))
                .clicked()
            {
                actions.push(UiAction::UnflattenDocument);
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Comparar con otro PDF..."))
                .clicked()
            {
                actions.push(UiAction::CompareWithPdf);
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Superponer planos (Overlay + Alineación)..."),
                )
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::OverlayPdfs));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Marcar zona para Redacción (Censura)"),
                )
                .clicked()
            {
                actions.push(UiAction::SetTool(ActiveTool::Redaction));
                ui.close();
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Aplicar redacciones (Destructivo)..."),
                )
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::ApplyRedactionsConfirm));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Procesamiento de color (B/N, Grises, Tinte)..."),
                )
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::ColorProcess));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Reducir tamaño de archivo PDF..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::ReduceFileSize));
                ui.close();
            }
        });

        ui.menu_button("Herramientas", |ui| {
            for tool in [
                ActiveTool::Note,
                ActiveTool::TextBox,
                ActiveTool::Typewriter,
                ActiveTool::Callout,
                ActiveTool::Cloud,
                ActiveTool::CloudPlus,
                ActiveTool::Arrow,
                ActiveTool::Line,
                ActiveTool::Polyline,
                ActiveTool::Arc,
                ActiveTool::Rectangle,
                ActiveTool::Ellipse,
                ActiveTool::Polygon,
                ActiveTool::Ink,
                ActiveTool::Highlight,
                ActiveTool::Stamp,
                ActiveTool::Image,
                ActiveTool::Legend,
                ActiveTool::Snapshot,
                ActiveTool::Hyperlink,
                ActiveTool::Space,
                ActiveTool::Redaction,
            ] {
                if ui.button(tool.label_es()).clicked() {
                    actions.push(UiAction::SetTool(tool));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Crear sello personalizado...").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::CreateStamp));
                ui.close();
            }
        });

        ui.menu_button("Medición", |ui| {
            if ui
                .add_enabled(has_doc, egui::Button::new("Calibrar escala (2 puntos)..."))
                .clicked()
            {
                actions.push(UiAction::StartTwoPointCalibration);
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Configurar escala y unidades..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::SetScale));
                ui.close();
            }
            ui.menu_button("Escalas métricas rápidas (Chile)", |ui| {
                for p in METRIC_PRESETS {
                    if ui.button(p.label).clicked() {
                        actions.push(UiAction::ApplyPageScale {
                            scale: PageScale::from_preset(p),
                            apply_all: false,
                        });
                        ui.close();
                    }
                }
            });
            ui.menu_button("Escalas imperiales (EE.UU.)", |ui| {
                for p in IMPERIAL_PRESETS {
                    if ui.button(p.label).clicked() {
                        actions.push(UiAction::ApplyPageScale {
                            scale: PageScale::from_preset(p),
                            apply_all: false,
                        });
                        ui.close();
                    }
                }
            });
            ui.separator();
            for tool in [
                ActiveTool::MeasureLength,
                ActiveTool::MeasurePolylength,
                ActiveTool::MeasurePerimeter,
                ActiveTool::MeasureArea,
                ActiveTool::MeasureVolume,
                ActiveTool::MeasureAngle,
                ActiveTool::MeasureRadius,
                ActiveTool::MeasureDiameter,
                ActiveTool::MeasureCount,
                ActiveTool::CutoutPoly,
                ActiveTool::ViewportRegion,
                ActiveTool::Space,
                ActiveTool::VisualSearch,
            ] {
                if ui.button(tool.label_es()).clicked() {
                    actions.push(UiAction::SetTool(tool));
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Columnas personalizadas y fórmulas..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::ManageCustomColumns));
                ui.close();
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Dibujo a escala exacto..."))
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::SketchToScale));
                ui.close();
            }
        });

        ui.menu_button("Ayuda", |ui| {
            if ui.button("Paleta de comandos...  (Ctrl+K)").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::CommandPalette));
                ui.close();
            }
            if ui.button("Atajos de teclado").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::Shortcuts));
                ui.close();
            }
            if !crate::updates::is_packaged() && ui.button("Buscar actualizaciones…").clicked() {
                actions.push(UiAction::CheckForUpdates);
                ui.close();
            }
            if ui.button("Acerca de RedBeam").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::About));
                ui.close();
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            #[cfg(target_os = "windows")]
            let maximized = draw_window_buttons(ui, actions);

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("⌘ Paleta (Ctrl+K)")
                            .size(11.0)
                            .color(theme.text_muted),
                    )
                    .fill(theme.control_bg)
                    .corner_radius(egui::CornerRadius::same(4)),
                )
                .on_hover_text("Buscar cualquier herramienta o comando rápido (Ctrl+K)")
                .clicked()
            {
                actions.push(UiAction::OpenDialog(DialogRequest::CommandPalette));
            }

            #[cfg(target_os = "windows")]
            {
                let drag_area = ui.allocate_response(
                    egui::vec2(ui.available_width().max(0.0), 26.0),
                    egui::Sense::click_and_drag(),
                );
                if drag_area.double_clicked_by(egui::PointerButton::Primary) {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                } else if drag_area.drag_started_by(egui::PointerButton::Primary) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            }
        });
    });
}

#[cfg(target_os = "windows")]
fn draw_window_buttons(ui: &mut egui::Ui, actions: &mut Vec<UiAction>) -> bool {
    use egui::ViewportCommand;

    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    let (maximize_icon, maximize_label) = if maximized {
        ("❐", "Restaurar ventana")
    } else {
        ("□", "Maximizar ventana")
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (icon, label, command) in [
            ("×", "Cerrar RedBeam", ViewportCommand::Close),
            (maximize_icon, maximize_label, ViewportCommand::Maximized(!maximized)),
            ("−", "Minimizar ventana", ViewportCommand::Minimized(true)),
        ] {
            let button = ui.add(
                egui::Button::new(RichText::new(icon).size(16.0))
                    .frame_when_inactive(false)
                    .min_size(egui::vec2(35.0, 26.0)),
            );
            button.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
            if button.on_hover_text(label).clicked() {
                if command == ViewportCommand::Close {
                    actions.push(UiAction::ExitApp);
                } else {
                    ui.ctx().send_viewport_cmd(command);
                }
            }
        }
    });
    maximized
}

#[cfg(target_os = "windows")]
pub fn handle_window_resize(ctx: &egui::Context) {
    use egui::{CursorIcon, ViewportCommand, viewport::ResizeDirection};

    let Some((pointer, pressed)) = ctx.input(|i| {
        let viewport = i.viewport();
        if viewport.maximized.unwrap_or(false) || viewport.fullscreen.unwrap_or(false) {
            return None;
        }
        let pressed = i.pointer.primary_pressed();
        if i.pointer.primary_down() && !pressed {
            return None;
        }
        Some((i.pointer.hover_pos()?, pressed))
    }) else {
        return;
    };
    let rect = ctx.viewport_rect();
    if !rect.contains(pointer) {
        return;
    }

    let edge = 5.0;
    let left = pointer.x < rect.left() + edge;
    let right = pointer.x > rect.right() - edge;
    let top = pointer.y < rect.top() + edge;
    let bottom = pointer.y > rect.bottom() - edge;
    let (direction, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        (true, _, _, _) => (ResizeDirection::West, CursorIcon::ResizeHorizontal),
        (_, true, _, _) => (ResizeDirection::East, CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (ResizeDirection::North, CursorIcon::ResizeVertical),
        (_, _, _, true) => (ResizeDirection::South, CursorIcon::ResizeVertical),
        _ => return,
    };
    ctx.set_cursor_icon(cursor);
    if pressed {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}
