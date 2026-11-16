#![windows_subsystem = "windows"]
// On Linux only the tests use the modules: the app runs only on Windows.
#![cfg_attr(not(windows), allow(dead_code))]
// TEMPORARY until Task 4 of the Win32 rewrite uses all modules.
#![cfg_attr(windows, allow(dead_code))]

mod border;
mod capture_logic;
mod config;
mod geometry;
mod log;
mod modes;
mod watchdog;
mod worker;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    println!("webcam-view-pip runs only on Windows.");
}
