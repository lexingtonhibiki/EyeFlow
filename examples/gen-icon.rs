//! Regenerates the static icon asset `assets/icon.ico` (used by the README and
//! the NSIS installer) from the procedural renderer in `src/icon.rs`.
//!
//! Run inside the repository root:
//!   cargo run --example gen-icon
//! or without cargo (writes relative to the current directory):
//!   rustc --edition 2021 examples/gen-icon.rs -o gen-icon.exe && ./gen-icon.exe

#[path = "../src/icon.rs"]
mod icon;

use std::fs;
use std::path::PathBuf;

/// Same multi-resolution set as the one embedded by `build.rs`.
const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().expect("cannot determine repository root"));

    let assets = root.join("assets");
    fs::create_dir_all(&assets).expect("failed to create assets directory");

    let bytes = icon::encode_ico(&ICON_SIZES);
    let path = assets.join("icon.ico");
    fs::write(&path, &bytes).expect("failed to write assets/icon.ico");
    println!(
        "wrote {} ({} bytes, sizes {:?})",
        path.display(),
        bytes.len(),
        ICON_SIZES
    );
}
