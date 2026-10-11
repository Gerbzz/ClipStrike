//! Run the production saved-project compositor on a caller-owned project.
//! Example: cargo run -p clipstrike-ui --features gstreamer-backend \
//!   --example export_saved_project -- input.cstrike output.mp4

#[cfg(all(not(target_arch = "wasm32"), feature = "gstreamer-backend"))]
fn main() -> Result<(), String> {
    use clipstrike_media::export::{ExportFormat, ExportSettings, VideoCodec};
    use clipstrike_ui::ui::ClipStrikeApp;
    use std::path::PathBuf;

    let mut args = std::env::args_os().skip(1);
    let project = PathBuf::from(args.next().ok_or("Missing project .cstrike path")?);
    let output = PathBuf::from(args.next().ok_or("Missing output .mp4 path")?);
    if args.next().is_some() {
        return Err("Usage: export_saved_project <project.cstrike> <output.mp4>".into());
    }

    let settings = ExportSettings {
        format: ExportFormat::MP4,
        codec: VideoCodec::H264,
        width: 1280,
        height: 720,
        fps_num: 30,
        fps_den: 1,
        bitrate_kbps: 5000,
        use_hardware_acceleration: false,
    };
    ClipStrikeApp::export_saved_project_offscreen(&project, &output, settings)?;
    println!("Shared compositor export complete: {}", output.display());
    Ok(())
}

#[cfg(any(target_arch = "wasm32", not(feature = "gstreamer-backend")))]
fn main() {
    eprintln!("export_saved_project requires a native GStreamer-enabled build");
    std::process::exit(2);
}
