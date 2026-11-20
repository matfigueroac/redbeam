use crate::model::settings::ThemeMode;
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Visuals};

#[derive(Debug, Clone, Copy)]
pub struct RedBimTheme {
    pub mode: ThemeMode,
    pub window_bg: Color32,
    pub panel_bg: Color32,
    pub toolbar_bg: Color32,
    pub canvas_bg: Color32,
    pub card_bg: Color32,
    pub control_bg: Color32,
    pub control_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent_red: Color32,
    pub accent_red_soft: Color32,
    pub accent_cyan: Color32,
    pub accent_amber: Color32,
    pub accent_green: Color32,
}

impl RedBimTheme {
    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self {
                mode,
                window_bg: Color32::from_rgb(20, 23, 29),
                panel_bg: Color32::from_rgb(27, 31, 39),
                toolbar_bg: Color32::from_rgb(31, 36, 46),
                canvas_bg: Color32::from_rgb(16, 18, 23),
                card_bg: Color32::from_rgb(35, 41, 52),
                control_bg: Color32::from_rgb(42, 48, 61),
                control_hover: Color32::from_rgb(56, 64, 80),
                border: Color32::from_rgb(50, 58, 74),
                border_strong: Color32::from_rgb(72, 83, 104),
                text_primary: Color32::from_rgb(241, 245, 249),
                text_secondary: Color32::from_rgb(203, 213, 225),
                text_muted: Color32::from_rgb(148, 163, 184),
                accent_red: Color32::from_rgb(229, 57, 53),
                accent_red_soft: Color32::from_rgba_unmultiplied(229, 57, 53, 46),
                accent_cyan: Color32::from_rgb(56, 189, 248),
                accent_amber: Color32::from_rgb(245, 158, 11),
                accent_green: Color32::from_rgb(34, 197, 94),
            },
            ThemeMode::Light => Self {
                mode,
                window_bg: Color32::from_rgb(241, 244, 249),
                panel_bg: Color32::from_rgb(248, 250, 252),
                toolbar_bg: Color32::from_rgb(255, 255, 255),
                canvas_bg: Color32::from_rgb(218, 224, 232),
                card_bg: Color32::from_rgb(255, 255, 255),
                control_bg: Color32::from_rgb(236, 240, 246),
                control_hover: Color32::from_rgb(222, 228, 237),
                border: Color32::from_rgb(209, 217, 228),
                border_strong: Color32::from_rgb(165, 177, 194),
                text_primary: Color32::from_rgb(15, 23, 42),
                text_secondary: Color32::from_rgb(51, 65, 85),
                text_muted: Color32::from_rgb(100, 116, 139),
                accent_red: Color32::from_rgb(211, 47, 47),
                accent_red_soft: Color32::from_rgba_unmultiplied(211, 47, 47, 32),
                accent_cyan: Color32::from_rgb(2, 132, 199),
                accent_amber: Color32::from_rgb(217, 119, 6),
                accent_green: Color32::from_rgb(22, 163, 74),
            },
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = match self.mode {
            ThemeMode::Dark => Visuals::dark(),
            ThemeMode::Light => Visuals::light(),
        };
        visuals.panel_fill = self.panel_bg;
        visuals.window_fill = self.panel_bg;
        visuals.extreme_bg_color = self.canvas_bg;
        visuals.faint_bg_color = self.card_bg;
        visuals.selection.bg_fill = self.accent_red;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.bg_fill = self.control_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.hovered.bg_fill = self.control_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, self.border_strong);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, self.text_primary);
        visuals.widgets.active.bg_fill = self.accent_red_soft;
        visuals.widgets.active.bg_stroke = Stroke::new(1.2, self.accent_red);
        visuals.window_corner_radius = CornerRadius::same(6);
        visuals.menu_corner_radius = CornerRadius::same(6);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(4);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(4);
        visuals.widgets.active.corner_radius = CornerRadius::same(4);

        let egui_theme = match self.mode {
            ThemeMode::Dark => egui::Theme::Dark,
            ThemeMode::Light => egui::Theme::Light,
        };
        ctx.set_theme(egui_theme);
        ctx.set_visuals_of(egui_theme, visuals);

        ctx.style_mut_of(egui_theme, |style| {
            style.spacing.item_spacing = egui::vec2(6.0, 5.0);
            style.spacing.button_padding = egui::vec2(7.0, 4.0);
            style.spacing.window_margin = Margin::same(10);
            style.spacing.indent = 14.0;
        });
    }
}
