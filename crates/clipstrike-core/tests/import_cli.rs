use clipstrike_core::capture_import::import_package_to_new_project;
use clipstrike_core::project::Project;
use std::fs;

#[test]
fn imports_three_separate_capture_clips_into_new_project_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let package = dir.path().join("package");
    fs::create_dir(&package).unwrap();
    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut clips: Vec<_> = (1..=3)
        .map(|order| {
            fs::write(package.join(format!("clip-{order}.mp4")), b"abc").unwrap();
            serde_json::json!({
                "clip_id": format!("clip-{order}"),
                "relative_path": format!("clip-{order}.mp4"),
                "sha256": hash, "demo_sha256": hash,
                "event_id": format!("event-{order}"),
                "player_steamid64": "76561198000000001", "map": "de_mirage",
                "round": 4, "utility_type": "smoke", "start_tick": 1000 + order * 100,
                "end_tick": 1100 + order * 100, "camera_role": "projectile",
                "duration_ms": 1000, "width": 1920, "height": 1080,
            "fps_num": 30000, "fps_den": 1001,
                "audio": {"source": "hlae_wav", "channels": 2,
                    "sample_rate": 48000, "verified": true},
                "qc_status": "accepted", "group_order": order
            })
        })
        .collect();
    fs::write(
        package.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "package_id": "pilot-001", "clips": clips
        }))
        .unwrap(),
    )
    .unwrap();
    let output = dir.path().join("pilot.cstrike");
    let fractional = import_package_to_new_project(&package, &output).unwrap_err();
    assert!(fractional.to_string().contains("fractional source FPS"));
    assert!(!output.exists());
    for clip in &mut clips {
        clip["fps_num"] = serde_json::json!(60);
        clip["fps_den"] = serde_json::json!(1);
    }
    fs::write(
        package.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "package_id": "pilot-001", "clips": clips
        }))
        .unwrap(),
    )
    .unwrap();
    import_package_to_new_project(&package, &output).unwrap();
    let project = Project::load(&output).unwrap();
    assert_eq!(project.clips.len(), 3);
    assert_eq!(project.assets.len(), 3);
    assert_eq!(project.capture_sources.len(), 3);
    assert_ne!(project.clips[0].asset_id, project.clips[1].asset_id);
    assert_eq!(project.capture_sources[0].clip.fps_num, 60);
    assert_eq!(project.capture_sources[0].clip.fps_den, 1);
    assert!(import_package_to_new_project(&package, &output).is_err());
    assert_eq!(Project::load(&output).unwrap(), project);
}
