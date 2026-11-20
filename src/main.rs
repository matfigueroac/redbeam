#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

struct RedBimBootstrap;

impl eframe::App for RedBimBootstrap {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("RedBIM Bootstrap");
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RedBIM",
        options,
        Box::new(|_cc| Ok(Box::new(RedBimBootstrap))),
    )
}
