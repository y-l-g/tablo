fn main() {
    // The panel's markup carries Tailwind classes, so an app's stylesheet must
    // scan this crate's sources. `links` hands their absolute path to the build
    // script of every package that depends on this one, as `DEP_TABLO_CORE_SRC`,
    // wherever Cargo unpacked the crate; `tablo_build::tailwind` reads it.
    println!("cargo::rerun-if-changed=build.rs");
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo::metadata=src={dir}/src");
}
