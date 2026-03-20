#![warn(clippy::all, rust_2018_idioms)]

use clipstrike_ui::ui::ClipStrikeApp;
use eframe::egui;

pub fn run_native() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("ClipStrike (OpenShot Clone)"),
        ..Default::default()
    };

    eframe::run_native(
        "ClipStrike",
        options,
        Box::new(|cc| Ok(Box::new(ClipStrikeApp::new(cc)))),
    )
}
