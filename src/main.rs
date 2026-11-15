#![windows_subsystem = "windows"]

mod app;
mod camera;
mod config;
mod geometry;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("pip-view"),
        ..Default::default()
    };
    eframe::run_native(
        "pip-view",
        options,
        Box::new(|cc| Ok(Box::new(app::PipApp::new(cc)))),
    )
}
