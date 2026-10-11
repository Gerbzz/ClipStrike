#![warn(clippy::all, rust_2018_idioms)]

use clipstrike_ui::ui::ClipStrikeApp;
use eframe::egui;

pub fn run_native() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_hal::vulkan::instance=error,wgpu_hal::vulkan::adapter=error,wgpu_hal::vulkan::conv=error,wgpu_hal::auxil::dxgi::exception=error"))
        .format_timestamp_millis()
        .init();

    #[cfg(target_os = "windows")]
    if std::env::var("WGPU_BACKEND").is_err() {
        unsafe {
            std::env::set_var("WGPU_BACKEND", "dx12");
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("ClipStrike"),
        ..Default::default()
    };

    eframe::run_native(
        "ClipStrike",
        options,
        Box::new(|cc| Ok(Box::new(ClipStrikeApp::new(cc)))),
    )
}
