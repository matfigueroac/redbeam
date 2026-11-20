use serde::{Deserialize, Serialize};
use std::fmt;

/// Standard PDF points per inch (ISO 32000-1).
pub const PT_PER_INCH: f64 = 72.0;
/// Standard PDF points per millimeter.
pub const PT_PER_MM: f64 = 72.0 / 25.4;
/// Standard PDF points per centimeter.
pub const PT_PER_CM: f64 = 720.0 / 25.4;
/// Standard PDF points per meter.
pub const PT_PER_METER: f64 = 72_000.0 / 25.4;

pub const METERS_PER_INCH: f64 = 0.0254;
pub const METERS_PER_FOOT: f64 = 0.3048;
pub const METERS_PER_YARD: f64 = 0.9144;
pub const SQ_METERS_PER_ACRE: f64 = 4046.856_422_4;
pub const SQ_METERS_PER_HECTARE: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum UnitSystem {
    #[default]
    Metric,
    Imperial,
}

impl UnitSystem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Metric => "Métrico (Chile / SI)",
            Self::Imperial => "Imperial (EE.UU. / ft-in)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    #[default]
    Meter,
    Kilometer,
    Inch,
    Foot,
    FootInch,
    Yard,
}

impl LengthUnit {
    pub const ALL: &'static [Self] = &[
        Self::Meter,
        Self::Centimeter,
        Self::Millimeter,
        Self::Kilometer,
        Self::Foot,
        Self::Inch,
        Self::FootInch,
        Self::Yard,
    ];

    pub const METRIC: &'static [Self] = &[
        Self::Meter,
        Self::Centimeter,
        Self::Millimeter,
        Self::Kilometer,
    ];

    pub const IMPERIAL: &'static [Self] = &[Self::Foot, Self::Inch, Self::FootInch, Self::Yard];

    pub fn is_metric(self) -> bool {
        matches!(
            self,
            Self::Millimeter | Self::Centimeter | Self::Meter | Self::Kilometer
        )
    }

    pub fn meters_per_unit(self) -> f64 {
        match self {
            Self::Millimeter => 0.001,
            Self::Centimeter => 0.01,
            Self::Meter => 1.0,
            Self::Kilometer => 1000.0,
            Self::Inch => METERS_PER_INCH,
            Self::Foot | Self::FootInch => METERS_PER_FOOT,
            Self::Yard => METERS_PER_YARD,
        }
    }

    pub fn points_per_unit(self) -> f64 {
        self.meters_per_unit() * PT_PER_METER
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Centimeter => "cm",
            Self::Meter => "m",
            Self::Kilometer => "km",
            Self::Inch => "in",
            Self::Foot => "ft",
            Self::FootInch => "ft-in",
            Self::Yard => "yd",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Millimeter => "Milímetros (mm)",
            Self::Centimeter => "Centímetros (cm)",
            Self::Meter => "Metros (m)",
            Self::Kilometer => "Kilómetros (km)",
            Self::Inch => "Pulgadas (in)",
            Self::Foot => "Pies (ft)",
            Self::FootInch => "Pies y pulgadas (ft'-in\")",
            Self::Yard => "Yardas (yd)",
        }
    }

    pub fn from_meters(self, meters: f64) -> f64 {
        meters / self.meters_per_unit()
    }

    pub fn to_meters(self, value: f64) -> f64 {
        value * self.meters_per_unit()
    }

    /// Best matching area unit for a length unit.
    pub fn default_area_unit(self) -> AreaUnit {
        match self {
            Self::Millimeter => AreaUnit::SqMillimeter,
            Self::Centimeter => AreaUnit::SqCentimeter,
            Self::Meter | Self::Kilometer => AreaUnit::SqMeter,
            Self::Inch => AreaUnit::SqInch,
            Self::Foot | Self::FootInch => AreaUnit::SqFoot,
            Self::Yard => AreaUnit::SqYard,
        }
    }

    /// Best matching volume unit for a length unit.
    pub fn default_volume_unit(self) -> VolumeUnit {
        match self {
            Self::Millimeter => VolumeUnit::CuMillimeter,
            Self::Centimeter => VolumeUnit::CuCentimeter,
            Self::Meter | Self::Kilometer => VolumeUnit::CuMeter,
            Self::Inch => VolumeUnit::CuInch,
            Self::Foot | Self::FootInch => VolumeUnit::CuFoot,
            Self::Yard => VolumeUnit::CuYard,
        }
    }
}

impl fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.suffix())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AreaUnit {
    SqMillimeter,
    SqCentimeter,
    #[default]
    SqMeter,
    Hectare,
    SqInch,
    SqFoot,
    SqYard,
    Acre,
}

impl AreaUnit {
    pub const ALL: &'static [Self] = &[
        Self::SqMeter,
        Self::SqCentimeter,
        Self::SqMillimeter,
        Self::Hectare,
        Self::SqFoot,
        Self::SqInch,
        Self::SqYard,
        Self::Acre,
    ];

    pub fn sq_meters_per_unit(self) -> f64 {
        match self {
            Self::SqMillimeter => 1e-6,
            Self::SqCentimeter => 1e-4,
            Self::SqMeter => 1.0,
            Self::Hectare => SQ_METERS_PER_HECTARE,
            Self::SqInch => METERS_PER_INCH * METERS_PER_INCH,
            Self::SqFoot => METERS_PER_FOOT * METERS_PER_FOOT,
            Self::SqYard => METERS_PER_YARD * METERS_PER_YARD,
            Self::Acre => SQ_METERS_PER_ACRE,
        }
    }

    pub fn from_sq_meters(self, sqm: f64) -> f64 {
        sqm / self.sq_meters_per_unit()
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::SqMillimeter => "mm²",
            Self::SqCentimeter => "cm²",
            Self::SqMeter => "m²",
            Self::Hectare => "ha",
            Self::SqInch => "in²",
            Self::SqFoot => "ft²",
            Self::SqYard => "yd²",
            Self::Acre => "acres",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::SqMillimeter => "Milímetros cuadrados (mm²)",
            Self::SqCentimeter => "Centímetros cuadrados (cm²)",
            Self::SqMeter => "Metros cuadrados (m²)",
            Self::Hectare => "Hectáreas (ha)",
            Self::SqInch => "Pulgadas cuadradas (in²)",
            Self::SqFoot => "Pies cuadrados (ft²)",
            Self::SqYard => "Yardas cuadradas (yd²)",
            Self::Acre => "Acres",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum VolumeUnit {
    CuMillimeter,
    CuCentimeter,
    #[default]
    CuMeter,
    Liter,
    CuInch,
    CuFoot,
    CuYard,
}

impl VolumeUnit {
    pub const ALL: &'static [Self] = &[
        Self::CuMeter,
        Self::Liter,
        Self::CuCentimeter,
        Self::CuMillimeter,
        Self::CuFoot,
        Self::CuInch,
        Self::CuYard,
    ];

    pub fn cu_meters_per_unit(self) -> f64 {
        match self {
            Self::CuMillimeter => 1e-9,
            Self::CuCentimeter => 1e-6,
            Self::CuMeter => 1.0,
            Self::Liter => 0.001,
            Self::CuInch => METERS_PER_INCH.powi(3),
            Self::CuFoot => METERS_PER_FOOT.powi(3),
            Self::CuYard => METERS_PER_YARD.powi(3),
        }
    }

    pub fn from_cu_meters(self, cum: f64) -> f64 {
        cum / self.cu_meters_per_unit()
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::CuMillimeter => "mm³",
            Self::CuCentimeter => "cm³",
            Self::CuMeter => "m³",
            Self::Liter => "L",
            Self::CuInch => "in³",
            Self::CuFoot => "ft³",
            Self::CuYard => "yd³",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::CuMillimeter => "Milímetros cúbicos (mm³)",
            Self::CuCentimeter => "Centímetros cúbicos (cm³)",
            Self::CuMeter => "Metros cúbicos (m³)",
            Self::Liter => "Litros (L)",
            Self::CuInch => "Pulgadas cúbicas (in³)",
            Self::CuFoot => "Pies cúbicos (ft³)",
            Self::CuYard => "Yardas cúbicas (yd³)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SlopeFormat {
    #[default]
    Percent,
    Degrees,
    Pitch12,
}

impl SlopeFormat {
    pub const ALL: &'static [Self] = &[Self::Percent, Self::Degrees, Self::Pitch12];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Percent => "Porcentaje (%)",
            Self::Degrees => "Grados (°)",
            Self::Pitch12 => "Pendiente (x:12)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Precision {
    Dec0,
    Dec1,
    #[default]
    Dec2,
    Dec3,
    Dec4,
    Frac2,
    Frac4,
    Frac8,
    Frac16,
    Frac32,
}

impl Precision {
    pub const ALL: &'static [Self] = &[
        Self::Dec0,
        Self::Dec1,
        Self::Dec2,
        Self::Dec3,
        Self::Dec4,
        Self::Frac2,
        Self::Frac4,
        Self::Frac8,
        Self::Frac16,
        Self::Frac32,
    ];

    pub fn decimals(self) -> usize {
        match self {
            Self::Dec0 => 0,
            Self::Dec1 => 1,
            Self::Dec2 => 2,
            Self::Dec3 => 3,
            Self::Dec4 => 4,
            Self::Frac2 | Self::Frac4 | Self::Frac8 | Self::Frac16 | Self::Frac32 => 2,
        }
    }

    pub fn denominator(self) -> Option<u32> {
        match self {
            Self::Frac2 => Some(2),
            Self::Frac4 => Some(4),
            Self::Frac8 => Some(8),
            Self::Frac16 => Some(16),
            Self::Frac32 => Some(32),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dec0 => "0 (entero)",
            Self::Dec1 => "0.1 (1 dec.)",
            Self::Dec2 => "0.01 (2 dec.)",
            Self::Dec3 => "0.001 (3 dec.)",
            Self::Dec4 => "0.0001 (4 dec.)",
            Self::Frac2 => "1/2",
            Self::Frac4 => "1/4",
            Self::Frac8 => "1/8",
            Self::Frac16 => "1/16",
            Self::Frac32 => "1/32",
        }
    }
}

pub fn format_number_dec(val: f64, decimals: usize) -> String {
    if !val.is_finite() {
        return "0".to_string();
    }
    let clean = if val.abs() < 1e-9 { 0.0 } else { val };
    format!("{clean:.decimals$}")
}

/// Formats an inch value as `whole num/den"` (e.g. `6 1/2"`).
pub fn format_fractional_inches(inches: f64, denom: u32) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let abs_in = inches.abs();
    let total_ticks = (abs_in * denom as f64).round() as u64;
    let whole = total_ticks / denom as u64;
    let rem = (total_ticks % denom as u64) as u32;
    if rem == 0 {
        format!("{sign}{whole}\"")
    } else {
        let g = gcd(rem, denom);
        let n = rem / g;
        let d = denom / g;
        if whole == 0 {
            format!("{sign}{n}/{d}\"")
        } else {
            format!("{sign}{whole} {n}/{d}\"")
        }
    }
}

/// Formats a feet value in American architectural style `12'-6 1/2"`.
pub fn format_feet_inches(feet_total: f64, precision: Precision) -> String {
    let sign = if feet_total < 0.0 { "-" } else { "" };
    let total_inches = feet_total.abs() * 12.0;
    let denom = precision.denominator().unwrap_or(16);
    let total_ticks = (total_inches * denom as f64).round() as u64;
    let ticks_per_foot = 12 * denom as u64;
    let feet = total_ticks / ticks_per_foot;
    let rem_ticks = total_ticks % ticks_per_foot;
    let whole_in = rem_ticks / denom as u64;
    let frac_ticks = (rem_ticks % denom as u64) as u32;

    if frac_ticks == 0 {
        format!("{sign}{feet}'-{whole_in}\"")
    } else {
        let g = gcd(frac_ticks, denom);
        let n = frac_ticks / g;
        let d = denom / g;
        format!("{sign}{feet}'-{whole_in} {n}/{d}\"")
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

pub fn format_length(meters: f64, unit: LengthUnit, precision: Precision) -> String {
    match unit {
        LengthUnit::FootInch => format_feet_inches(meters / METERS_PER_FOOT, precision),
        LengthUnit::Inch => {
            let inches = meters / METERS_PER_INCH;
            if let Some(denom) = precision.denominator() {
                format_fractional_inches(inches, denom)
            } else {
                format!("{} in", format_number_dec(inches, precision.decimals()))
            }
        }
        _ => {
            let v = unit.from_meters(meters);
            format!(
                "{} {}",
                format_number_dec(v, precision.decimals()),
                unit.suffix()
            )
        }
    }
}

pub fn format_area(sq_meters: f64, unit: AreaUnit, precision: Precision) -> String {
    let v = unit.from_sq_meters(sq_meters);
    format!(
        "{} {}",
        format_number_dec(v, precision.decimals()),
        unit.suffix()
    )
}

pub fn format_volume(cu_meters: f64, unit: VolumeUnit, precision: Precision) -> String {
    let v = unit.from_cu_meters(cu_meters);
    format!(
        "{} {}",
        format_number_dec(v, precision.decimals()),
        unit.suffix()
    )
}

pub fn format_angle(degrees: f64) -> String {
    format!("{degrees:.1}°")
}

pub fn format_slope(rise_over_run: f64, format_kind: SlopeFormat) -> String {
    if !rise_over_run.is_finite() {
        return "0.0%".to_string();
    }
    match format_kind {
        SlopeFormat::Percent => format!("{:.1}%", rise_over_run * 100.0),
        SlopeFormat::Degrees => format!("{:.1}°", rise_over_run.atan().to_degrees()),
        SlopeFormat::Pitch12 => format!("{:.2}:12", rise_over_run * 12.0),
    }
}

/// Identifies standard ISO / Chilean & US paper formats from PDF dimensions in points.
pub fn detect_paper_format(width_pt: f64, height_pt: f64) -> Option<&'static str> {
    let w_mm = width_pt / PT_PER_MM;
    let h_mm = height_pt / PT_PER_MM;
    let (short_mm, long_mm) = if w_mm <= h_mm {
        (w_mm, h_mm)
    } else {
        (h_mm, w_mm)
    };

    const FORMATS: &[(&str, f64, f64)] = &[
        ("A0", 841.0, 1189.0),
        ("A1", 594.0, 841.0),
        ("A2", 420.0, 594.0),
        ("A3", 297.0, 420.0),
        ("A4", 210.0, 297.0),
        ("Carta", 215.9, 279.4),
        ("Oficio", 215.9, 330.2),
        ("Tabloide", 279.4, 431.8),
        ("Arch C", 457.2, 609.6),
        ("Arch D", 609.6, 914.4),
        ("Arch E", 914.4, 1219.2),
    ];

    for &(name, sw, lw) in FORMATS {
        if (short_mm - sw).abs() <= 4.0 && (long_mm - lw).abs() <= 4.0 {
            return Some(name);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SheetDisplayUnit {
    #[default]
    Millimeters,
    Centimeters,
    Inches,
}

impl SheetDisplayUnit {
    pub fn next(self) -> Self {
        match self {
            Self::Millimeters => Self::Centimeters,
            Self::Centimeters => Self::Inches,
            Self::Inches => Self::Millimeters,
        }
    }
}

/// Formats page physical dimensions, defaulting to Chilean mm + ISO label (e.g. `841 × 594 mm (A1)`).
pub fn format_page_dimensions(width_pt: f64, height_pt: f64, unit: SheetDisplayUnit) -> String {
    let tag = detect_paper_format(width_pt, height_pt)
        .map(|f| format!(" ({f})"))
        .unwrap_or_default();
    match unit {
        SheetDisplayUnit::Millimeters => {
            let w = width_pt / PT_PER_MM;
            let h = height_pt / PT_PER_MM;
            format!("{w:.0} × {h:.0} mm{tag}")
        }
        SheetDisplayUnit::Centimeters => {
            let w = width_pt / PT_PER_CM;
            let h = height_pt / PT_PER_CM;
            format!("{w:.1} × {h:.1} cm{tag}")
        }
        SheetDisplayUnit::Inches => {
            let w = width_pt / PT_PER_INCH;
            let h = height_pt / PT_PER_INCH;
            format!("{w:.2} × {h:.2} in{tag}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_conversions_are_exact() {
        // 28.34645669 pt = 10 mm = 1 cm
        let ten_mm_pts = 10.0 * PT_PER_MM;
        let meters = ten_mm_pts / LengthUnit::Meter.points_per_unit();
        assert!((meters - 0.01).abs() < 1e-9);
    }

    #[test]
    fn fractional_feet_inches_formatting() {
        // 12.5416667 ft = 12'-6 1/2"
        let ft = 12.0 + 6.5 / 12.0;
        assert_eq!(format_feet_inches(ft, Precision::Frac16), "12'-6 1/2\"");
        assert_eq!(format_fractional_inches(6.25, 8), "6 1/4\"");
    }

    #[test]
    fn detects_iso_a1_and_formats_chile_mm() {
        let w_pt = 841.0 * PT_PER_MM;
        let h_pt = 594.0 * PT_PER_MM;
        assert_eq!(detect_paper_format(w_pt, h_pt), Some("A1"));
        assert_eq!(
            format_page_dimensions(w_pt, h_pt, SheetDisplayUnit::Millimeters),
            "841 × 594 mm (A1)"
        );
    }
}
