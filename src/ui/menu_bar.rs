use crate::model::document::{AlignKind, DocumentTab};
use crate::model::scale::{IMPERIAL_PRESETS, METRIC_PRESETS, PageScale};
use crate::model::settings::{AppSettings, CsvDelimiter, ThemeMode};
use crate::model::tool_chest::ActiveTool;
use crate::ui::actions::{DialogRequest, UiAction};
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, RichText};

pub struct PanelVisibility {
    pub left_open: bool,
    pub right_open: bool,
    pub bottom_open: bool,
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
            RichText::new("RedBIM")
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
            ui.separator();
            ui.checkbox(&mut settings.show_grid, "Mostrar grilla");
            ui.checkbox(&mut settings.dimmer_enabled, "Atenuar plano (Dimmer)");
            ui.checkbox(&mut settings.snap.snap_to_content, "Snap al contenido PDF");
            ui.checkbox(&mut settings.snap.snap_to_markup, "Snap a marcas");
            ui.checkbox(&mut settings.snap.snap_to_grid, "Snap a grilla");
            ui.separator();
            ui.checkbox(&mut panels.left_open, "Panel izquierdo (Herramientas)");
            ui.checkbox(&mut panels.right_open, "Panel derecho (Propiedades / Medición)");
            ui.checkbox(&mut panels.bottom_open, "Panel inferior (Lista de marcas)");
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
                .add_enabled(has_doc, egui::Button::new("Comparar con otro PDF..."))
                .clicked()
            {
                actions.push(UiAction::CompareWithPdf);
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
            ] {
                if ui.button(tool.label_es()).clicked() {
                    actions.push(UiAction::SetTool(tool));
                    ui.close();
                }
            }
            ui.separator();
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
            if ui.button("Acerca de RedBIM").clicked() {
                actions.push(UiAction::OpenDialog(DialogRequest::About));
                ui.close();
            }
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
        });
    });
}
