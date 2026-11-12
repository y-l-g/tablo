fn main() {
    // The log's markup carries Tailwind classes, so an app's stylesheet must scan this crate's
    // sources. `links` hands their path to the build script of every package that depends on this
    // one, as `DEP_TABLO_AUDIT_TABLO_SRC`, which `tablo_build::tailwind` reads.
    println!("cargo::rerun-if-changed=build.rs");
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo::metadata=tablo_src={dir}/src");
}
