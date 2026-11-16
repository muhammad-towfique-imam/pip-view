//! The Windows app: Media Foundation camera and Win32 windows.

pub mod app;
pub mod capture;
pub mod draw;
pub mod mf;
pub mod pip;
pub mod setup;

use windows::core::w;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config;
use crate::log::{self, log};
use crate::watchdog;

pub fn run() {
    let config_path = config::default_path();
    if let Some(path) = &config_path {
        log::init(&path.with_file_name("log.txt"));
    }
    std::panic::set_hook(Box::new(|info| log!("PANIC: {info}")));
    log!("webcam-view-pip {} on Windows ({})", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH);
    watchdog::start();

    if let Err(e) = mf::startup() {
        log!("Media Foundation does not start: {e:#}");
        // SAFETY: a message box with static text.
        unsafe {
            MessageBoxW(
                None,
                w!("Windows Media Foundation does not start. On Windows N editions, install the Media Feature Pack."),
                w!("webcam-view-pip"),
                MB_OK | MB_ICONERROR,
            );
        }
        return;
    }

    let saved = config_path.as_deref().and_then(config::load);
    log!("saved config: {}", if saved.is_some() { "yes" } else { "no" });
    app::install(app::App::new(config_path, saved));

    // SAFETY: the module handle of this exe.
    let hinst = unsafe { GetModuleHandleW(None) }.map(Into::into).unwrap_or_default();
    setup::register(hinst);
    if setup::create(hinst).is_none() {
        log!("cannot create the setup window");
        return;
    }
    let ops = app::with_app(|a| a.open_camera_page(None)).unwrap_or_default();
    app::apply(ops);

    let mut msg = MSG::default();
    // SAFETY: the standard message loop of this thread.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let setup = app::with_app(|a| a.setup).unwrap_or_default();
            if !IsDialogMessageW(setup, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    log!("exit");
}
