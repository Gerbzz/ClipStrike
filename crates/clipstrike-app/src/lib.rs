#![warn(clippy::all, rust_2018_idioms)]

use clipstrike_ui::ui::ClipStrikeApp;
#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> eframe::Result {
    env_logger::init();

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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let canvas = eframe::web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("the_canvas_id"))
        .ok_or_else(|| JsValue::from_str("failed to find #the_canvas_id"))?
        .dyn_into::<eframe::web_sys::HtmlCanvasElement>()?;

    wasm_bindgen_futures::spawn_local(async move {
        let web_options = eframe::WebOptions::default();
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(ClipStrikeApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}
