#![windows_subsystem = "windows"]

mod app;
mod border;
mod camera;
mod config;
mod geometry;
mod log;
mod watchdog;

use log::log;

fn main() -> eframe::Result {
    let config_path = config::default_path();
    if let Some(path) = &config_path {
        log::init(&path.with_file_name("log.txt"));
    }
    std::panic::set_hook(Box::new(|info| log!("PANIC: {info}")));
    log!(
        "webcam-view-pip {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    watchdog::start();

    // "renderer": "glow" in config.json selects OpenGL in place of wgpu.
    let saved = config_path.as_deref().and_then(config::load);
    let glow = saved.as_ref().and_then(|c| c.renderer.as_deref()) == Some("glow");
    log!("renderer: {}", if glow { "glow (OpenGL)" } else { "wgpu" });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("webcam-view-pip"),
        renderer: if glow { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu },
        ..Default::default()
    };
    eframe::run_native(
        "webcam-view-pip",
        options,
        Box::new(|cc| Ok(Box::new(app::PipApp::new(cc)))),
    )
}
