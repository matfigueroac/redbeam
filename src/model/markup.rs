use crate::geometry::math::{
    Point2, Rect2, angle_three_points_deg, circle_from_three_points, dimension_offset_segment,
    polygon_centroid,
};
use crate::model::custom_columns::CustomColumnValue;
use crate::model::scale::{PageScale, ViewportScale, resolve_scale_at};
use crate::model::units::{
    AreaUnit, LengthUnit, Precision, SlopeFormat, VolumeUnit, format_angle, format_area,
    format_length, format_slope, format_volume,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MarkupStatus {
    #[default]
    None,
    Accepted,
    Rejected,
    Cancelled,
    Completed,
    Pending,
}

impl MarkupStatus {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Pending,
        Self::Accepted,
        Self::Completed,
        Self::Rejected,
        Self::Cancelled,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Sin estado",
            Self::Accepted => "Aceptado",
            Self::Rejected => "Rechazado",
            Self::Cancelled => "Anulado",
            Self::Completed => "Completado",
            Self::Pending => "Pendiente",
        }
    }

    pub fn badge_color(self) -> [u8; 4] {
        match self {
            Self::None => [120, 128, 145, 255],
            Self::Accepted => [34, 197, 94, 255],
            Self::Rejected => [239, 68, 68, 255],
            Self::Cancelled => [156, 163, 175, 255],
            Self::Completed => [56, 189, 248, 255],
            Self::Pending => [245, 158, 11, 255],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
    Cloud,
}

impl LineStyle {
    pub const ALL: &'static [Self] = &[
        Self::Solid,
        Self::Dashed,
        Self::Dotted,
        Self::DashDot,
        Self::Cloud,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Continua",
            Self::Dashed => "Segmentada",
            Self::Dotted => "Punteada",
            Self::DashDot => "Trazo-Punto",
            Self::Cloud => "Nube",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HatchPattern {
    #[default]
    None,
    Diagonal45,
    Diagonal135,
    CrossHatch,
    Horizontal,
    Vertical,
}

impl HatchPattern {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Diagonal45,
        Self::Diagonal135,
        Self::CrossHatch,
        Self::Horizontal,
        Self::Vertical,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Sin achurado",
            Self::Diagonal45 => "Diagonal 45°",
            Self::Diagonal135 => "Diagonal 135°",
            Self::CrossHatch => "Cuadrícula 45°",
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
        }
    }
}

/// Mirrors `Bluebeam.Types.Core.Enums.LineEndEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LineEnd {
    #[default]
    None,
    OpenArrow,
    ClosedArrow,
    ROpenArrow,
    RClosedArrow,
    Square,
    Circle,
    Diamond,
    Butt,
    Slash,
}

impl LineEnd {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::OpenArrow,
        Self::ClosedArrow,
        Self::ROpenArrow,
        Self::RClosedArrow,
        Self::Slash,
        Self::Butt,
        Self::Circle,
        Self::Square,
        Self::Diamond,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Ninguno",
            Self::OpenArrow => "Flecha abierta",
            Self::ClosedArrow => "Flecha rellena",
            Self::ROpenArrow => "Flecha inv. abierta",
            Self::RClosedArrow => "Flecha inv. rellena",
            Self::Square => "Cuadrado",
            Self::Circle => "Círculo",
            Self::Diamond => "Diamante",
            Self::Butt => "Tope",
            Self::Slash => "Diagonal (/)",
        }
    }

    pub fn pdf_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::OpenArrow => "OpenArrow",
            Self::ClosedArrow => "ClosedArrow",
            Self::ROpenArrow => "ROpenArrow",
            Self::RClosedArrow => "RClosedArrow",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::Butt => "Butt",
            Self::Slash => "Slash",
        }
    }
}

/// Mirrors `Bluebeam.Types.Core.Enums.FreeTextShapeEnu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BoxShape {
    #[default]
    Rectangle,
    Circle,
    Triangle,
    Hexagon,
}

impl BoxShape {
    pub const ALL: &'static [Self] = &[Self::Rectangle, Self::Circle, Self::Triangle, Self::Hexagon];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectángulo",
            Self::Circle => "Óvalo / Círculo",
            Self::Triangle => "Triángulo",
            Self::Hexagon => "Hexágono",
        }
    }
}

/// Mirrors `Bluebeam.Revu.Model.Core.Enums.Annotations.NoteIconEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum NoteIcon {
    #[default]
    Comment,
    Note,
    Checkmark,
    Cross,
    Help,
    Key,
    Star,
    Circle,
    Insert,
    NewParagraph,
    Paragraph,
    RightArrow,
    RightPointer,
    UpArrow,
    UpLeftArrow,
}

impl NoteIcon {
    pub const ALL: &'static [Self] = &[
        Self::Comment,
        Self::Note,
        Self::Checkmark,
        Self::Cross,
        Self::Help,
        Self::Key,
        Self::Star,
        Self::Circle,
        Self::Insert,
        Self::NewParagraph,
        Self::Paragraph,
        Self::RightArrow,
        Self::RightPointer,
        Self::UpArrow,
        Self::UpLeftArrow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Comment => "Comentario",
            Self::Note => "Nota",
            Self::Checkmark => "Aprobado",
            Self::Cross => "Cruz / Alerta",
            Self::Help => "Consulta (?)",
            Self::Key => "Clave",
            Self::Star => "Destacado",
            Self::Circle => "Círculo",
            Self::Insert => "Insertar",
            Self::NewParagraph => "Nuevo párrafo",
            Self::Paragraph => "Párrafo",
            Self::RightArrow => "Flecha der.",
            Self::RightPointer => "Puntero der.",
            Self::UpArrow => "Flecha arriba",
            Self::UpLeftArrow => "Flecha diag.",
        }
    }
}

/// Mirrors `Bluebeam.Types.Core.Enums.MeasureCountStyleEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CountStyle {
    #[default]
    Checkmark,
    Circle,
    Square,
    Triangle,
    Diamond,
}

impl CountStyle {
    pub const ALL: &'static [Self] = &[
        Self::Checkmark,
        Self::Circle,
        Self::Square,
        Self::Triangle,
        Self::Diamond,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Checkmark => "Check (✓)",
            Self::Circle => "Círculo (●)",
            Self::Square => "Cuadrado (■)",
            Self::Triangle => "Triángulo (▲)",
            Self::Diamond => "Diamante (◆)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentReply {
    pub id: Uuid,
    pub author: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkupStyle {
    pub stroke_color: [u8; 4],
    pub fill_color: Option<[u8; 4]>,
    pub opacity: f32,
    pub fill_opacity: f32,
    pub line_width: f32,
    pub line_style: LineStyle,
    pub cloud_arc_size: f32,
    pub hatch: HatchPattern,
    pub start_end: LineEnd,
    pub end_end: LineEnd,
    pub font_size: f32,
    pub text_color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub box_shape: BoxShape,
    pub note_icon: NoteIcon,
    pub count_style: CountStyle,
}

impl Default for MarkupStyle {
    fn default() -> Self {
        Self {
            stroke_color: [229, 57, 53, 255],
            fill_color: None,
            opacity: 1.0,
            fill_opacity: 0.25,
            line_width: 2.0,
            line_style: LineStyle::Solid,
            cloud_arc_size: 16.0,
            hatch: HatchPattern::None,
            start_end: LineEnd::None,
            end_end: LineEnd::None,
            font_size: 11.0,
            text_color: [229, 57, 53, 255],
            bold: false,
            italic: false,
            align: TextAlign::Left,
            box_shape: BoxShape::Rectangle,
            note_icon: NoteIcon::Comment,
            count_style: CountStyle::Checkmark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementOptions {
    pub show_caption: bool,
    pub show_segments: bool,
    pub show_centroid: bool,
    pub depth_meters: f64,
    pub rise_drop_meters: f64,
    pub length_unit_override: Option<LengthUnit>,
    pub area_unit_override: Option<AreaUnit>,
    pub volume_unit_override: Option<VolumeUnit>,
    pub slope_format_override: Option<SlopeFormat>,
    pub precision_override: Option<Precision>,
}

impl Default for MeasurementOptions {
    fn default() -> Self {
        Self {
            show_caption: true,
            show_segments: false,
            show_centroid: false,
            depth_meters: 1.0,
            rise_drop_meters: 0.0,
            length_unit_override: None,
            area_unit_override: None,
            volume_unit_override: None,
            slope_format_override: None,
            precision_override: None,
        }
    }
}

/// Destination of a clickable PDF Hyperlink markup (mirrors Bluebeam's `IAnnotationHyperlinkModel`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HyperlinkTarget {
    Page { page_index: usize },
    Url(String),
}

impl Default for HyperlinkTarget {
    fn default() -> Self {
        Self::Page { page_index: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarkupKind {
    // --- Comentarios y texto ---
    Note {
        pos: Point2,
    },
    TextBox {
        rect: Rect2,
        text: String,
    },
    Typewriter {
        pos: Point2,
        text: String,
    },
    Callout {
        arrow_tip: Point2,
        knee: Point2,
        box_rect: Rect2,
        text: String,
    },
    Cloud {
        points: Vec<Point2>,
        is_rect: bool,
    },
    CloudPlus {
        cloud_points: Vec<Point2>,
        is_rect: bool,
        knee: Point2,
        box_rect: Rect2,
        text: String,
    },

    // --- Dibujo y marcas geométricas ---
    Line {
        start: Point2,
        end: Point2,
    },
    Arrow {
        start: Point2,
        end: Point2,
    },
    Polyline {
        points: Vec<Point2>,
    },
    Arc {
        start: Point2,
        mid: Point2,
        end: Point2,
    },
    Rectangle {
        rect: Rect2,
    },
    Ellipse {
        rect: Rect2,
    },
    Polygon {
        points: Vec<Point2>,
    },
    Ink {
        strokes: Vec<Vec<Point2>>,
    },
    Highlight {
        strokes: Vec<Vec<Point2>>,
    },
    Stamp {
        rect: Rect2,
        title: String,
        subtitle: String,
        show_date: bool,
        show_author: bool,
    },
    Image {
        rect: Rect2,
        png_b64: Arc<str>,
        caption: String,
    },
    Legend {
        pos: Point2,
        title: String,
        width_pt: f64,
    },
    Hyperlink {
        rect: Rect2,
        target: HyperlinkTarget,
        description: String,
    },
    /// Destructive redaction zone (Bluebeam `IAnnotationRedactionModel` / `SCmdRedact`).
    Redaction {
        rect: Rect2,
        overlay_text: String,
    },

    // --- Mediciones a escala (Chile métrico + EE.UU. imperial) ---
    MeasureLength {
        start: Point2,
        end: Point2,
        offset_pt: f64,
        opts: MeasurementOptions,
    },
    MeasurePolylength {
        points: Vec<Point2>,
        opts: MeasurementOptions,
    },
    MeasurePerimeter {
        points: Vec<Point2>,
        closed: bool,
        opts: MeasurementOptions,
    },
    MeasureArea {
        points: Vec<Point2>,
        cutouts: Vec<Vec<Point2>>,
        opts: MeasurementOptions,
    },
    MeasureVolume {
        points: Vec<Point2>,
        cutouts: Vec<Vec<Point2>>,
        opts: MeasurementOptions,
    },
    MeasureAngle {
        p1: Point2,
        vertex: Point2,
        p2: Point2,
        opts: MeasurementOptions,
    },
    MeasureRadius {
        center: Point2,
        edge: Point2,
        three_point: Option<[Point2; 3]>,
        opts: MeasurementOptions,
    },
    MeasureDiameter {
        p1: Point2,
        p2: Point2,
        opts: MeasurementOptions,
    },
    MeasureCount {
        points: Vec<Point2>,
        opts: MeasurementOptions,
    },
    SketchRect {
        rect: Rect2,
        opts: MeasurementOptions,
    },
    SketchPoly {
        points: Vec<Point2>,
        closed: bool,
        opts: MeasurementOptions,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    pub length_m: Option<f64>,
    pub perimeter_m: Option<f64>,
    pub area_m2: Option<f64>,
    pub cutout_area_m2: Option<f64>,
    pub volume_m3: Option<f64>,
    pub wall_area_m2: Option<f64>,
    pub width_m: Option<f64>,
    pub height_m: Option<f64>,
    pub angle_deg: Option<f64>,
    pub radius_m: Option<f64>,
    pub diameter_m: Option<f64>,
    pub slope_ratio: Option<f64>,
    pub count: Option<usize>,
    pub primary_display: Option<String>,
    pub segment_displays: Vec<(Point2, Point2, String)>,
}

/// Site photo / punch-list image attached to any markup (Bluebeam Markup Capture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkupAttachment {
    pub id: Uuid,
    pub caption: String,
    pub image_b64: Arc<str>,
    pub captured_at: DateTime<Utc>,
}

impl MarkupAttachment {
    /// Decodes a local PNG/JPEG photo, downscales to at most `1024px` max dimension
    /// to keep embedded PDF sizes fast & compact, and encodes as base64 JPEG.
    pub fn from_image_bytes(caption: impl Into<String>, raw_bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(raw_bytes)
            .map_err(|e| format!("Imagen no válida: {e}"))?;
        let resized = if img.width() > 1024 || img.height() > 1024 {
            img.thumbnail(1024, 1024)
        } else {
            img
        };
        let mut encoded = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut encoded);
            resized
                .to_rgb8()
                .write_to(&mut cursor, image::ImageFormat::Jpeg)
                .map_err(|e| format!("Error comprimiendo foto: {e}"))?;
        }
        Ok(Self {
            id: Uuid::new_v4(),
            caption: caption.into(),
            image_b64: crate::pdf::base64_encode(&encoded).into(),
            captured_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Markup {
    pub id: Uuid,
    pub page_index: usize,
    pub subject: String,
    pub label: String,
    pub author: String,
    pub comment: String,
    pub replies: Vec<CommentReply>,
    pub status: MarkupStatus,
    pub layer: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub locked: bool,
    pub group_id: Option<Uuid>,
    #[serde(default)]
    pub unit_cost: Option<f64>,
    #[serde(default)]
    pub custom_values: BTreeMap<Uuid, CustomColumnValue>,
    #[serde(default)]
    pub attachments: Vec<MarkupAttachment>,
    pub style: MarkupStyle,
    pub kind: MarkupKind,
}

impl Markup {
    pub fn new(
        page_index: usize,
        subject: impl Into<String>,
        author: impl Into<String>,
        style: MarkupStyle,
        kind: MarkupKind,
    ) -> Self {
        let now = Utc::now();
        let comment = match &kind {
            MarkupKind::TextBox { text, .. }
            | MarkupKind::Typewriter { text, .. }
            | MarkupKind::Callout { text, .. }
            | MarkupKind::CloudPlus { text, .. } => text.clone(),
            MarkupKind::Stamp { title, .. } => title.clone(),
            MarkupKind::Hyperlink { description, .. } => description.clone(),
            MarkupKind::Redaction { overlay_text, .. } => overlay_text.clone(),
            _ => String::new(),
        };
        Self {
            id: Uuid::new_v4(),
            page_index,
            subject: subject.into(),
            label: String::new(),
            author: author.into(),
            comment,
            replies: Vec::new(),
            status: MarkupStatus::None,
            layer: "General".to_string(),
            created_at: now,
            modified_at: now,
            locked: false,
            group_id: None,
            unit_cost: None,
            custom_values: BTreeMap::new(),
            attachments: Vec::new(),
            style,
            kind,
        }
    }

    pub fn type_name_es(&self) -> &'static str {
        match &self.kind {
            MarkupKind::Note { .. } => "Nota",
            MarkupKind::TextBox { .. } => "Cuadro de texto",
            MarkupKind::Typewriter { .. } => "Máquina de escribir",
            MarkupKind::Callout { .. } => "Llamada",
            MarkupKind::Cloud { .. } => "Nube",
            MarkupKind::CloudPlus { .. } => "Nube+",
            MarkupKind::Line { .. } => "Línea",
            MarkupKind::Arrow { .. } => "Flecha",
            MarkupKind::Polyline { .. } => "Polilínea",
            MarkupKind::Arc { .. } => "Arco",
            MarkupKind::Rectangle { .. } => "Rectángulo",
            MarkupKind::Ellipse { .. } => "Elipse",
            MarkupKind::Polygon { .. } => "Polígono",
            MarkupKind::Ink { .. } => "Lápiz",
            MarkupKind::Highlight { .. } => "Resaltador",
            MarkupKind::Stamp { .. } => "Sello",
            MarkupKind::Image { .. } => "Imagen",
            MarkupKind::Legend { .. } => "Leyenda",
            MarkupKind::Hyperlink { .. } => "Hipervínculo",
            MarkupKind::Redaction { .. } => "Redacción (Censura)",
            MarkupKind::MeasureLength { .. } => "Longitud",
            MarkupKind::MeasurePolylength { .. } => "Polilongitud",
            MarkupKind::MeasurePerimeter { .. } => "Perímetro",
            MarkupKind::MeasureArea { .. } => "Área",
            MarkupKind::MeasureVolume { .. } => "Volumen",
            MarkupKind::MeasureAngle { .. } => "Ángulo",
            MarkupKind::MeasureRadius { .. } => "Radio",
            MarkupKind::MeasureDiameter { .. } => "Diámetro",
            MarkupKind::MeasureCount { .. } => "Conteo",
            MarkupKind::SketchRect { .. } => "Rectángulo a escala",
            MarkupKind::SketchPoly { .. } => "Polígono a escala",
        }
    }

    pub fn is_measurement(&self) -> bool {
        matches!(
            self.kind,
            MarkupKind::MeasureLength { .. }
                | MarkupKind::MeasurePolylength { .. }
                | MarkupKind::MeasurePerimeter { .. }
                | MarkupKind::MeasureArea { .. }
                | MarkupKind::MeasureVolume { .. }
                | MarkupKind::MeasureAngle { .. }
                | MarkupKind::MeasureRadius { .. }
                | MarkupKind::MeasureDiameter { .. }
                | MarkupKind::MeasureCount { .. }
                | MarkupKind::SketchRect { .. }
                | MarkupKind::SketchPoly { .. }
        )
    }

    pub fn measurement_opts(&self) -> Option<&MeasurementOptions> {
        match &self.kind {
            MarkupKind::MeasureLength { opts, .. }
            | MarkupKind::MeasurePolylength { opts, .. }
            | MarkupKind::MeasurePerimeter { opts, .. }
            | MarkupKind::MeasureArea { opts, .. }
            | MarkupKind::MeasureVolume { opts, .. }
            | MarkupKind::MeasureAngle { opts, .. }
            | MarkupKind::MeasureRadius { opts, .. }
            | MarkupKind::MeasureDiameter { opts, .. }
            | MarkupKind::MeasureCount { opts, .. }
            | MarkupKind::SketchRect { opts, .. }
            | MarkupKind::SketchPoly { opts, .. } => Some(opts),
            _ => None,
        }
    }

    pub fn measurement_opts_mut(&mut self) -> Option<&mut MeasurementOptions> {
        match &mut self.kind {
            MarkupKind::MeasureLength { opts, .. }
            | MarkupKind::MeasurePolylength { opts, .. }
            | MarkupKind::MeasurePerimeter { opts, .. }
            | MarkupKind::MeasureArea { opts, .. }
            | MarkupKind::MeasureVolume { opts, .. }
            | MarkupKind::MeasureAngle { opts, .. }
            | MarkupKind::MeasureRadius { opts, .. }
            | MarkupKind::MeasureDiameter { opts, .. }
            | MarkupKind::MeasureCount { opts, .. }
            | MarkupKind::SketchRect { opts, .. }
            | MarkupKind::SketchPoly { opts, .. } => Some(opts),
            _ => None,
        }
    }

    pub fn text_content(&self) -> Option<&str> {
        match &self.kind {
            MarkupKind::TextBox { text, .. }
            | MarkupKind::Typewriter { text, .. }
            | MarkupKind::Callout { text, .. }
            | MarkupKind::CloudPlus { text, .. } => Some(text),
            MarkupKind::Stamp { title, .. } => Some(title),
            MarkupKind::Legend { title, .. } => Some(title),
            MarkupKind::Hyperlink { description, .. } => Some(description),
            MarkupKind::Redaction { overlay_text, .. } => Some(overlay_text),
            _ => None,
        }
    }

    pub fn set_text_content(&mut self, new_text: String) {
        self.modified_at = Utc::now();
        match &mut self.kind {
            MarkupKind::TextBox { text, .. }
            | MarkupKind::Typewriter { text, .. }
            | MarkupKind::Callout { text, .. }
            | MarkupKind::CloudPlus { text, .. } => {
                *text = new_text.clone();
                self.comment = new_text;
            }
            MarkupKind::Stamp { title, .. } => {
                *title = new_text.clone();
                self.comment = new_text;
            }
            MarkupKind::Legend { title, .. } => {
                *title = new_text;
            }
            MarkupKind::Hyperlink { description, .. } => {
                *description = new_text.clone();
                self.comment = new_text;
            }
            MarkupKind::Redaction { overlay_text, .. } => {
                *overlay_text = new_text.clone();
                self.comment = new_text;
            }
            _ => {
                self.comment = new_text;
            }
        }
    }

    pub fn anchor_point(&self) -> Point2 {
        match &self.kind {
            MarkupKind::Note { pos } | MarkupKind::Typewriter { pos, .. } => *pos,
            MarkupKind::TextBox { rect, .. }
            | MarkupKind::Rectangle { rect }
            | MarkupKind::Ellipse { rect }
            | MarkupKind::Stamp { rect, .. }
            | MarkupKind::Image { rect, .. }
            | MarkupKind::Hyperlink { rect, .. }
            | MarkupKind::Redaction { rect, .. }
            | MarkupKind::SketchRect { rect, .. } => rect.center(),
            MarkupKind::Callout { arrow_tip, .. } => *arrow_tip,
            MarkupKind::Cloud { points, .. }
            | MarkupKind::Polyline { points }
            | MarkupKind::Polygon { points }
            | MarkupKind::MeasurePolylength { points, .. }
            | MarkupKind::MeasurePerimeter { points, .. }
            | MarkupKind::MeasureArea { points, .. }
            | MarkupKind::MeasureVolume { points, .. }
            | MarkupKind::MeasureCount { points, .. }
            | MarkupKind::SketchPoly { points, .. } => polygon_centroid(points),
            MarkupKind::CloudPlus { cloud_points, .. } => polygon_centroid(cloud_points),
            MarkupKind::Line { start, end }
            | MarkupKind::Arrow { start, end }
            | MarkupKind::MeasureLength { start, end, .. } => start.midpoint(*end),
            MarkupKind::Arc { mid, .. } => *mid,
            MarkupKind::Ink { strokes } | MarkupKind::Highlight { strokes, .. } => strokes
                .first()
                .and_then(|s| s.first().copied())
                .unwrap_or_default(),
            MarkupKind::Legend { pos, .. } => *pos,
            MarkupKind::MeasureAngle { vertex, .. } => *vertex,
            MarkupKind::MeasureRadius { center, .. } => *center,
            MarkupKind::MeasureDiameter { p1, p2, .. } => p1.midpoint(*p2),
        }
    }

    pub fn compute_metrics(
        &self,
        page_scale: &PageScale,
        viewports: &[ViewportScale],
    ) -> ComputedMetrics {
        let anchor = self.anchor_point();
        let scale = resolve_scale_at(page_scale, viewports, anchor);
        let mut out = ComputedMetrics::default();

        let Some(opts) = self.measurement_opts() else {
            return out;
        };

        let len_unit = opts.length_unit_override.unwrap_or(scale.display_unit);
        let area_unit = opts.area_unit_override.unwrap_or(scale.area_unit);
        let vol_unit = opts.volume_unit_override.unwrap_or(scale.volume_unit);
        let prec = opts.precision_override.unwrap_or(scale.precision);
        let slope_fmt = opts.slope_format_override.unwrap_or(scale.slope_format);

        let fmt_len = |m: f64| format_length(m, len_unit, prec);
        let fmt_area = |m2: f64| format_area(m2, area_unit, prec);
        let fmt_vol = |m3: f64| format_volume(m3, vol_unit, prec);

        match &self.kind {
            MarkupKind::MeasureLength {
                start,
                end,
                offset_pt,
                ..
            } => {
                let d_m = scale.distance_meters(*start, *end);
                out.length_m = Some(d_m);
                let mut text = fmt_len(d_m);
                if opts.rise_drop_meters.abs() > 1e-6 && d_m > 1e-9 {
                    let slope = opts.rise_drop_meters / d_m;
                    out.slope_ratio = Some(slope);
                    text = format!("{text} ({})", format_slope(slope, slope_fmt));
                }
                out.primary_display = Some(text);
                let (da, db, _) = dimension_offset_segment(*start, *end, *offset_pt);
                let _ = (da, db);
            }
            MarkupKind::MeasurePolylength { points, .. } => {
                let total = scale.polyline_length_meters(points, false);
                out.length_m = Some(total);
                out.primary_display = Some(fmt_len(total));
                if opts.show_segments {
                    for w in points.windows(2) {
                        let seg = scale.distance_meters(w[0], w[1]);
                        out.segment_displays.push((w[0], w[1], fmt_len(seg)));
                    }
                }
            }
            MarkupKind::MeasurePerimeter { points, closed, .. } => {
                let total = scale.polyline_length_meters(points, *closed);
                out.perimeter_m = Some(total);
                if opts.depth_meters > 0.0 {
                    out.wall_area_m2 = Some(total * opts.depth_meters);
                }
                out.primary_display = Some(fmt_len(total));
                if opts.show_segments && points.len() >= 2 {
                    for w in points.windows(2) {
                        let seg = scale.distance_meters(w[0], w[1]);
                        out.segment_displays.push((w[0], w[1], fmt_len(seg)));
                    }
                    if *closed && points.len() >= 3 {
                        let a = *points.last().unwrap();
                        let b = points[0];
                        let seg = scale.distance_meters(a, b);
                        out.segment_displays.push((a, b, fmt_len(seg)));
                    }
                }
            }
            MarkupKind::MeasureArea {
                points, cutouts, ..
            } => {
                let gross = scale.area_sq_meters(points);
                let cut: f64 = cutouts.iter().map(|c| scale.area_sq_meters(c)).sum();
                let net = (gross - cut).max(0.0);
                let perim = scale.polyline_length_meters(points, true);
                out.area_m2 = Some(net);
                out.cutout_area_m2 = Some(cut);
                out.perimeter_m = Some(perim);
                if let Some(bb) = Rect2::from_points(points) {
                    out.width_m = Some(bb.width() * scale.pt_to_meters_x());
                    out.height_m = Some(bb.height() * scale.pt_to_meters_y());
                }
                out.primary_display = Some(fmt_area(net));
                if opts.show_segments && points.len() >= 3 {
                    for i in 0..points.len() {
                        let a = points[i];
                        let b = points[(i + 1) % points.len()];
                        out.segment_displays
                            .push((a, b, fmt_len(scale.distance_meters(a, b))));
                    }
                }
            }
            MarkupKind::MeasureVolume {
                points, cutouts, ..
            } => {
                let gross = scale.area_sq_meters(points);
                let cut: f64 = cutouts.iter().map(|c| scale.area_sq_meters(c)).sum();
                let net = (gross - cut).max(0.0);
                let perim = scale.polyline_length_meters(points, true);
                let depth = opts.depth_meters.max(0.0);
                let vol = net * depth;
                let wall = perim * depth;
                out.area_m2 = Some(net);
                out.cutout_area_m2 = Some(cut);
                out.perimeter_m = Some(perim);
                out.volume_m3 = Some(vol);
                out.wall_area_m2 = Some(wall);
                out.primary_display = Some(format!("{} ({})", fmt_vol(vol), fmt_area(net)));
                if opts.show_segments && points.len() >= 3 {
                    for i in 0..points.len() {
                        let a = points[i];
                        let b = points[(i + 1) % points.len()];
                        out.segment_displays
                            .push((a, b, fmt_len(scale.distance_meters(a, b))));
                    }
                }
            }
            MarkupKind::MeasureAngle { p1, vertex, p2, .. } => {
                let deg = angle_three_points_deg(*p1, *vertex, *p2);
                out.angle_deg = Some(deg);
                out.primary_display = Some(format_angle(deg));
            }
            MarkupKind::MeasureRadius {
                center,
                edge,
                three_point,
                ..
            } => {
                let (c, e) = if let Some([a, b, c_pt]) = three_point {
                    if let Some((ctr, _)) = circle_from_three_points(*a, *b, *c_pt) {
                        (ctr, *a)
                    } else {
                        (*center, *edge)
                    }
                } else {
                    (*center, *edge)
                };
                let r_m = scale.distance_meters(c, e);
                out.radius_m = Some(r_m);
                out.length_m = Some(r_m);
                out.area_m2 = Some(std::f64::consts::PI * r_m * r_m);
                out.primary_display = Some(format!("R {}", fmt_len(r_m)));
            }
            MarkupKind::MeasureDiameter { p1, p2, .. } => {
                let d_m = scale.distance_meters(*p1, *p2);
                let r_m = d_m * 0.5;
                out.diameter_m = Some(d_m);
                out.radius_m = Some(r_m);
                out.length_m = Some(d_m);
                out.area_m2 = Some(std::f64::consts::PI * r_m * r_m);
                out.primary_display = Some(format!("Ø {}", fmt_len(d_m)));
            }
            MarkupKind::MeasureCount { points, .. } => {
                let n = points.len();
                out.count = Some(n);
                out.primary_display = Some(format!("{n} und"));
            }
            MarkupKind::SketchRect { rect, .. } => {
                let w_m = rect.width() * scale.pt_to_meters_x();
                let h_m = rect.height() * scale.pt_to_meters_y();
                let area = w_m * h_m;
                out.width_m = Some(w_m);
                out.height_m = Some(h_m);
                out.area_m2 = Some(area);
                out.perimeter_m = Some(2.0 * (w_m + h_m));
                out.primary_display = Some(format!("{} × {}", fmt_len(w_m), fmt_len(h_m)));
            }
            MarkupKind::SketchPoly { points, closed, .. } => {
                let perim = scale.polyline_length_meters(points, *closed);
                out.length_m = Some(perim);
                if *closed && points.len() >= 3 {
                    let area = scale.area_sq_meters(points);
                    out.area_m2 = Some(area);
                    out.perimeter_m = Some(perim);
                    out.primary_display = Some(fmt_area(area));
                } else {
                    out.primary_display = Some(fmt_len(perim));
                }
            }
            _ => {}
        }

        out
    }

    /// Computes total cost (`quantity_in_display_unit * unit_cost`) when `unit_cost` is assigned.
    pub fn total_cost(&self, page_scale: &PageScale, viewports: &[ViewportScale]) -> Option<f64> {
        let unit_cost = self.unit_cost?;
        let anchor = self.anchor_point();
        let scale = resolve_scale_at(page_scale, viewports, anchor);
        let opts = self.measurement_opts();
        let m = self.compute_metrics(page_scale, viewports);
        let qty = if let Some(v) = m.volume_m3 {
            let vu = opts.and_then(|o| o.volume_unit_override).unwrap_or(scale.volume_unit);
            vu.from_cu_meters(v)
        } else if let Some(a) = m.area_m2 {
            let au = opts.and_then(|o| o.area_unit_override).unwrap_or(scale.area_unit);
            au.from_sq_meters(a)
        } else if let Some(l) = m.length_m.or(m.perimeter_m) {
            let lu = opts.and_then(|o| o.length_unit_override).unwrap_or(scale.display_unit);
            lu.from_meters(l)
        } else if let Some(c) = m.count {
            c as f64
        } else {
            1.0
        };
        Some(qty * unit_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::units::PT_PER_MM;

    #[test]
    fn volume_with_cutout_computes_exact_net_area_and_cubic_meters() {
        let scale = PageScale::from_metric_ratio(100.0); // 10 mm/m -> 1m = 28.3464567 pt
        let ten_m_pt = 100.0 * PT_PER_MM; // 10 m on 1:100
        let four_m_pt = 40.0 * PT_PER_MM; // 4 m on 1:100
        let outer = Rect2::from_xywh(0.0, 0.0, ten_m_pt, ten_m_pt)
            .corners()
            .to_vec(); // 100 m²
        let hole = Rect2::from_xywh(20.0, 20.0, four_m_pt, four_m_pt)
            .corners()
            .to_vec(); // 16 m²

        let m = Markup::new(
            0,
            "Losa hormigón",
            "Matías",
            MarkupStyle::default(),
            MarkupKind::MeasureVolume {
                points: outer,
                cutouts: vec![hole],
                opts: MeasurementOptions {
                    depth_meters: 0.25,
                    ..Default::default()
                },
            },
        );
        let metrics = m.compute_metrics(&scale, &[]);
        assert!((metrics.area_m2.unwrap() - 84.0).abs() < 1e-4);
        assert!((metrics.volume_m3.unwrap() - 21.0).abs() < 1e-4);
        assert!((metrics.perimeter_m.unwrap() - 40.0).abs() < 1e-4);
    }
}
