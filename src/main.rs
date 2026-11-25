#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use redbim::app::{LaunchConfig, RedBimApp};
use std::path::PathBuf;
use std::sync::Arc;

fn parse_args() -> LaunchConfig {
    let mut cfg = LaunchConfig {
        files: Vec::new(),
        demo: false,
        light_theme: false,
        open_measurements_tab: false,
        screenshot_out: None,
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => cfg.demo = true,
            "--light" => cfg.light_theme = true,
            "--measurements" => cfg.open_measurements_tab = true,
            "--screenshot" => {
                if let Some(p) = args.next() {
                    cfg.screenshot_out = Some(PathBuf::from(p));
                }
            }
            other if !other.starts_with('-') => {
                cfg.files.push(PathBuf::from(other));
            }
            _ => {}
        }
    }
    cfg
}

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "--export-demo") {
        let _ = args.next();
        if let Some(out) = args.next() {
            let bytes = redbim::pdf::demo_blueprint::generate_demo_chile_pdf();
            if let Some(parent) = std::path::Path::new(&out).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&out, bytes);
        }
        return Ok(());
    }
    let launch = parse_args();
    let viewport = egui::ViewportBuilder::default()
        .with_title("RedBeam — Visor y Cubicación de Planos PDF (Chile / Métrico)")
        .with_icon(Arc::new(redbim::ui::icons::generate_app_icon()))
        .with_inner_size([1440.0, 880.0])
        .with_maximized(true)
        .with_min_inner_size([960.0, 620.0]);
    #[cfg(target_os = "windows")]
    let viewport = viewport.with_decorations(false);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "RedBeam",
        options,
        Box::new(move |cc| Ok(Box::new(RedBimApp::new(cc, launch)))),
    )
}
