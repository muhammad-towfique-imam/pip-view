#![windows_subsystem = "windows"]

mod app;
mod border;
mod camera;
mod config;
mod geometry;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("webcam-view-pip"),
        ..Default::default()
    };
    eframe::run_native(
        "webcam-view-pip",
        options,
        Box::new(|cc| Ok(Box::new(app::PipApp::new(cc)))),
    )
}
