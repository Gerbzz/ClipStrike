//! Windows VERSIONINFO for the ClipStrike editor. The window icon is drawn in main.rs.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    windows::embed_resources();
}

#[cfg(windows)]
mod windows {
    pub fn embed_resources() {
        let mut res = winresource::WindowsResource::new();
        res.set("ProductName", "ClipStrike")
            .set("FileDescription", "ClipStrike video editor")
            .set("LegalCopyright", "Copyright (c) the FilmCraft contributors and ClipStrike contributors.")
            .set("OriginalFilename", "clipstrike.exe")
            .set("InternalName", "clipstrike");
        if let Err(e) = res.compile() {
            println!("cargo:warning=clipstrike.exe built without version resources: {e}");
        }
    }
}

#[cfg(not(windows))]
mod windows {
    pub fn embed_resources() {}
}
