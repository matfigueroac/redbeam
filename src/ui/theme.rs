use crate::model::settings::ThemeMode;
use eframe::egui::{self, Color32, CornerRadius, Margin, Shadow, Stroke, Visuals};

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
                window_bg: Color32::from_rgb(19, 22, 29),
                panel_bg: Color32::from_rgb(25, 29, 38),
                toolbar_bg: Color32::from_rgb(30, 35, 46),
                canvas_bg: Color32::from_rgb(14, 17, 23),
                card_bg: Color32::from_rgb(33, 39, 51),
                control_bg: Color32::from_rgb(40, 47, 61),
                control_hover: Color32::from_rgb(54, 63, 82),
                border: Color32::from_rgb(47, 55, 72),
                border_strong: Color32::from_rgb(76, 89, 114),
                text_primary: Color32::from_rgb(242, 246, 252),
                text_secondary: Color32::from_rgb(200, 210, 224),
                text_muted: Color32::from_rgb(142, 156, 178),
                accent_red: Color32::from_rgb(232, 56, 62),
                accent_red_soft: Color32::from_rgba_unmultiplied(232, 56, 62, 42),
                accent_cyan: Color32::from_rgb(56, 189, 248),
                accent_amber: Color32::from_rgb(245, 158, 11),
                accent_green: Color32::from_rgb(34, 197, 94),
            },
            ThemeMode::Light => Self {
                mode,
                window_bg: Color32::from_rgb(240, 244, 249),
                panel_bg: Color32::from_rgb(249, 250, 252),
                toolbar_bg: Color32::from_rgb(255, 255, 255),
                canvas_bg: Color32::from_rgb(220, 226, 235),
                card_bg: Color32::from_rgb(255, 255, 255),
                control_bg: Color32::from_rgb(236, 241, 247),
                control_hover: Color32::from_rgb(223, 230, 240),
                border: Color32::from_rgb(211, 220, 232),
                border_strong: Color32::from_rgb(163, 176, 196),
                text_primary: Color32::from_rgb(15, 23, 42),
                text_secondary: Color32::from_rgb(51, 65, 85),
                text_muted: Color32::from_rgb(100, 116, 139),
                accent_red: Color32::from_rgb(214, 42, 48),
                accent_red_soft: Color32::from_rgba_unmultiplied(214, 42, 48, 30),
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
        visuals.window_corner_radius = CornerRadius::same(8);
        visuals.menu_corner_radius = CornerRadius::same(7);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(5);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(5);
        visuals.widgets.active.corner_radius = CornerRadius::same(5);
        visuals.window_shadow = Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(if self.mode == ThemeMode::Dark { 110 } else { 38 }),
        };
        visuals.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if self.mode == ThemeMode::Dark { 95 } else { 32 }),
        };

        let egui_theme = match self.mode {
            ThemeMode::Dark => egui::Theme::Dark,
            ThemeMode::Light => egui::Theme::Light,
        };
        ctx.set_theme(egui_theme);
        ctx.set_visuals_of(egui_theme, visuals);

        ctx.style_mut_of(egui_theme, |style| {
            style.animation_time = 0.12;
            style.spacing.item_spacing = egui::vec2(6.0, 5.0);
            style.spacing.button_padding = egui::vec2(8.0, 4.0);
            style.spacing.window_margin = Margin::same(11);
            style.spacing.indent = 14.0;
        });
    }
}
