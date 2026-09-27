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
    // Tailwind is per-app (`tablo_ui::tailwind_build` in app's build.rs), nothing to do here.
}
