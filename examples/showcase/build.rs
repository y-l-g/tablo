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
            // Cargo shows a build script's output only when it fails, or as a `cargo::warning`.
            println!(
                "cargo::warning=tablo_build::tailwind failed: {e} - creating empty stylesheet for \
                 offline build"
            );
            let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
            let out = std::path::Path::new(&out_dir).join("tailwind.css");
            std::fs::write(out, "/* tailwind build failed - offline */\n")
                .expect("the fallback stylesheet is written");
        }
    }
}
