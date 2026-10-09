use clipstrike_core::capture_import::import_package_to_new_project;
use clipstrike_core::project::Project;
use serde_json::json;
use std::fs;

#[test]
fn moved_package_relinks_without_losing_edits_and_changed_hash_stays_offline() {
    let dir = tempfile::TempDir::new().unwrap();
    let workspace = dir.path().join("workspace");
    let package = workspace.join("package");
    fs::create_dir_all(&package).unwrap();
    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let clips: Vec<_> = (1..=3)
        .map(|order| {
            fs::write(package.join(format!("clip-{order}.mp4")), b"abc").unwrap();
            json!({
                "clip_id": format!("clip-{order}"),
                "relative_path": format!("clip-{order}.mp4"),
                "sha256": hash, "demo_sha256": hash,
                "event_id": format!("event-{order}"),
                "player_steamid64": "76561198000000001",
                "map": "de_mirage", "round": 4, "utility_type": "smoke",
                "start_tick": 1000 + order * 100, "end_tick": 1100 + order * 100,
                "camera_role": "projectile", "duration_ms": 1000,
                "width": 1920, "height": 1080, "fps_num": 60, "fps_den": 1,
                "audio": {"source": "hlae_wav", "channels": 2, "sample_rate": 48000, "verified": true},
                "qc_status": "accepted", "group_order": order
            })
        })
        .collect();
    fs::write(package.join("manifest.json"), serde_json::to_vec(&json!({
        "schema_version": 1, "package_id": "pilot-001", "clips": clips
    })).unwrap()).unwrap();
    let project_path = workspace.join("edit.cstrike");
    let mut project = import_package_to_new_project(&package, &project_path).unwrap();
    project.clips[1].start = 5_000;
    project.clips[1].end = 6_000;
    project.save(&project_path).unwrap();

    let relocated = dir.path().join("relocated");
    fs::rename(&workspace, &relocated).unwrap();
    let relocated_project = relocated.join("edit.cstrike");
    let (loaded, warnings) = Project::load_with_capture_warnings(&relocated_project).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(loaded.clips[1].start, 5_000);
    assert!(loaded.assets.iter().all(|asset| asset.uri.starts_with("file:///")));

    let moved_package = dir.path().join("package-only-moved");
    fs::rename(relocated.join("package"), &moved_package).unwrap();
    let (mut offline, warnings) = Project::load_with_capture_warnings(&relocated_project).unwrap();
    assert_eq!(warnings.len(), 3);
    assert!(offline.assets.iter().all(|asset| asset.uri.starts_with("capture-offline://")));
    assert_eq!(offline.clips[1].start, 5_000);
    offline.relink_capture_package(&relocated_project, "pilot-001", &moved_package).unwrap();
    offline.save(&relocated_project).unwrap();
    let (relinked, warnings) = Project::load_with_capture_warnings(&relocated_project).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(relinked.clips[1].start, 5_000);
    assert!(relinked.assets.iter().all(|asset| asset.uri.starts_with("file:///")));

    fs::write(moved_package.join("clip-2.mp4"), b"abx").unwrap();
    let (mut changed, warnings) = Project::load_with_capture_warnings(&relocated_project).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("media hash mismatch: clip-2"));
    assert!(changed.assets[0].uri.starts_with("file:///"));
    assert!(changed.assets[1].uri.starts_with("capture-offline://"));
    assert!(changed.assets[2].uri.starts_with("file:///"));
    assert_eq!(changed.clips[1].start, 5_000);
    let before = changed.clone();
    assert!(changed.relink_capture_package(&relocated_project, "pilot-001", &moved_package).is_err());
    assert_eq!(changed, before);
}
