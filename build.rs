//! Build script: embeds the application icon + version info into the Windows
//! executable. The icon itself is rendered at build time by the zero-dependency
//! `src/icon.rs`, so no binary asset needs to be committed for the embedded
//! resource. On machines without a resource compiler (rc.exe / windres) the
//! build still succeeds — we emit a `cargo:warning` and skip embedding.

use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "src/icon.rs"]
mod icon;

/// Icon sizes embedded into the executable (a classic multi-resolution set).
const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");

    // Resources only make sense for Windows targets (cross-compiles included).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // 1. Render the multi-size icon into $OUT_DIR/eyeflow.ico.
    let out_dir = match env::var("OUT_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(e) => {
            println!("cargo:warning=EyeFlow build script: OUT_DIR not set ({e}); skipping resource embedding.");
            return;
        }
    };
    let ico_path = out_dir.join("eyeflow.ico");
    if let Err(e) = fs::write(&ico_path, icon::encode_ico(&ICON_SIZES)) {
        println!(
            "cargo:warning=EyeFlow build script: failed to write {} ({e}); skipping resource embedding.",
            ico_path.display()
        );
        return;
    }

    // 2. Compile the resource (icon + version info). winresource picks rc.exe
    //    for MSVC and windres for the GNU toolchain. Failure is non-fatal.
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_string_lossy().as_ref());
    res.set("FileDescription", "EyeFlow 护眼提醒");
    res.set("ProductName", "EyeFlow");
    res.set("CompanyName", "lexingtonhibiki");
    res.set(
        "LegalCopyright",
        "Copyright (c) 2026 lexingtonhibiki. MIT License.",
    );
    res.set("OriginalFilename", "eyeflow.exe");
    res.set("InternalName", "eyeflow");
    // FileVersion / ProductVersion default to CARGO_PKG_VERSION inside winresource.

    if let Err(e) = res.compile() {
        println!(
            "cargo:warning=EyeFlow: could not embed Windows resources (icon/version info); \
             the executable will be built without them. \
             Typical cause: no resource compiler (rc.exe from the MSVC SDK, or windres from \
             MinGW binutils) found on PATH. Underlying error: {e}"
        );
    }
}
