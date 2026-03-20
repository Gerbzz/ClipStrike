use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn gstreamer_root() -> PathBuf {
    let repo_local = repo_root()
        .join("gst_extracted")
        .join("PFiles64")
        .join("gstreamer")
        .join("1.0")
        .join("msvc_x86_64");
    if repo_local.join("bin").join("glib-2.0-0.dll").exists() {
        return repo_local;
    }

    PathBuf::from(r"C:\Program Files\gstreamer\1.0\msvc_x86_64")
}

fn clipstrike_exe() -> PathBuf {
    repo_root()
        .join("target")
        .join("debug")
        .join("clipstrike-app.exe")
}

fn configure_gstreamer_env(cmd: &mut Command, gst_root: &Path) {
    cmd.env("GST_ROOT", gst_root);
    cmd.env(
        "PATH",
        format!(
            "{};{}",
            gst_root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    cmd.env(
        "GST_PLUGIN_SYSTEM_PATH",
        gst_root.join("lib").join("gstreamer-1.0"),
    );
    cmd.env(
        "GST_PLUGIN_PATH",
        gst_root.join("lib").join("gstreamer-1.0"),
    );
    cmd.env("PKG_CONFIG", gst_root.join("bin").join("pkg-config.exe"));
    cmd.env(
        "PKG_CONFIG_PATH",
        format!(
            "{};{}",
            gst_root.join("lib").join("pkgconfig").display(),
            gst_root
                .join("lib")
                .join("gstreamer-1.0")
                .join("pkgconfig")
                .display()
        ),
    );
    cmd.env("GSTREAMER_1_0_ROOT_MSVC_X86_64", gst_root);
    cmd.env("RUST_BACKTRACE", "1");
}

fn build_clipstrike_desktop() -> io::Result<()> {
    let gst_root = gstreamer_root();
    if !gst_root.join("bin").join("glib-2.0-0.dll").exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("GStreamer runtime not found at {}", gst_root.display()),
        ));
    }

    let mut cmd = Command::new("cargo");
    cmd.current_dir(repo_root());
    cmd.args([
        "build",
        "-p",
        "clipstrike-app",
        "--features",
        "gstreamer-backend",
        "--locked",
    ]);
    configure_gstreamer_env(&mut cmd, &gst_root);

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "cargo build failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

pub fn launch_clipstrike_desktop() -> io::Result<Child> {
    build_clipstrike_desktop()?;

    let gst_root = gstreamer_root();
    let exe = clipstrike_exe();

    if !exe.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("desktop executable not found at {}", exe.display()),
        ));
    }

    if !gst_root.join("bin").join("glib-2.0-0.dll").exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("GStreamer runtime not found at {}", gst_root.display()),
        ));
    }

    let mut cmd = Command::new(exe);
    cmd.current_dir(repo_root());
    configure_gstreamer_env(&mut cmd, &gst_root);

    cmd.spawn()
}
