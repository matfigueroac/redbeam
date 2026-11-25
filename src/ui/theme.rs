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
    pub text_on_accent: Color32,
    pub accent_cyan: Color32,
    pub accent_amber: Color32,
    pub accent_green: Color32,
}

impl RedBimTheme {
    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self {
                mode,
                window_bg: Color32::from_rgb(36, 30, 28),
                panel_bg: Color32::from_rgb(43, 36, 33),
                toolbar_bg: Color32::from_rgb(50, 41, 36),
                canvas_bg: Color32::from_rgb(29, 25, 23),
                card_bg: Color32::from_rgb(55, 45, 40),
                control_bg: Color32::from_rgb(64, 52, 45),
                control_hover: Color32::from_rgb(79, 63, 54),
                border: Color32::from_rgb(73, 59, 51),
                border_strong: Color32::from_rgb(121, 96, 80),
                text_primary: Color32::from_rgb(248, 237, 226),
                text_secondary: Color32::from_rgb(221, 199, 181),
                text_muted: Color32::from_rgb(181, 157, 141),
                accent_red: Color32::from_rgb(232, 151, 121),
                accent_red_soft: Color32::from_rgb(83, 53, 44),
                text_on_accent: Color32::from_rgb(45, 29, 23),
                accent_cyan: Color32::from_rgb(160, 198, 185),
                accent_amber: Color32::from_rgb(239, 185, 106),
                accent_green: Color32::from_rgb(163, 195, 141),
            },
            ThemeMode::Light => Self {
                mode,
                window_bg: Color32::from_rgb(239, 228, 216),
                panel_bg: Color32::from_rgb(249, 241, 231),
                toolbar_bg: Color32::from_rgb(246, 235, 223),
                canvas_bg: Color32::from_rgb(222, 204, 188),
                card_bg: Color32::from_rgb(244, 231, 218),
                control_bg: Color32::from_rgb(237, 221, 205),
                control_hover: Color32::from_rgb(232, 208, 187),
                border: Color32::from_rgb(215, 193, 174),
                border_strong: Color32::from_rgb(171, 139, 116),
                text_primary: Color32::from_rgb(48, 34, 28),
                text_secondary: Color32::from_rgb(91, 65, 51),
                text_muted: Color32::from_rgb(113, 84, 68),
                accent_red: Color32::from_rgb(150, 63, 39),
                accent_red_soft: Color32::from_rgb(241, 213, 197),
                text_on_accent: Color32::from_rgb(255, 247, 238),
                accent_cyan: Color32::from_rgb(41, 104, 88),
                accent_amber: Color32::from_rgb(133, 76, 23),
                accent_green: Color32::from_rgb(63, 105, 42),
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
        visuals.selection.bg_fill = self.accent_red_soft;
        visuals.selection.stroke = Stroke::new(1.0, self.text_primary);
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
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, self.text_primary);
        visuals.widgets.open = visuals.widgets.hovered;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.weak_bg_fill = widget.bg_fill;
        }
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
