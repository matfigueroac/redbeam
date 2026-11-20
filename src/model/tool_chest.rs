use crate::model::markup::{
    BoxShape, CountStyle, HatchPattern, LineEnd, LineStyle, Markup, MarkupKind, MarkupStyle,
    MeasurementOptions, NoteIcon,
};
use crate::model::units::{AreaUnit, LengthUnit, VolumeUnit};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ActiveTool {
    #[default]
    Select,
    Pan,
    ZoomWindow,
    // Comments & Text
    Note,
    TextBox,
    Typewriter,
    Callout,
    Cloud,
    CloudPlus,
    // Drawing & Shapes
    Line,
    Arrow,
    Polyline,
    Arc,
    Rectangle,
    Ellipse,
    Polygon,
    Ink,
    Highlight,
    Stamp,
    Image,
    Snapshot,
    Legend,
    // Measurements & Scale (Chile Metric + Imperial)
    Calibrate,
    MeasureLength,
    MeasurePolylength,
    MeasurePerimeter,
    MeasureArea,
    MeasureVolume,
    MeasureAngle,
    MeasureRadius,
    MeasureDiameter,
    MeasureCount,
    CutoutPoly,
    SketchRect,
    SketchPoly,
    ViewportRegion,
}

impl ActiveTool {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::Select => "Seleccionar (V)",
            Self::Pan => "Desplazar / Mano",
            Self::ZoomWindow => "Zoom ventana (Z)",
            Self::Note => "Nota (N)",
            Self::TextBox => "Cuadro de texto (T)",
            Self::Typewriter => "Máquina de escribir (W)",
            Self::Callout => "Llamada / Flecha con texto (Q)",
            Self::Cloud => "Nube (C)",
            Self::CloudPlus => "Nube+ con comentario (K)",
            Self::Line => "Línea (L)",
            Self::Arrow => "Flecha (A)",
            Self::Polyline => "Polilínea (Shift+N)",
            Self::Arc => "Arco (Shift+C)",
            Self::Rectangle => "Rectángulo (R)",
            Self::Ellipse => "Elipse / Círculo (E)",
            Self::Polygon => "Polígono (P)",
            Self::Ink => "Lápiz / Mano alzada",
            Self::Highlight => "Resaltador (H)",
            Self::Stamp => "Sello (S)",
            Self::Image => "Imagen (I)",
            Self::Snapshot => "Instantánea",
            Self::Legend => "Leyenda de mediciones",
            Self::Calibrate => "Calibrar escala (2 pts)",
            Self::MeasureLength => "Longitud (Shift+L)",
            Self::MeasurePolylength => "Polilongitud",
            Self::MeasurePerimeter => "Perímetro (Shift+P)",
            Self::MeasureArea => "Área (Shift+A)",
            Self::MeasureVolume => "Volumen (Shift+V)",
            Self::MeasureAngle => "Ángulo (Shift+G)",
            Self::MeasureRadius => "Radio (Shift+U)",
            Self::MeasureDiameter => "Diámetro",
            Self::MeasureCount => "Conteo (Shift+O)",
            Self::CutoutPoly => "Recorte / Descuento área",
            Self::SketchRect => "Rectángulo a escala",
            Self::SketchPoly => "Polígono a escala",
            Self::ViewportRegion => "Crear Viewport de escala",
        }
    }

    pub fn default_subject(self) -> &'static str {
        match self {
            Self::Select | Self::Pan | Self::ZoomWindow => "Selección",
            Self::Note => "Nota de revisión",
            Self::TextBox => "Cuadro de texto",
            Self::Typewriter => "Texto",
            Self::Callout => "Observación",
            Self::Cloud => "Nube de revisión",
            Self::CloudPlus => "Nube+ observación",
            Self::Line => "Línea",
            Self::Arrow => "Flecha",
            Self::Polyline => "Polilínea",
            Self::Arc => "Arco",
            Self::Rectangle => "Rectángulo",
            Self::Ellipse => "Elipse",
            Self::Polygon => "Polígono",
            Self::Ink => "Trazo",
            Self::Highlight => "Resaltado",
            Self::Stamp => "Sello",
            Self::Image => "Imagen",
            Self::Snapshot => "Captura",
            Self::Legend => "Leyenda",
            Self::Calibrate => "Calibración",
            Self::MeasureLength => "Longitud",
            Self::MeasurePolylength => "Polilongitud",
            Self::MeasurePerimeter => "Perímetro",
            Self::MeasureArea => "Área",
            Self::MeasureVolume => "Volumen",
            Self::MeasureAngle => "Ángulo",
            Self::MeasureRadius => "Radio",
            Self::MeasureDiameter => "Diámetro",
            Self::MeasureCount => "Conteo",
            Self::CutoutPoly => "Recorte",
            Self::SketchRect | Self::SketchPoly => "Dibujo a escala",
            Self::ViewportRegion => "Viewport",
        }
    }

    pub fn is_measurement(self) -> bool {
        matches!(
            self,
            Self::MeasureLength
                | Self::MeasurePolylength
                | Self::MeasurePerimeter
                | Self::MeasureArea
                | Self::MeasureVolume
                | Self::MeasureAngle
                | Self::MeasureRadius
                | Self::MeasureDiameter
                | Self::MeasureCount
                | Self::SketchRect
                | Self::SketchPoly
        )
    }

    pub fn from_markup_kind(kind: &MarkupKind) -> Self {
        match kind {
            MarkupKind::Note { .. } => Self::Note,
            MarkupKind::TextBox { .. } => Self::TextBox,
            MarkupKind::Typewriter { .. } => Self::Typewriter,
            MarkupKind::Callout { .. } => Self::Callout,
            MarkupKind::Cloud { .. } => Self::Cloud,
            MarkupKind::CloudPlus { .. } => Self::CloudPlus,
            MarkupKind::Line { .. } => Self::Line,
            MarkupKind::Arrow { .. } => Self::Arrow,
            MarkupKind::Polyline { .. } => Self::Polyline,
            MarkupKind::Arc { .. } => Self::Arc,
            MarkupKind::Rectangle { .. } => Self::Rectangle,
            MarkupKind::Ellipse { .. } => Self::Ellipse,
            MarkupKind::Polygon { .. } => Self::Polygon,
            MarkupKind::Ink { .. } => Self::Ink,
            MarkupKind::Highlight { .. } => Self::Highlight,
            MarkupKind::Stamp { .. } => Self::Stamp,
            MarkupKind::Image { .. } => Self::Image,
            MarkupKind::Legend { .. } => Self::Legend,
            MarkupKind::MeasureLength { .. } => Self::MeasureLength,
            MarkupKind::MeasurePolylength { .. } => Self::MeasurePolylength,
            MarkupKind::MeasurePerimeter { .. } => Self::MeasurePerimeter,
            MarkupKind::MeasureArea { .. } => Self::MeasureArea,
            MarkupKind::MeasureVolume { .. } => Self::MeasureVolume,
            MarkupKind::MeasureAngle { .. } => Self::MeasureAngle,
            MarkupKind::MeasureRadius { .. } => Self::MeasureRadius,
            MarkupKind::MeasureDiameter { .. } => Self::MeasureDiameter,
            MarkupKind::MeasureCount { .. } => Self::MeasureCount,
            MarkupKind::SketchRect { .. } => Self::SketchRect,
            MarkupKind::SketchPoly { .. } => Self::SketchPoly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChestItem {
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub tool: ActiveTool,
    pub style: MarkupStyle,
    pub default_text: String,
    pub stamp_subtitle: String,
    pub measurement_opts: Option<MeasurementOptions>,
    /// True = applies style/properties to selected markup; False = starts drawing new markup.
    pub properties_mode: bool,
}

impl ToolChestItem {
    pub fn from_markup(markup: &Markup) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: if markup.label.is_empty() {
                markup.subject.clone()
            } else {
                markup.label.clone()
            },
            subject: markup.subject.clone(),
            tool: ActiveTool::from_markup_kind(&markup.kind),
            style: markup.style.clone(),
            default_text: markup.text_content().unwrap_or("").to_string(),
            stamp_subtitle: match &markup.kind {
                MarkupKind::Stamp { subtitle, .. } => subtitle.clone(),
                _ => String::new(),
            },
            measurement_opts: markup.measurement_opts().cloned(),
            properties_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSet {
    pub id: Uuid,
    pub title: String,
    pub collapsed: bool,
    pub is_my_tools: bool,
    pub is_recents: bool,
    pub items: Vec<ToolChestItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChest {
    pub sets: Vec<ToolSet>,
}

impl Default for ToolChest {
    fn default() -> Self {
        Self::default_chilean_sets()
    }
}

impl ToolChest {
    pub fn storage_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("cl", "RedBIM", "RedBIM")
            .map(|d| d.config_dir().join("tool_chest.json"))
    }

    pub fn load_or_default() -> Self {
        if let Some(path) = Self::storage_path()
            && let Ok(raw) = std::fs::read_to_string(&path)
            && let Ok(parsed) = serde_json::from_str::<Self>(&raw)
            && !parsed.sets.is_empty()
        {
            return parsed;
        }
        Self::default_chilean_sets()
    }

    pub fn save(&self) {
        if let Some(path) = Self::storage_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn push_recent(&mut self, item: ToolChestItem) {
        if let Some(recents) = self.sets.iter_mut().find(|s| s.is_recents) {
            recents
                .items
                .retain(|x| !(x.tool == item.tool && x.subject == item.subject));
            recents.items.insert(0, item);
            recents.items.truncate(12);
        }
    }

    pub fn add_to_my_tools(&mut self, item: ToolChestItem) {
        if let Some(my) = self.sets.iter_mut().find(|s| s.is_my_tools) {
            my.items.push(item);
        }
        self.save();
    }

    pub fn default_chilean_sets() -> Self {
        let red = [229, 57, 53, 255];
        let blue = [37, 99, 235, 255];
        let amber = [245, 158, 11, 255];
        let green = [22, 163, 74, 255];
        let cyan = [6, 182, 212, 255];
        let purple = [147, 51, 234, 255];
        let yellow_hi = [253, 224, 71, 255];

        let item = |name: &str,
                    subject: &str,
                    tool: ActiveTool,
                    style: MarkupStyle,
                    default_text: &str| ToolChestItem {
            id: Uuid::new_v4(),
            name: name.to_string(),
            subject: subject.to_string(),
            tool,
            style,
            default_text: default_text.to_string(),
            stamp_subtitle: String::new(),
            measurement_opts: if tool.is_measurement() {
                Some(MeasurementOptions::default())
            } else {
                None
            },
            properties_mode: false,
        };

        let my_tools = ToolSet {
            id: Uuid::new_v4(),
            title: "Mis herramientas".to_string(),
            collapsed: false,
            is_my_tools: true,
            is_recents: false,
            items: vec![
                item(
                    "Nota amarilla",
                    "Nota de revisión",
                    ActiveTool::Note,
                    MarkupStyle {
                        stroke_color: [250, 204, 21, 255],
                        fill_color: Some([254, 240, 138, 255]),
                        note_icon: NoteIcon::Note,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Llamada roja",
                    "Observación",
                    ActiveTool::Callout,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([255, 255, 255, 240]),
                        fill_opacity: 0.92,
                        end_end: LineEnd::ClosedArrow,
                        text_color: red,
                        ..Default::default()
                    },
                    "Revisar detalle",
                ),
                item(
                    "Nube+ roja",
                    "Nube+ revisión",
                    ActiveTool::CloudPlus,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([255, 255, 255, 240]),
                        fill_opacity: 0.92,
                        line_style: LineStyle::Cloud,
                        end_end: LineEnd::ClosedArrow,
                        text_color: red,
                        ..Default::default()
                    },
                    "Modificar según plano",
                ),
                item(
                    "Flecha roja",
                    "Flecha",
                    ActiveTool::Arrow,
                    MarkupStyle {
                        stroke_color: red,
                        line_width: 2.5,
                        end_end: LineEnd::ClosedArrow,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Cuadro de texto",
                    "Texto",
                    ActiveTool::TextBox,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([255, 255, 255, 240]),
                        fill_opacity: 0.92,
                        text_color: [24, 24, 27, 255],
                        ..Default::default()
                    },
                    "Comentario",
                ),
                item(
                    "Nube revisión",
                    "Nube",
                    ActiveTool::Cloud,
                    MarkupStyle {
                        stroke_color: red,
                        line_style: LineStyle::Cloud,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Cota metros",
                    "Longitud (m)",
                    ActiveTool::MeasureLength,
                    MarkupStyle {
                        stroke_color: red,
                        start_end: LineEnd::Slash,
                        end_end: LineEnd::Slash,
                        text_color: red,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Conteo ✓",
                    "Conteo",
                    ActiveTool::MeasureCount,
                    MarkupStyle {
                        stroke_color: green,
                        count_style: CountStyle::Checkmark,
                        ..Default::default()
                    },
                    "",
                ),
            ],
        };

        let recents = ToolSet {
            id: Uuid::new_v4(),
            title: "Recientes".to_string(),
            collapsed: false,
            is_my_tools: false,
            is_recents: true,
            items: Vec::new(),
        };

        let architect = ToolSet {
            id: Uuid::new_v4(),
            title: "Arquitecto revisor".to_string(),
            collapsed: false,
            is_my_tools: false,
            is_recents: false,
            items: vec![
                item(
                    "Nube+ Arquitectura",
                    "Obs. Arquitectura",
                    ActiveTool::CloudPlus,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([255, 255, 255, 240]),
                        fill_opacity: 0.92,
                        line_style: LineStyle::Cloud,
                        end_end: LineEnd::ClosedArrow,
                        text_color: red,
                        ..Default::default()
                    },
                    "Verificar cota / plomo",
                ),
                item(
                    "Llamada Arquitectura",
                    "Obs. Arquitectura",
                    ActiveTool::Callout,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([255, 255, 255, 240]),
                        fill_opacity: 0.92,
                        end_end: LineEnd::ClosedArrow,
                        text_color: red,
                        ..Default::default()
                    },
                    "Cambiar especificación",
                ),
                item(
                    "Texto círculo",
                    "Referencia",
                    ActiveTool::TextBox,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([254, 226, 226, 230]),
                        fill_opacity: 0.9,
                        box_shape: BoxShape::Circle,
                        text_color: red,
                        ..Default::default()
                    },
                    "R1",
                ),
                item(
                    "Flecha revisión",
                    "Flecha",
                    ActiveTool::Arrow,
                    MarkupStyle {
                        stroke_color: red,
                        line_width: 2.5,
                        end_end: LineEnd::ClosedArrow,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Check rojo",
                    "Verificado",
                    ActiveTool::Note,
                    MarkupStyle {
                        stroke_color: red,
                        fill_color: Some([254, 226, 226, 255]),
                        note_icon: NoteIcon::Checkmark,
                        ..Default::default()
                    },
                    "",
                ),
            ],
        };

        let contractor = ToolSet {
            id: Uuid::new_v4(),
            title: "Contratista revisor".to_string(),
            collapsed: false,
            is_my_tools: false,
            is_recents: false,
            items: vec![
                item(
                    "Nube+ RDI / RFI",
                    "RDI Terreno",
                    ActiveTool::CloudPlus,
                    MarkupStyle {
                        stroke_color: amber,
                        fill_color: Some([255, 251, 235, 245]),
                        fill_opacity: 0.92,
                        line_style: LineStyle::Cloud,
                        end_end: LineEnd::ClosedArrow,
                        text_color: [180, 83, 9, 255],
                        ..Default::default()
                    },
                    "RDI: confirmar en obra",
                ),
                item(
                    "Llamada Contratista",
                    "Consulta Contratista",
                    ActiveTool::Callout,
                    MarkupStyle {
                        stroke_color: amber,
                        fill_color: Some([255, 251, 235, 245]),
                        fill_opacity: 0.92,
                        end_end: LineEnd::ClosedArrow,
                        text_color: [180, 83, 9, 255],
                        ..Default::default()
                    },
                    "Interferencia detectada",
                ),
                item(
                    "Resaltador amarillo",
                    "Resaltado",
                    ActiveTool::Highlight,
                    MarkupStyle {
                        stroke_color: yellow_hi,
                        line_width: 14.0,
                        opacity: 0.42,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Nota Consulta (?)",
                    "Consulta",
                    ActiveTool::Note,
                    MarkupStyle {
                        stroke_color: amber,
                        fill_color: Some([254, 243, 199, 255]),
                        note_icon: NoteIcon::Help,
                        ..Default::default()
                    },
                    "",
                ),
            ],
        };

        let engineer = ToolSet {
            id: Uuid::new_v4(),
            title: "Ingeniero revisor".to_string(),
            collapsed: false,
            is_my_tools: false,
            is_recents: false,
            items: vec![
                item(
                    "Nube+ Cálculo",
                    "Obs. Estructural",
                    ActiveTool::CloudPlus,
                    MarkupStyle {
                        stroke_color: blue,
                        fill_color: Some([239, 246, 255, 245]),
                        fill_opacity: 0.92,
                        line_style: LineStyle::Cloud,
                        end_end: LineEnd::ClosedArrow,
                        text_color: blue,
                        ..Default::default()
                    },
                    "Verificar armadura / perfil",
                ),
                item(
                    "Llamada Ingeniería",
                    "Nota Ingeniería",
                    ActiveTool::Callout,
                    MarkupStyle {
                        stroke_color: cyan,
                        fill_color: Some([236, 254, 255, 245]),
                        fill_opacity: 0.92,
                        end_end: LineEnd::ClosedArrow,
                        text_color: [14, 116, 144, 255],
                        ..Default::default()
                    },
                    "Ver detalle estructural",
                ),
                item(
                    "Flecha azul",
                    "Flecha",
                    ActiveTool::Arrow,
                    MarkupStyle {
                        stroke_color: blue,
                        line_width: 2.5,
                        end_end: LineEnd::ClosedArrow,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Área achurada",
                    "Zona estructural",
                    ActiveTool::Polygon,
                    MarkupStyle {
                        stroke_color: blue,
                        fill_color: Some(blue),
                        fill_opacity: 0.18,
                        hatch: HatchPattern::Diagonal45,
                        ..Default::default()
                    },
                    "",
                ),
            ],
        };

        let measurements = ToolSet {
            id: Uuid::new_v4(),
            title: "Mediciones generales (Chile)".to_string(),
            collapsed: false,
            is_my_tools: false,
            is_recents: false,
            items: vec![
                item(
                    "Longitud (m)",
                    "Longitud (m)",
                    ActiveTool::MeasureLength,
                    MarkupStyle {
                        stroke_color: red,
                        start_end: LineEnd::Slash,
                        end_end: LineEnd::Slash,
                        text_color: red,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Polilongitud (m)",
                    "Trazado (m)",
                    ActiveTool::MeasurePolylength,
                    MarkupStyle {
                        stroke_color: cyan,
                        text_color: cyan,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Perímetro (m)",
                    "Perímetro (m)",
                    ActiveTool::MeasurePerimeter,
                    MarkupStyle {
                        stroke_color: purple,
                        text_color: purple,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Área (m²)",
                    "Superficie (m²)",
                    ActiveTool::MeasureArea,
                    MarkupStyle {
                        stroke_color: green,
                        fill_color: Some(green),
                        fill_opacity: 0.22,
                        hatch: HatchPattern::Diagonal45,
                        text_color: green,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Volumen (m³)",
                    "Cubicación (m³)",
                    ActiveTool::MeasureVolume,
                    MarkupStyle {
                        stroke_color: blue,
                        fill_color: Some(blue),
                        fill_opacity: 0.22,
                        hatch: HatchPattern::CrossHatch,
                        text_color: blue,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Ángulo (°)",
                    "Ángulo",
                    ActiveTool::MeasureAngle,
                    MarkupStyle {
                        stroke_color: amber,
                        text_color: amber,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Radio (m)",
                    "Radio",
                    ActiveTool::MeasureRadius,
                    MarkupStyle {
                        stroke_color: purple,
                        text_color: purple,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Diámetro (m)",
                    "Diámetro",
                    ActiveTool::MeasureDiameter,
                    MarkupStyle {
                        stroke_color: purple,
                        text_color: purple,
                        ..Default::default()
                    },
                    "",
                ),
                item(
                    "Conteo (und)",
                    "Conteo unidades",
                    ActiveTool::MeasureCount,
                    MarkupStyle {
                        stroke_color: green,
                        count_style: CountStyle::Circle,
                        ..Default::default()
                    },
                    "",
                ),
            ],
        };

        let stamp_item = |name: &str, title: &str, sub: &str, color: [u8; 4]| {
            let mut it = item(
                name,
                "Sello",
                ActiveTool::Stamp,
                MarkupStyle {
                    stroke_color: color,
                    fill_color: Some([color[0], color[1], color[2], 32]),
                    fill_opacity: 0.14,
                    text_color: color,
                    bold: true,
                    font_size: 15.0,
                    ..Default::default()
                },
                title,
            );
            it.stamp_subtitle = sub.to_string();
            it
        };

        let stamps = ToolSet {
            id: Uuid::new_v4(),
            title: "Símbolos y sellos".to_string(),
            collapsed: true,
            is_my_tools: false,
            is_recents: false,
            items: vec![
                stamp_item("APROBADO", "APROBADO", "Revisión técnica conforme", green),
                stamp_item("RECHAZADO", "RECHAZADO", "Corregir observaciones", red),
                stamp_item("PRELIMINAR", "PRELIMINAR", "No apto para construcción", amber),
                stamp_item(
                    "PARA CONSTRUCCIÓN",
                    "PARA CONSTRUCCIÓN",
                    "Emitido para obra - Chile",
                    blue,
                ),
                stamp_item("AS-BUILT", "AS-BUILT", "Plano conforme a obra", cyan),
            ],
        };

        // Example explicit unit override item for American measure convenience
        let mut us_len = item(
            "Longitud (ft-in)",
            "Longitud Imperial",
            ActiveTool::MeasureLength,
            MarkupStyle {
                stroke_color: amber,
                start_end: LineEnd::ClosedArrow,
                end_end: LineEnd::ClosedArrow,
                text_color: amber,
                ..Default::default()
            },
            "",
        );
        us_len.measurement_opts = Some(MeasurementOptions {
            length_unit_override: Some(LengthUnit::FootInch),
            area_unit_override: Some(AreaUnit::SqFoot),
            volume_unit_override: Some(VolumeUnit::CuFoot),
            ..Default::default()
        });
        let mut imperial_set = measurements;
        imperial_set.items.push(us_len);

        Self {
            sets: vec![
                my_tools,
                recents,
                architect,
                contractor,
                engineer,
                imperial_set,
                stamps,
            ],
        }
    }
}
