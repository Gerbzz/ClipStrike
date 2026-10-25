//! FilmStrike wordmark, drawn in code so the editor does not use ArtCraft trademarks.

/// Draw the wordmark `height` points tall with its left edge at `left_center`.
pub fn paint_wordmark(ui: &egui::Ui, left_center: egui::Pos2, height: f32, dark: bool) -> Option<egui::Rect> {
    let color = if dark { egui::Color32::from_rgb(238, 240, 245) } else { egui::Color32::from_rgb(31, 35, 44) };
    let font = egui::FontId::proportional(height);
    let galley = ui.painter().layout_no_wrap("FILMSTRIKE".to_owned(), font, color);
    let rect = egui::Rect::from_min_size(egui::pos2(left_center.x, left_center.y - galley.size().y / 2.0), galley.size());
    ui.painter().galley(rect.min, galley, color);
    Some(rect)
}
