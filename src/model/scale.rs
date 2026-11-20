use crate::geometry::math::{Point2, Rect2, polygon_area};
use crate::model::units::{
    AreaUnit, LengthUnit, PT_PER_METER, Precision, SlopeFormat, UnitSystem, VolumeUnit,
    format_area, format_length, format_volume,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetScaleDef {
    pub label: &'static str,
    pub paper_value: f64,
    pub paper_unit: LengthUnit,
    pub real_value: f64,
    pub real_unit: LengthUnit,
    pub default_display_unit: LengthUnit,
    pub is_metric: bool,
}

/// Chilean / ISO standard metric architectural & civil engineering scales first.
pub const METRIC_PRESETS: &[PresetScaleDef] = &[
    PresetScaleDef {
        label: "1:1",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 1.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Millimeter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:2",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 2.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Millimeter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:5",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 5.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Centimeter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:10",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 10.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:20",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 20.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:25",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 25.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:50",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 50.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:75",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 75.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:100",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 100.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:125",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 125.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:150",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 150.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:200",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 200.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:250",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 250.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:500",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 500.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:1000",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 1000.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:2000",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 2000.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
    PresetScaleDef {
        label: "1:5000",
        paper_value: 1.0,
        paper_unit: LengthUnit::Millimeter,
        real_value: 5000.0,
        real_unit: LengthUnit::Millimeter,
        default_display_unit: LengthUnit::Meter,
        is_metric: true,
    },
];

/// US Architectural & Engineering scales (matched to Bluebeam Revu's `Scale` fields).
pub const IMPERIAL_PRESETS: &[PresetScaleDef] = &[
    PresetScaleDef {
        label: "1/16\" = 1'-0\"",
        paper_value: 0.0625,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "3/32\" = 1'-0\"",
        paper_value: 0.09375,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1/8\" = 1'-0\"",
        paper_value: 0.125,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "3/16\" = 1'-0\"",
        paper_value: 0.1875,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1/4\" = 1'-0\"",
        paper_value: 0.25,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "3/8\" = 1'-0\"",
        paper_value: 0.375,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1/2\" = 1'-0\"",
        paper_value: 0.5,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "3/4\" = 1'-0\"",
        paper_value: 0.75,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 1'-0\"",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1 1/2\" = 1'-0\"",
        paper_value: 1.5,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "3\" = 1'-0\"",
        paper_value: 3.0,
        paper_unit: LengthUnit::Inch,
        real_value: 1.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::FootInch,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 10'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 10.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 20'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 20.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 30'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 30.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 40'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 40.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 50'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 50.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
    PresetScaleDef {
        label: "1\" = 100'",
        paper_value: 1.0,
        paper_unit: LengthUnit::Inch,
        real_value: 100.0,
        real_unit: LengthUnit::Foot,
        default_display_unit: LengthUnit::Foot,
        is_metric: false,
    },
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageScale {
    pub is_set: bool,
    pub paper_value: f64,
    pub paper_unit: LengthUnit,
    pub real_value: f64,
    pub real_unit: LengthUnit,
    pub separate_xy: bool,
    pub y_paper_value: f64,
    pub y_paper_unit: LengthUnit,
    pub y_real_value: f64,
    pub y_real_unit: LengthUnit,
    pub display_unit: LengthUnit,
    pub area_unit: AreaUnit,
    pub volume_unit: VolumeUnit,
    pub precision: Precision,
    pub slope_format: SlopeFormat,
}

impl Default for PageScale {
    /// Chilean default: uncalibrated 1:1 on paper, metric meters/m²/m³, 2 decimals, % slope.
    fn default() -> Self {
        Self {
            is_set: false,
            paper_value: 1.0,
            paper_unit: LengthUnit::Millimeter,
            real_value: 1.0,
            real_unit: LengthUnit::Millimeter,
            separate_xy: false,
            y_paper_value: 1.0,
            y_paper_unit: LengthUnit::Millimeter,
            y_real_value: 1.0,
            y_real_unit: LengthUnit::Millimeter,
            display_unit: LengthUnit::Meter,
            area_unit: AreaUnit::SqMeter,
            volume_unit: VolumeUnit::CuMeter,
            precision: Precision::Dec2,
            slope_format: SlopeFormat::Percent,
        }
    }
}

impl PageScale {
    pub fn from_metric_ratio(ratio: f64) -> Self {
        Self {
            is_set: true,
            paper_value: 1.0,
            paper_unit: LengthUnit::Millimeter,
            real_value: ratio.max(1e-6),
            real_unit: LengthUnit::Millimeter,
            y_paper_value: 1.0,
            y_paper_unit: LengthUnit::Millimeter,
            y_real_value: ratio.max(1e-6),
            y_real_unit: LengthUnit::Millimeter,
            display_unit: LengthUnit::Meter,
            area_unit: AreaUnit::SqMeter,
            volume_unit: VolumeUnit::CuMeter,
            precision: Precision::Dec2,
            slope_format: SlopeFormat::Percent,
            separate_xy: false,
        }
    }

    pub fn from_preset(preset: &PresetScaleDef) -> Self {
        let precision = if preset.is_metric {
            Precision::Dec2
        } else {
            Precision::Frac16
        };
        Self {
            is_set: true,
            paper_value: preset.paper_value,
            paper_unit: preset.paper_unit,
            real_value: preset.real_value,
            real_unit: preset.real_unit,
            separate_xy: false,
            y_paper_value: preset.paper_value,
            y_paper_unit: preset.paper_unit,
            y_real_value: preset.real_value,
            y_real_unit: preset.real_unit,
            display_unit: preset.default_display_unit,
            area_unit: preset.default_display_unit.default_area_unit(),
            volume_unit: preset.default_display_unit.default_volume_unit(),
            precision,
            slope_format: SlopeFormat::Percent,
        }
    }

    pub fn from_calibration(pdf_pts: f64, real_val: f64, real_unit: LengthUnit) -> Self {
        let paper_unit = if real_unit.is_metric() {
            LengthUnit::Millimeter
        } else {
            LengthUnit::Inch
        };
        let paper_val = (pdf_pts / paper_unit.points_per_unit()).max(1e-6);
        Self {
            is_set: true,
            paper_value: paper_val,
            paper_unit,
            real_value: real_val.max(1e-9),
            real_unit,
            separate_xy: false,
            y_paper_value: paper_val,
            y_paper_unit: paper_unit,
            y_real_value: real_val.max(1e-9),
            y_real_unit: real_unit,
            display_unit: real_unit,
            area_unit: real_unit.default_area_unit(),
            volume_unit: real_unit.default_volume_unit(),
            precision: if real_unit.is_metric() {
                Precision::Dec2
            } else {
                Precision::Frac16
            },
            slope_format: SlopeFormat::Percent,
        }
    }

    pub fn set_unit_system(&mut self, system: UnitSystem) {
        match system {
            UnitSystem::Metric => {
                self.display_unit = LengthUnit::Meter;
                self.area_unit = AreaUnit::SqMeter;
                self.volume_unit = VolumeUnit::CuMeter;
                if matches!(
                    self.precision,
                    Precision::Frac2
                        | Precision::Frac4
                        | Precision::Frac8
                        | Precision::Frac16
                        | Precision::Frac32
                ) {
                    self.precision = Precision::Dec2;
                }
            }
            UnitSystem::Imperial => {
                self.display_unit = LengthUnit::FootInch;
                self.area_unit = AreaUnit::SqFoot;
                self.volume_unit = VolumeUnit::CuFoot;
                self.precision = Precision::Frac16;
            }
        }
    }

    /// Real-world meters per 1 PDF point along X.
    pub fn pt_to_meters_x(&self) -> f64 {
        let paper_pts = (self.paper_value * self.paper_unit.points_per_unit()).max(1e-9);
        let real_m = self.real_unit.to_meters(self.real_value);
        real_m / paper_pts
    }

    /// Real-world meters per 1 PDF point along Y.
    pub fn pt_to_meters_y(&self) -> f64 {
        if !self.separate_xy {
            return self.pt_to_meters_x();
        }
        let paper_pts = (self.y_paper_value * self.y_paper_unit.points_per_unit()).max(1e-9);
        let real_m = self.y_real_unit.to_meters(self.y_real_value);
        real_m / paper_pts
    }

    /// Dimensionless ratio (real / paper) along X, e.g. 100.0 for 1:100.
    pub fn ratio_x(&self) -> f64 {
        let paper_m = self.paper_unit.to_meters(self.paper_value).max(1e-12);
        let real_m = self.real_unit.to_meters(self.real_value);
        real_m / paper_m
    }

    /// Dimensionless ratio (real / paper) along Y.
    pub fn ratio_y(&self) -> f64 {
        if !self.separate_xy {
            return self.ratio_x();
        }
        let paper_m = self.y_paper_unit.to_meters(self.y_paper_value).max(1e-12);
        let real_m = self.y_real_unit.to_meters(self.y_real_value);
        real_m / paper_m
    }

    /// Distance in real-world meters between two normalized page-point coordinates.
    pub fn distance_meters(&self, a: Point2, b: Point2) -> f64 {
        let dx_m = (b.x - a.x) * self.pt_to_meters_x();
        let dy_m = (b.y - a.y) * self.pt_to_meters_y();
        (dx_m * dx_m + dy_m * dy_m).sqrt()
    }

    /// Perimeter in real-world meters of a polyline or closed polygon.
    pub fn polyline_length_meters(&self, points: &[Point2], closed: bool) -> f64 {
        if points.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0;
        for w in points.windows(2) {
            total += self.distance_meters(w[0], w[1]);
        }
        if closed && points.len() >= 3 {
            total += self.distance_meters(*points.last().unwrap(), points[0]);
        }
        total
    }

    /// Area in real-world square meters of a polygon in PDF points.
    pub fn area_sq_meters(&self, points: &[Point2]) -> f64 {
        let area_pt2 = polygon_area(points);
        area_pt2 * self.pt_to_meters_x() * self.pt_to_meters_y()
    }

    /// Exact PDF `/Measure` `C` factor (real display units per 1 PDF point) for ISO 32000 export.
    pub fn pdf_measure_c_factor(&self) -> f64 {
        let meters_per_pt = self.pt_to_meters_x();
        self.display_unit.from_meters(meters_per_pt)
    }

    pub fn format_length_val(&self, meters: f64) -> String {
        format_length(meters, self.display_unit, self.precision)
    }

    pub fn format_area_val(&self, sq_meters: f64) -> String {
        format_area(sq_meters, self.area_unit, self.precision)
    }

    pub fn format_volume_val(&self, cu_meters: f64) -> String {
        format_volume(cu_meters, self.volume_unit, self.precision)
    }

    pub fn label(&self) -> String {
        if !self.is_set {
            return "Escala no establecida".to_string();
        }
        let ratio = self.ratio_x();
        if self.paper_unit.is_metric() && self.real_unit.is_metric() {
            if (ratio - ratio.round()).abs() < 0.05 {
                format!("1:{:.0} ({})", ratio.round(), self.display_unit.suffix())
            } else {
                format!("1:{:.1} ({})", ratio, self.display_unit.suffix())
            }
        } else {
            // Check matching imperial preset first
            for p in IMPERIAL_PRESETS {
                let p_ratio = p.real_unit.to_meters(p.real_value)
                    / p.paper_unit.to_meters(p.paper_value).max(1e-12);
                if (ratio - p_ratio).abs() / p_ratio.max(1.0) < 0.002 {
                    return format!("{} ({})", p.label, self.display_unit.suffix());
                }
            }
            format!(
                "{:.3} {} = {:.2} {}",
                self.paper_value,
                self.paper_unit.suffix(),
                self.real_value,
                self.real_unit.suffix()
            )
        }
    }

    pub fn meters_to_pt_avg(&self, meters: f64) -> f64 {
        let avg_m_per_pt = 0.5 * (self.pt_to_meters_x() + self.pt_to_meters_y());
        if avg_m_per_pt <= 1e-12 {
            meters * PT_PER_METER
        } else {
            meters / avg_m_per_pt
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportScale {
    pub id: Uuid,
    pub name: String,
    pub rect: Rect2,
    pub scale: PageScale,
}

impl ViewportScale {
    pub fn new(name: impl Into<String>, rect: Rect2, scale: PageScale) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            rect,
            scale,
        }
    }
}

/// Resolves the active scale at a given page coordinate: viewports take precedence over page scale.
pub fn resolve_scale_at<'a>(
    page_scale: &'a PageScale,
    viewports: &'a [ViewportScale],
    pt: Point2,
) -> &'a PageScale {
    // Later / smaller viewports win if overlapping.
    for vp in viewports.iter().rev() {
        if vp.rect.contains(pt) {
            return &vp.scale;
        }
    }
    page_scale
}

/// Parses 1-based page range strings like `"1, 3-5"` into deduplicated 0-based page indices.
pub fn parse_page_range(input: &str, total_pages: usize) -> Vec<usize> {
    if total_pages == 0 {
        return Vec::new();
    }
    let trimmed = input.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("todas")
        || trimmed.eq_ignore_ascii_case("all")
    {
        return (0..total_pages).collect();
    }
    let mut out = Vec::new();
    for part in trimmed.split([',', ';']) {
        let seg = part.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some((a_s, b_s)) = seg.split_once('-') {
            if let (Ok(a), Ok(b)) = (a_s.trim().parse::<usize>(), b_s.trim().parse::<usize>()) {
                let lo = a.min(b).max(1);
                let hi = a.max(b).min(total_pages);
                for p in lo..=hi {
                    if !out.contains(&(p - 1)) {
                        out.push(p - 1);
                    }
                }
            }
        } else if let Ok(p) = seg.parse::<usize>()
            && p >= 1
            && p <= total_pages
            && !out.contains(&(p - 1))
        {
            out.push(p - 1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::units::PT_PER_MM;

    #[test]
    fn metric_1_to_100_measures_59_5_meters_accurately() {
        let scale = PageScale::from_metric_ratio(100.0);
        // At 1:100, 59.50 m on paper is 595 mm.
        let pdf_len = 595.0 * PT_PER_MM;
        let measured = scale.distance_meters(Point2::new(0.0, 0.0), Point2::new(pdf_len, 0.0));
        assert!((measured - 59.5).abs() < 1e-6);
        assert_eq!(scale.format_length_val(measured), "59.50 m");
    }

    #[test]
    fn calibration_two_points_matches_known_length() {
        // Suppose user picks two points 283.464567 pt apart (= 100 mm on paper) and types 12.5 m
        let pts = 100.0 * PT_PER_MM;
        let cal = PageScale::from_calibration(pts, 12.5, LengthUnit::Meter);
        assert!((cal.ratio_x() - 125.0).abs() < 1e-4);
        let d = cal.distance_meters(Point2::new(10.0, 20.0), Point2::new(10.0 + pts, 20.0));
        assert!((d - 12.5).abs() < 1e-6);
    }

    #[test]
    fn viewport_overrides_page_scale_inside_region() {
        let page_scale = PageScale::from_metric_ratio(100.0);
        let vp = ViewportScale::new(
            "Detalle 1:20",
            Rect2::from_min_max(Point2::new(100.0, 100.0), Point2::new(300.0, 300.0)),
            PageScale::from_metric_ratio(20.0),
        );
        let s_out = resolve_scale_at(&page_scale, std::slice::from_ref(&vp), Point2::new(50.0, 50.0));
        let s_in = resolve_scale_at(&page_scale, std::slice::from_ref(&vp), Point2::new(150.0, 150.0));
        assert!((s_out.ratio_x() - 100.0).abs() < 1e-6);
        assert!((s_in.ratio_x() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn parses_page_ranges_cleanly() {
        assert_eq!(parse_page_range("1, 3-5, 3", 6), vec![0, 2, 3, 4]);
        assert_eq!(parse_page_range("todas", 3), vec![0, 1, 2]);
        assert_eq!(parse_page_range("5-2", 4), vec![1, 2, 3]);
    }
}
