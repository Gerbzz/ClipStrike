//! Import mode (header "Import"): large browser cards for importing media and demo footage.

use egui::{Align2, Color32, Rect, Sense, vec2};
use serde_json::json;

use crate::FilmcraftApp;
use crate::theme::Tokens;

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    ui.painter().rect_filled(rect, t.radius, t.panel_bg);
    ui.painter().text(rect.min + vec2(24.0, 30.0), Align2::LEFT_CENTER, tl!("Import"), Tokens::semibold(22.0), t.text);
    ui.painter().text(
        rect.min + vec2(24.0, 58.0),
        Align2::LEFT_CENTER,
        tl!("Add media to your project. Drop files anywhere in the window, or choose a source below."),
        Tokens::ui(13.0),
        t.text_dim,
    );
    let ctx = ui.ctx().clone();
    let cards = [
        (tl!("Browse files…"), "file.import", tl!("Movies, audio and stills from disk")),
        (tl!("Demo footage"), "file.importDemoFootage", tl!("Six procedural 1080p clips with sound")),
        (tl!("Demo project"), "file.openDemoProject", tl!("A cut sequence with transitions, effects and music")),
        (tl!("Bars and Tone"), "file.newBarsAndTone", tl!("SMPTE HD bars with 1 kHz reference tone")),
    ];
    let cw = 260.0;
    for (i, (title, cmd, sub)) in cards.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(24.0 + i as f32 * (cw + 16.0), 90.0), vec2(cw, 150.0));
        let resp = ui.interact(r, egui::Id::new(("imp", *cmd)), Sense::click());
        app.auto.add(&format!("import.{cmd}"), r, title);
        ui.painter().rect_filled(r, 8.0, if resp.hovered() { t.hover } else { t.tl_header_bg });
        ui.painter().text(r.min + vec2(16.0, 110.0), Align2::LEFT_CENTER, *title, Tokens::semibold(14.0), t.text);
        ui.painter().text(r.min + vec2(16.0, 132.0), Align2::LEFT_CENTER, *sub, Tokens::ui(11.5), t.text_dim);
        crate::icons::paint(
            ui.painter(),
            Rect::from_min_size(r.min + vec2(16.0, 18.0), vec2(56.0, 56.0)),
            [crate::icons::Icon::Folder, crate::icons::Icon::Film, crate::icons::Icon::Sequence, crate::icons::Icon::Grid][i],
            Color32::from_rgb(140, 150, 255),
        );
        if resp.clicked() {
            let _ = crate::menus::invoke(app, &ctx, cmd, json!({}));
            app.ui.mode = crate::state::Mode::Edit;
        }
    }
    let y = 90.0 + 150.0 + 38.0;
    crate::brand::paint_wordmark(ui, rect.min + vec2(24.0, y), 17.0, app.ui.dark);
    ui.painter().text(
        rect.min + vec2(24.0, y + 30.0),
        Align2::LEFT_CENTER,
        "Import a capture or choose a project to start editing.",
        Tokens::ui(13.0),
        t.text_dim,
    );
}
