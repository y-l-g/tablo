fn main() {
    // Stage the Lucide icon set for `iconify_icon!` in primitives (ADR-0007
    // verbatim sync; the registry moved feather → lucide with the lazy-View
    // PR). The set is vendored in `assets/iconify` with its version sidecar,
    // so staging copies the committed files into OUT_DIR without network
    // access — the sniffing of `../../../topcoat` dates from the
    // sibling-clone era and silently disabled icons whenever the clone was
    // missing.
    topcoat::icon::iconify::BuildConfig::new()
        .icon_set_version("lucide", "1.2.137")
        .cache_dir("assets/iconify")
        .stage()
        .unwrap();
    // The components carry Tailwind classes, so an app's stylesheet must scan
    // these sources. `links` hands their absolute path to the build script of
    // every package that depends on this one, as `DEP_TABLO_UI_SRC`, wherever
    // Cargo unpacked the crate; `tablo_build::tailwind` reads it.
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo::metadata=src={dir}/src");
}
