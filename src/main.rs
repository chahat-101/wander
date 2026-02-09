#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Hide console on release

mod app;
mod filesystem;

use app::ExplorerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to console

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("RustExplorer"),
        ..Default::default()
    };

    eframe::run_native(
        "RustExplorer",
        options,
        Box::new(|cc| Ok(Box::new(ExplorerApp::new(cc)))),
    )
}