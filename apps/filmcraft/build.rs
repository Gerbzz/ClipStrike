//! Windows VERSIONINFO for the ClipStrike editor. The approved artwork also supplies the native window icon.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CLIPSTRIKE_REQUIRE_WINRES");
    println!("cargo:rerun-if-changed=../../assets/branding/ClipStrike.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    windows::embed_resources();
}

#[cfg(windows)]
mod windows {
    pub fn embed_resources() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/branding/ClipStrike.ico")
        .set("ProductName", "ClipStrike")
            .set("FileDescription", "ClipStrike video editor")
            .set("LegalCopyright", "Copyright (c) the FilmCraft contributors and ClipStrike contributors.")
            .set("OriginalFilename", "clipstrike.exe")
            .set("InternalName", "clipstrike");
        if let Err(e) = res.compile() {
            if std::env::var_os("CLIPSTRIKE_REQUIRE_WINRES").is_some() {
                eprintln!("embedding Windows resources failed: {e}");
                std::process::exit(1);
            }
            println!("cargo:warning=clipstrike.exe built without version resources: {e}");
        }
    }
}

#[cfg(not(windows))]
mod windows {
    pub fn embed_resources() {}
}
