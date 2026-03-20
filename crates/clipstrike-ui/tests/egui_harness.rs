use clipstrike_core::project::Project;
use clipstrike_ui::app_config::AppConfig;
use clipstrike_ui::ui::ClipStrikeApp;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn add_test_clip_button_does_not_crash_and_inserts_a_clip() {
    let project = Project::new("Harness", 1920, 1080, 30.0);
    let app = ClipStrikeApp::test_harness(project);
    let mut harness = Harness::new_ui_state(|ui, app| app.render_project_files_test_ui(ui), app);

    let add_button = harness.get_by_label("Add Test Clip");
    add_button.click();
    harness.run();

    let snapshot = harness.state().test_snapshot();
    assert_eq!(snapshot["clips_count"], 1);
    assert_eq!(snapshot["tracks_count"], 5);
}

#[test]
fn import_button_loads_fixture_and_row_add_button_adds_clip() {
    let project = Project::new("Import Harness", 1920, 1080, 30.0);
    let mut app = ClipStrikeApp::test_harness(project);

    let fixture_dir = std::env::temp_dir().join("clipstrike-egui-import-fixture");
    std::fs::create_dir_all(&fixture_dir).expect("create fixture dir");
    let fixture_path = fixture_dir.join("0203 (1)-1.mp4");
    std::fs::write(&fixture_path, b"clipstrike import fixture").expect("write fixture");
    app.set_test_media_picker_paths(vec![fixture_path.clone()]);

    let initial_snapshot = app.test_snapshot();
    let initial_assets = initial_snapshot["assets_count"].as_u64().unwrap_or(0);

    let mut harness = Harness::new_ui_state(|ui, app| app.render_project_files_test_ui(ui), app);

    harness.get_by_label("Import").click();
    harness.run();

    harness.get_by_label("0203 (1)-1.mp4");
    harness
        .get_all_by_label("+")
        .last()
        .expect("expected imported row add button")
        .click();
    harness.run();

    let snapshot = harness.state().test_snapshot();
    assert!(snapshot["assets_count"].as_u64().unwrap_or(0) >= initial_assets + 1);
    assert_eq!(snapshot["clips_count"], 1);
}

#[test]
fn motion_track_test_button_applies_keyframes_to_first_clip() {
    let project = Project::new("Harness Track", 1920, 1080, 30.0);
    let app = ClipStrikeApp::test_harness(project);
    let mut harness = Harness::new_ui_state(|ui, app| app.render_project_files_test_ui(ui), app);

    harness.get_by_label("Add Test Clip").click();
    harness.run();

    let motion_button = harness.get_by_label("Apply Motion Track (Test)");
    motion_button.click();
    harness.run();

    let snapshot = harness.state().test_snapshot();
    assert_eq!(snapshot["clips_count"], 1);
    assert_eq!(snapshot["tracking_data_count"].as_u64(), Some(1));
    assert_eq!(snapshot["motion_tracking_enabled"].as_bool(), Some(true));
    assert_eq!(
        snapshot["test_first_clip_motion_keyframes"].as_u64(),
        Some(3)
    );
}

#[test]
fn detection_labels_test_button_updates_state_and_snapshot() {
    let project = Project::new("Harness Labels", 1920, 1080, 30.0);
    let app = ClipStrikeApp::test_harness(project);
    let mut harness = Harness::new_ui_state(|ui, app| app.render_project_files_test_ui(ui), app);

    harness.get_by_label("Add Test Clip").click();
    harness.run();

    let labels_button = harness.get_by_label("Apply Detection Labels (Test)");
    labels_button.click();
    harness.run();

    let snapshot = harness.state().test_snapshot();
    assert_eq!(snapshot["clips_count"], 1);
    assert_eq!(snapshot["object_detection_enabled"].as_bool(), Some(true));
    assert_eq!(snapshot["detection_count"].as_u64(), Some(1));
    assert_eq!(
        snapshot["test_first_clip_detection_labels"].as_u64(),
        Some(1)
    );
}

#[test]
fn recovery_banner_shows_when_checkpoint_exists() {
    let project = Project::new("Harness Recovery Banner", 1920, 1080, 30.0);
    let app = ClipStrikeApp::test_harness(project);
    let mut harness = Harness::new_ui_state(|ui, app| app.render_project_files_test_ui(ui), app);

    let appdata_dir = std::env::temp_dir().join("clipstrike-egui-appdata");
    std::fs::create_dir_all(&appdata_dir).expect("create appdata dir");
    unsafe {
        std::env::set_var("APPDATA", &appdata_dir);
    }

    let checkpoint_path = AppConfig::untitled_recovery_checkpoint_path();
    if let Some(parent) = checkpoint_path.parent() {
        std::fs::create_dir_all(parent).expect("create recovery dir");
    }
    Project::new("Recovered", 1920, 1080, 30.0)
        .save(&checkpoint_path)
        .expect("seed recovery checkpoint");

    harness.run();

    harness.get_by_label("Recovery checkpoint available");
    harness.get_by_label("Restore Recovery Checkpoint");

    let _ = std::fs::remove_file(checkpoint_path);
}
