use crate::model::document::DocumentTab;
use crate::model::markup::{
    CountStyle, HatchPattern, LineEnd, LineStyle, MarkupStyle,
    MeasurementOptions, NoteIcon,
};
use crate::model::settings::CsvDelimiter;
use crate::model::tool_chest::ActiveTool;
use crate::model::units::LengthUnit;
use crate::ui::actions::UiAction;
use crate::ui::icons::{IconKind, icon_button};
use crate::ui::menu_bar::PanelVisibility;
use crate::ui::theme::RedBimTheme;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

const QUICK_COLORS: &[[u8; 4]] = &[
    [229, 57, 53, 255],   // RedBIM Red
    [37, 99, 235, 255],   // Engineering Blue
    [22, 163, 74, 255],   // Measurement Green
    [245, 158, 11, 255],  // Contractor Amber
    [6, 182, 212, 255],   // Cyan
    [147, 51, 234, 255],  // Purple
    [250, 204, 21, 255],  // Highlight Yellow
    [24, 24, 27, 255],    // Dark Charcoal
    [255, 255, 255, 255], // White
];

fn toolbar_group(
    ui: &mut egui::Ui,
    theme: &RedBimTheme,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(theme.card_bg)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                add_contents(ui);
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn tool_dropdown_group(
    ui: &mut egui::Ui,
    id: &str,
    tooltip: &str,
    tools: &[ActiveTool],
    active_tool: ActiveTool,
    has_doc: bool,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let any_active = tools.contains(&active_tool);
    let label_txt = if any_active {
        RichText::new("▾").size(12.0).strong().color(theme.accent_red)
    } else {
        RichText::new("▾").size(12.0).color(theme.text_secondary)
    };
    let btn = egui::Button::new(label_txt)
        .fill(if any_active {
            theme.accent_red.gamma_multiply(0.18)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if any_active {
            Stroke::new(1.0, theme.accent_red)
        } else {
            Stroke::NONE
        })
        .corner_radius(CornerRadius::same(4))
        .min_size(egui::vec2(18.0, 26.0));

    let resp = ui.add_enabled(has_doc, btn).on_hover_text(tooltip);
    let popup_id = ui.make_persistent_id(id);
    egui::Popup::from_toggle_button_response(&resp)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            ui.set_min_width(175.0);
            for &tool in tools {
                let is_cur = active_tool == tool;
                if ui
                    .selectable_label(is_cur, tool.label_es())
                    .clicked()
                {
                    actions.push(UiAction::SetTool(tool));
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
pub fn draw_main_toolbar(
    ui: &mut egui::Ui,
    tab: Option<&mut DocumentTab>,
    active_tool: ActiveTool,
    default_style: &mut MarkupStyle,
    default_meas_opts: &mut MeasurementOptions,
    panels: &mut PanelVisibility,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let has_doc = tab.is_some();
    let can_undo = tab.as_ref().is_some_and(|t| !t.undo_stack.is_empty());
    let can_redo = tab.as_ref().is_some_and(|t| !t.redo_stack.is_empty());

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 3.0);

        // 1. Archivo + Deshacer/Rehacer (compacto)
        toolbar_group(ui, theme, |ui| {
            if icon_button(ui, 26.0, IconKind::OpenFile, false, true, "Abrir PDF (Ctrl+O)", theme).clicked() {
                actions.push(UiAction::OpenPdfDialog);
            }
            if icon_button(ui, 26.0, IconKind::SaveFile, false, has_doc, "Guardar PDF (Ctrl+S)", theme).clicked() {
                actions.push(UiAction::SaveDocument);
            }
            if icon_button(ui, 26.0, IconKind::Undo, false, can_undo, "Deshacer (Ctrl+Z)", theme).clicked() {
                actions.push(UiAction::Undo);
            }
            if icon_button(ui, 26.0, IconKind::Redo, false, can_redo, "Rehacer (Ctrl+Y)", theme).clicked() {
                actions.push(UiAction::Redo);
            }
            let file_more_resp = ui
                .add(
                    egui::Button::new(RichText::new("▾").size(12.0).color(theme.text_secondary))
                        .frame(false)
                        .min_size(egui::vec2(18.0, 26.0)),
                )
                .on_hover_text("Más opciones de archivo y exportación");
            let file_popup_id = ui.make_persistent_id("tb_file_more");
            egui::Popup::from_toggle_button_response(&file_more_resp)
                .id(file_popup_id)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                .show(|ui| {
                    ui.set_min_width(175.0);
                    if ui.button("Nuevo plano en blanco").clicked() {
                        actions.push(UiAction::NewBlankDocument);
                    }
                    if ui
                        .add_enabled(has_doc, egui::Button::new("Exportar PDF aplanado"))
                        .clicked()
                    {
                        actions.push(UiAction::ExportFlattenedPdf);
                    }
                    if ui
                        .add_enabled(has_doc, egui::Button::new("Exportar CSV (Chile ;)"))
                        .clicked()
                    {
                        actions.push(UiAction::ExportCsv(CsvDelimiter::Semicolon));
                    }
                });
        });

        // 2. Navegación
        toolbar_group(ui, theme, |ui| {
            for tool in [ActiveTool::Select, ActiveTool::Pan, ActiveTool::ZoomWindow] {
                if icon_button(
                    ui,
                    26.0,
                    IconKind::from_tool(tool),
                    active_tool == tool,
                    has_doc,
                    tool.label_es(),
                    theme,
                )
                .clicked()
                {
                    actions.push(UiAction::SetTool(tool));
                }
            }
        });

        // 3. Comentarios y texto (esenciales + desplegable)
        toolbar_group(ui, theme, |ui| {
            for tool in [
                ActiveTool::Note,
                ActiveTool::TextBox,
                ActiveTool::Callout,
                ActiveTool::Cloud,
            ] {
                if icon_button(
                    ui,
                    26.0,
                    IconKind::from_tool(tool),
                    active_tool == tool,
                    has_doc,
                    tool.label_es(),
                    theme,
                )
                .clicked()
                {
                    actions.push(UiAction::SetTool(tool));
                }
            }
            tool_dropdown_group(
                ui,
                "tb_more_text",
                "Más herramientas de texto y revisión",
                &[ActiveTool::Typewriter, ActiveTool::CloudPlus],
                active_tool,
                has_doc,
                theme,
                actions,
            );
        });

        // 4. Dibujo y formas (esenciales + desplegable)
        toolbar_group(ui, theme, |ui| {
            for tool in [
                ActiveTool::Arrow,
                ActiveTool::Line,
                ActiveTool::Rectangle,
                ActiveTool::Highlight,
            ] {
                if icon_button(
                    ui,
                    26.0,
                    IconKind::from_tool(tool),
                    active_tool == tool,
                    has_doc,
                    tool.label_es(),
                    theme,
                )
                .clicked()
                {
                    actions.push(UiAction::SetTool(tool));
                }
            }
            tool_dropdown_group(
                ui,
                "tb_more_shapes",
                "Más formas y marcas (Polilínea, Arco, Elipse, Polígono, Tinta, Sello, Imagen...)",
                &[
                    ActiveTool::Polyline,
                    ActiveTool::Arc,
                    ActiveTool::Ellipse,
                    ActiveTool::Polygon,
                    ActiveTool::Ink,
                    ActiveTool::Stamp,
                    ActiveTool::Image,
                    ActiveTool::Legend,
                    ActiveTool::Snapshot,
                    ActiveTool::Hyperlink,
                    ActiveTool::Redaction,
                ],
                active_tool,
                has_doc,
                theme,
                actions,
            );
        });

        // 5. Medición y escala (esenciales + desplegable)
        toolbar_group(ui, theme, |ui| {
            if icon_button(
                ui,
                26.0,
                IconKind::Calibrate,
                active_tool == ActiveTool::Calibrate,
                has_doc,
                "Calibrar escala con 2 puntos",
                theme,
            )
            .clicked()
            {
                actions.push(UiAction::StartTwoPointCalibration);
            }
            for tool in [
                ActiveTool::MeasureLength,
                ActiveTool::MeasureArea,
                ActiveTool::MeasurePerimeter,
                ActiveTool::MeasureCount,
            ] {
                if icon_button(
                    ui,
                    26.0,
                    IconKind::from_tool(tool),
                    active_tool == tool,
                    has_doc,
                    tool.label_es(),
                    theme,
                )
                .clicked()
                {
                    actions.push(UiAction::SetTool(tool));
                }
            }
            tool_dropdown_group(
                ui,
                "tb_more_measure",
                "Más mediciones (Polilongitud, Volumen, Ángulo, Radio, Diámetro, Recorte, Viewport, Espacio, Búsqueda visual)",
                &[
                    ActiveTool::MeasurePolylength,
                    ActiveTool::MeasureVolume,
                    ActiveTool::MeasureAngle,
                    ActiveTool::MeasureRadius,
                    ActiveTool::MeasureDiameter,
                    ActiveTool::CutoutPoly,
                    ActiveTool::ViewportRegion,
                    ActiveTool::Space,
                    ActiveTool::VisualSearch,
                ],
                active_tool,
                has_doc,
                theme,
                actions,
            );
        });

        toolbar_group(ui, theme, |ui| {
            let style = ui
                .add_enabled(has_doc, egui::Button::new("Estilo ▾").min_size(egui::vec2(0.0, 26.0)))
                .on_hover_text("Color, relleno y estilo de la herramienta o marca seleccionada");
            if let Some(tab) = tab {
                egui::Popup::from_toggle_button_response(&style)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .width(330.0)
                    .gap(6.0)
                    .show(|ui| {
                        draw_style_controls(
                            ui, tab, active_tool, default_style, default_meas_opts, theme, actions,
                        );
                    });
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Marcas")
                        .selected(panels.markups_open)
                        .min_size(egui::vec2(0.0, 26.0)),
                )
                .on_hover_text("Lista de marcas (F8)")
                .clicked()
            {
                panels.markups_open = !panels.markups_open;
            }
        });
    });
}

fn draw_style_controls(
    ui: &mut egui::Ui,
    tab: &mut DocumentTab,
    active_tool: ActiveTool,
    default_style: &mut MarkupStyle,
    default_meas_opts: &mut MeasurementOptions,
    theme: &RedBimTheme,
    actions: &mut Vec<UiAction>,
) {
    let selected_ids = tab.selected_ids.clone();
    let has_selection = !selected_ids.is_empty();

    // Determine working style & measurement options (from first selected markup, or active tool defaults)
    let mut style = if let Some(first_id) = selected_ids.first()
        && let Some(m) = tab.markups.iter().find(|x| x.id == *first_id)
    {
        m.style.clone()
    } else {
        default_style.clone()
    };

    let first_kind = selected_ids
        .first()
        .and_then(|id| tab.markups.iter().find(|m| m.id == *id))
        .map(|m| ActiveTool::from_markup_kind(&m.kind))
        .unwrap_or(active_tool);

    let orig_style = style.clone();
    let mut style_dirty = false;
    let mut meas_dirty = false;
    let mut meas_opts = selected_ids
        .first()
        .and_then(|id| tab.markups.iter().find(|m| m.id == *id))
        .and_then(|m| m.measurement_opts().cloned())
        .unwrap_or_else(|| default_meas_opts.clone());
    let orig_meas_opts = meas_opts.clone();

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;

        let badge = if has_selection {
            if selected_ids.len() == 1 {
                tab.markups
                    .iter()
                    .find(|m| m.id == selected_ids[0])
                    .map(|m| m.type_name_es().to_string())
                    .unwrap_or_else(|| "Selección".to_string())
            } else {
                format!("{} marcas", selected_ids.len())
            }
        } else {
            active_tool.default_subject().to_string()
        };

        egui::Frame::new()
            .fill(theme.card_bg)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 2))
            .show(ui, |ui| {
                ui.label(RichText::new(badge).size(11.5).strong().color(theme.accent_cyan));
            });

        // Stroke color picker + quick swatches
        ui.label(RichText::new("Trazo:").size(11.5).color(theme.text_secondary));
        let mut s_rgb = [style.stroke_color[0], style.stroke_color[1], style.stroke_color[2]];
        if ui.color_edit_button_srgb(&mut s_rgb).changed() {
            style.stroke_color = [s_rgb[0], s_rgb[1], s_rgb[2], 255];
            style.text_color = style.stroke_color;
            style_dirty = true;
        }
        for &c in &QUICK_COLORS[..6] {
            let col = Color32::from_rgb(c[0], c[1], c[2]);
            let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
            let is_cur = style.stroke_color[0..3] == c[0..3];
            ui.painter().rect(
                r,
                CornerRadius::same(3),
                col,
                Stroke::new(if is_cur { 1.8 } else { 0.8 }, if is_cur { Color32::WHITE } else { theme.border }),
                egui::StrokeKind::Inside,
            );
            if resp.clicked() {
                style.stroke_color = c;
                style.text_color = c;
                style_dirty = true;
            }
        }

        // Fill color toggle + picker
        let mut has_fill = style.fill_color.is_some();
        if ui.checkbox(&mut has_fill, "Relleno").changed() {
            style.fill_color = if has_fill {
                Some(style.stroke_color)
            } else {
                None
            };
            style_dirty = true;
        }
        if let Some(fc) = &mut style.fill_color {
            let mut f_rgb = [fc[0], fc[1], fc[2]];
            if ui.color_edit_button_srgb(&mut f_rgb).changed() {
                *fc = [f_rgb[0], f_rgb[1], f_rgb[2], 255];
                style_dirty = true;
            }
        }

        ui.separator();

        // Line width
        ui.label(RichText::new("Grosor:").size(11.5).color(theme.text_secondary));
        if ui
            .add(
                egui::DragValue::new(&mut style.line_width)
                    .range(0.5..=20.0)
                    .speed(0.1)
                    .suffix(" pt"),
            )
            .changed()
        {
            style_dirty = true;
        }

        // Opacity
        ui.label(RichText::new("Opac.:").size(11.5).color(theme.text_secondary));
        let mut op_pct = (style.opacity * 100.0).round();
        if ui
            .add(
                egui::DragValue::new(&mut op_pct)
                    .range(10.0..=100.0)
                    .speed(1.0)
                    .suffix("%"),
            )
            .changed()
        {
            style.opacity = (op_pct / 100.0).clamp(0.1, 1.0);
            style_dirty = true;
        }

        // Line style
        egui::ComboBox::from_id_salt("dpt_line_style")
            .selected_text(style.line_style.label())
            .width(88.0)
            .show_ui(ui, |ui| {
                for &ls in LineStyle::ALL {
                    if ui
                        .selectable_value(&mut style.line_style, ls, ls.label())
                        .changed()
                    {
                        style_dirty = true;
                    }
                }
            });

        // Arrowhead ends for Line/Arrow/Callout/MeasureLength
        if matches!(
            first_kind,
            ActiveTool::Line
                | ActiveTool::Arrow
                | ActiveTool::Polyline
                | ActiveTool::Callout
                | ActiveTool::CloudPlus
                | ActiveTool::MeasureLength
        ) {
            egui::ComboBox::from_id_salt("dpt_end_arrow")
                .selected_text(format!("Punta: {}", style.end_end.label()))
                .width(120.0)
                .show_ui(ui, |ui| {
                    for &le in LineEnd::ALL {
                        if ui
                            .selectable_value(&mut style.end_end, le, le.label())
                            .changed()
                        {
                            if first_kind == ActiveTool::MeasureLength {
                                style.start_end = le;
                            }
                            style_dirty = true;
                        }
                    }
                });
        }

        // Hatch for area / polygon / volume / rect
        if matches!(
            first_kind,
            ActiveTool::Rectangle
                | ActiveTool::Ellipse
                | ActiveTool::Polygon
                | ActiveTool::MeasureArea
                | ActiveTool::MeasureVolume
                | ActiveTool::SketchRect
                | ActiveTool::SketchPoly
        ) {
            egui::ComboBox::from_id_salt("dpt_hatch")
                .selected_text(style.hatch.label())
                .width(105.0)
                .show_ui(ui, |ui| {
                    for &h in HatchPattern::ALL {
                        if ui.selectable_value(&mut style.hatch, h, h.label()).changed() {
                            style_dirty = true;
                        }
                    }
                });
        }

        // Font size for text/callout/measurement captions
        if matches!(
            first_kind,
            ActiveTool::TextBox
                | ActiveTool::Typewriter
                | ActiveTool::Callout
                | ActiveTool::CloudPlus
                | ActiveTool::Stamp
        ) || first_kind.is_measurement()
        {
            ui.label(RichText::new("Texto:").size(11.5).color(theme.text_secondary));
            if ui
                .add(
                    egui::DragValue::new(&mut style.font_size)
                        .range(6.0..=48.0)
                        .speed(0.5)
                        .suffix(" pt"),
                )
                .changed()
            {
                style_dirty = true;
            }
        }

        // Note icon selector
        if first_kind == ActiveTool::Note {
            egui::ComboBox::from_id_salt("dpt_note_icon")
                .selected_text(style.note_icon.label())
                .width(105.0)
                .show_ui(ui, |ui| {
                    for &ni in NoteIcon::ALL {
                        if ui
                            .selectable_value(&mut style.note_icon, ni, ni.label())
                            .changed()
                        {
                            style_dirty = true;
                        }
                    }
                });
        }

        // Count symbol selector
        if first_kind == ActiveTool::MeasureCount {
            egui::ComboBox::from_id_salt("dpt_count_style")
                .selected_text(style.count_style.label())
                .width(105.0)
                .show_ui(ui, |ui| {
                    for &cs in CountStyle::ALL {
                        if ui
                            .selectable_value(&mut style.count_style, cs, cs.label())
                            .changed()
                        {
                            style_dirty = true;
                        }
                    }
                });
        }

        // Measurement controls
        if first_kind.is_measurement() {
            ui.separator();
            if ui
                .checkbox(&mut meas_opts.show_caption, "Etiqueta")
                .changed()
            {
                meas_dirty = true;
            }
            if ui
                .checkbox(&mut meas_opts.show_segments, "Segmentos")
                .changed()
            {
                meas_dirty = true;
            }
            let cur_unit_label = meas_opts
                .length_unit_override
                .map(|u| u.suffix())
                .unwrap_or("Página");
            egui::ComboBox::from_id_salt("dpt_meas_unit")
                .selected_text(format!("Und: {cur_unit_label}"))
                .width(90.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut meas_opts.length_unit_override, None, "Según página")
                        .changed()
                    {
                        meas_dirty = true;
                    }
                    for &lu in LengthUnit::ALL {
                        if ui
                            .selectable_value(
                                &mut meas_opts.length_unit_override,
                                Some(lu),
                                lu.display_name(),
                            )
                            .changed()
                        {
                            meas_dirty = true;
                        }
                    }
                });
            if first_kind == ActiveTool::MeasureVolume {
                ui.label(RichText::new("Prof.:").size(11.5).color(theme.text_secondary));
                if ui
                    .add(
                        egui::DragValue::new(&mut meas_opts.depth_meters)
                            .range(0.01..=500.0)
                            .speed(0.05)
                            .suffix(" m"),
                    )
                    .changed()
                {
                    meas_dirty = true;
                }
            }
        }

        if has_selection {
            ui.separator();
            if ui
                .button("+ Caja de herramientas")
                .on_hover_text("Guardar esta marca en Mis herramientas")
                .clicked()
            {
                actions.push(UiAction::AddSelectedToToolChest);
            }
            if ui
                .button("Usar como pred.")
                .on_hover_text("Usar este estilo para futuras marcas")
                .clicked()
            {
                actions.push(UiAction::SetCurrentStyleAsDefault);
            }
        }
    });

    if style_dirty {
        if has_selection {
            let locked_layers: std::collections::HashSet<String> = tab
                .layers
                .iter()
                .filter(|l| l.locked)
                .map(|l| l.name.clone())
                .collect();
            let any_unlocked = tab
                .markups
                .iter()
                .any(|m| selected_ids.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer));
            if any_unlocked {
                tab.push_undo();
                for m in &mut tab.markups {
                    if selected_ids.contains(&m.id)
                        && !m.locked
                        && !locked_layers.contains(&m.layer)
                    {
                        if style.stroke_color != orig_style.stroke_color {
                            m.style.stroke_color = style.stroke_color;
                        }
                        if style.text_color != orig_style.text_color {
                            m.style.text_color = style.text_color;
                        }
                        if style.fill_color != orig_style.fill_color {
                            m.style.fill_color = style.fill_color;
                        }
                        if (style.opacity - orig_style.opacity).abs() > 1e-4 {
                            m.style.opacity = style.opacity;
                        }
                        if (style.fill_opacity - orig_style.fill_opacity).abs() > 1e-4 {
                            m.style.fill_opacity = style.fill_opacity;
                        }
                        if (style.line_width - orig_style.line_width).abs() > 1e-4 {
                            m.style.line_width = style.line_width;
                        }
                        if style.line_style != orig_style.line_style {
                            m.style.line_style = style.line_style;
                        }
                        if (style.cloud_arc_size - orig_style.cloud_arc_size).abs() > 1e-4 {
                            m.style.cloud_arc_size = style.cloud_arc_size;
                        }
                        if style.hatch != orig_style.hatch {
                            m.style.hatch = style.hatch;
                        }
                        if style.start_end != orig_style.start_end {
                            m.style.start_end = style.start_end;
                        }
                        if style.end_end != orig_style.end_end {
                            m.style.end_end = style.end_end;
                        }
                        if (style.font_size - orig_style.font_size).abs() > 1e-4 {
                            m.style.font_size = style.font_size;
                        }
                        if style.bold != orig_style.bold {
                            m.style.bold = style.bold;
                        }
                        if style.italic != orig_style.italic {
                            m.style.italic = style.italic;
                        }
                        if style.align != orig_style.align {
                            m.style.align = style.align;
                        }
                        if style.box_shape != orig_style.box_shape {
                            m.style.box_shape = style.box_shape;
                        }
                        if style.note_icon != orig_style.note_icon {
                            m.style.note_icon = style.note_icon;
                        }
                        if style.count_style != orig_style.count_style {
                            m.style.count_style = style.count_style;
                        }
                        m.modified_at = chrono::Utc::now();
                    }
                }
                actions.push(UiAction::MarkupsStyleChanged);
            }
        } else {
            *default_style = style;
        }
    }

    if meas_dirty {
        if has_selection {
            let locked_layers: std::collections::HashSet<String> = tab
                .layers
                .iter()
                .filter(|l| l.locked)
                .map(|l| l.name.clone())
                .collect();
            let any_unlocked = tab.markups.iter().any(|m| {
                selected_ids.contains(&m.id)
                    && !m.locked
                    && !locked_layers.contains(&m.layer)
                    && m.measurement_opts().is_some()
            });
            if any_unlocked {
                tab.push_undo();
                for m in &mut tab.markups {
                    if selected_ids.contains(&m.id)
                        && !m.locked
                        && !locked_layers.contains(&m.layer)
                        && let Some(o) = m.measurement_opts_mut()
                    {
                        if o.show_caption != meas_opts.show_caption
                            && meas_opts.show_caption != orig_meas_opts.show_caption
                        {
                            o.show_caption = meas_opts.show_caption;
                        }
                        if meas_opts.show_segments != orig_meas_opts.show_segments {
                            o.show_segments = meas_opts.show_segments;
                        }
                        if meas_opts.show_centroid != orig_meas_opts.show_centroid {
                            o.show_centroid = meas_opts.show_centroid;
                        }
                        if (meas_opts.depth_meters - orig_meas_opts.depth_meters).abs() > 1e-6 {
                            o.depth_meters = meas_opts.depth_meters;
                        }
                        if (meas_opts.rise_drop_meters - orig_meas_opts.rise_drop_meters).abs()
                            > 1e-6
                        {
                            o.rise_drop_meters = meas_opts.rise_drop_meters;
                        }
                        if meas_opts.length_unit_override != orig_meas_opts.length_unit_override {
                            o.length_unit_override = meas_opts.length_unit_override;
                        }
                        if meas_opts.area_unit_override != orig_meas_opts.area_unit_override {
                            o.area_unit_override = meas_opts.area_unit_override;
                        }
                        if meas_opts.volume_unit_override != orig_meas_opts.volume_unit_override {
                            o.volume_unit_override = meas_opts.volume_unit_override;
                        }
                        if meas_opts.slope_format_override != orig_meas_opts.slope_format_override {
                            o.slope_format_override = meas_opts.slope_format_override;
                        }
                        if meas_opts.precision_override != orig_meas_opts.precision_override {
                            o.precision_override = meas_opts.precision_override;
                        }
                        m.modified_at = chrono::Utc::now();
                    }
                }
            }
        } else {
            *default_meas_opts = meas_opts;
        }
    }
}

pub fn apply_tool_item_to_selection(tab: &mut DocumentTab, item_style: &MarkupStyle) {
    let ids = tab.selected_ids.clone();
    if ids.is_empty() {
        return;
    }
    let locked_layers: std::collections::HashSet<String> = tab
        .layers
        .iter()
        .filter(|l| l.locked)
        .map(|l| l.name.clone())
        .collect();
    let any_unlocked = tab
        .markups
        .iter()
        .any(|m| ids.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer));
    if !any_unlocked {
        return;
    }
    tab.push_undo();
    for m in &mut tab.markups {
        if ids.contains(&m.id) && !m.locked && !locked_layers.contains(&m.layer) {
            m.style = item_style.clone();
            m.modified_at = chrono::Utc::now();
        }
    }
}
