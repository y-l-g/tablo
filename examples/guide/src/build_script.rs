//! The build script the Stylesheet chapter shows.

/// The app's `build.rs` runs the Tailwind build.
pub fn build_main() {
    // ANCHOR: build-script
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src");
    tablo_build::tailwind().expect("the Tailwind build runs");
    // ANCHOR_END: build-script
}
