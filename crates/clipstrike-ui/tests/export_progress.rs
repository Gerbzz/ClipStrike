use std::time::Duration;

use clipstrike_ui::export::{ExportProgress, export_frame_count_for_duration_ms};

#[test]
fn export_frame_count_rounds_up_and_never_returns_zero() {
    assert_eq!(export_frame_count_for_duration_ms(0, 30, 1), 1);
    assert_eq!(export_frame_count_for_duration_ms(1_000, 10, 3), 4);
    assert_eq!(export_frame_count_for_duration_ms(1_000, 25, 1), 25);
}

#[test]
fn export_progress_reports_fraction_eta_and_cancel() {
    let mut progress = ExportProgress::new(10);

    progress.mark_frame_complete();
    progress.mark_frame_complete();

    assert!((progress.progress_fraction() - 0.2).abs() < f32::EPSILON);
    assert_eq!(
        progress.estimated_seconds_remaining(Duration::from_secs(2)),
        Some(8.0)
    );

    progress.request_cancel();
    assert!(progress.cancel_requested());
}
