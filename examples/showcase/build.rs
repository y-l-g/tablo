fn main() {
    // Per-app Tailwind contract: one styles.css + `tablo_build::tailwind()`,
    // which adds Tablo's own sources and watches them. See ADR-0007.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    // Try to build Tailwind; on failure (e.g. offline) create empty fallback so `cargo test` stays
    // green.
    match tablo_build::tailwind() {
        Ok(_) => {}
        Err(e) => {
            eprintln!(
                "warning: tablo_build::tailwind failed: {e} - creating empty stylesheet for offline \
                 build"
            );
            if let Ok(out_dir) = std::env::var("OUT_DIR") {
                let out = std::path::Path::new(&out_dir).join("tailwind.css");
                let _ = std::fs::write(out, "/* tailwind build failed - offline */\n");
            }
        }
    }
}
