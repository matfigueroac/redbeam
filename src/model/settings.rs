use crate::geometry::snap::SnapSettings;
use crate::model::units::{AreaUnit, LengthUnit, Precision, SheetDisplayUnit, SlopeFormat, UnitSystem, VolumeUnit};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CsvDelimiter {
    #[default]
    Semicolon, // Standard Excel delimiter in Chile / es-CL
    Comma,
}

impl CsvDelimiter {
    pub fn as_char(self) -> char {
        match self {
            Self::Semicolon => ';',
            Self::Comma => ',',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Semicolon => "Punto y coma (;) — Excel Chile",
            Self::Comma => "Coma (,) — Estándar internacional",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub author_name: String,
    pub default_unit_system: UnitSystem,
    pub default_length_unit: LengthUnit,
    pub default_area_unit: AreaUnit,
    pub default_volume_unit: VolumeUnit,
    pub default_precision: Precision,
    pub default_slope_format: SlopeFormat,
    pub sheet_display_unit: SheetDisplayUnit,
    pub csv_delimiter: CsvDelimiter,
    pub snap: SnapSettings,
    pub show_grid: bool,
    pub grid_spacing_meters: f64,
    pub reuse_tool: bool,
    pub dimmer_enabled: bool,
    pub dimmer_opacity: f32,
    pub recent_files: Vec<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "Revisor Chile".to_string());
        Self {
            theme: ThemeMode::Dark,
            author_name: user,
            default_unit_system: UnitSystem::Metric,
            default_length_unit: LengthUnit::Meter,
            default_area_unit: AreaUnit::SqMeter,
            default_volume_unit: VolumeUnit::CuMeter,
            default_precision: Precision::Dec2,
            default_slope_format: SlopeFormat::Percent,
            sheet_display_unit: SheetDisplayUnit::Millimeters,
            csv_delimiter: CsvDelimiter::Semicolon,
            snap: SnapSettings::default(),
            show_grid: false,
            grid_spacing_meters: 1.0,
            reuse_tool: true,
            dimmer_enabled: false,
            dimmer_opacity: 0.45,
            recent_files: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn storage_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("cl", "RedBIM", "RedBIM")
            .map(|d| d.config_dir().join("settings.json"))
    }

    pub fn load_or_default() -> Self {
        if let Some(path) = Self::storage_path()
            && let Ok(raw) = std::fs::read_to_string(&path)
            && let Ok(parsed) = serde_json::from_str::<Self>(&raw)
        {
            return parsed;
        }
        Self::default()
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

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(12);
        self.save();
    }
}
