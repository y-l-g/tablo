fn main() {
    // Per-app Tailwind contract: one styles.css + `tablo_build::tailwind()`,
    // which adds Tablo's own sources and watches them. See ADR-0007.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let out_dir = std::path::Path::new(&out_dir);
    // Try to build Tailwind; on failure (e.g. offline) create empty fallbacks so `cargo test` stays
    // green.
    match tablo_build::tailwind() {
        Ok(_) => {}
        Err(e) => {
            // Cargo shows a build script's output only when it fails, or as a `cargo::warning`.
            println!(
                "cargo::warning=tablo_build::tailwind failed: {e} - creating empty stylesheet for \
                 offline build"
            );
            std::fs::write(
                out_dir.join("tailwind.css"),
                "/* tailwind build failed - offline */\n",
            )
            .expect("the fallback stylesheet is written");
        }
    }
    // Tablo's prebuilt stylesheet, which `tests/default_stylesheet.rs` compares with the copy
    // `tablo-ui` ships. Empty when the build fails, which the test reads as offline.
    if let Err(e) = tablo_build::default_stylesheet() {
        println!("cargo::warning=tablo_build::default_stylesheet failed: {e}");
        std::fs::write(out_dir.join(tablo_build::DEFAULT_STYLESHEET), "")
            .expect("the empty default stylesheet is written");
    }
}
