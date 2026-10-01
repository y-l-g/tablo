fn main() {
    // A dependent's build script only sees the `links` metadata of its direct
    // dependencies, so an app that depends on this facade alone would not see
    // `tablo-core`'s or `tablo-ui`'s source directories. Forward each one as
    // its own key — `DEP_TABLO_CORE` and `DEP_TABLO_UI` — which
    // `tablo_build::tailwind` reads. A change in either crate's metadata reruns
    // this script through the `links` edge.
    println!("cargo::rerun-if-changed=build.rs");
    for (from, key) in [("DEP_TABLO_CORE_SRC", "core"), ("DEP_TABLO_UI_SRC", "ui")] {
        let dir = std::env::var(from).unwrap_or_else(|_| panic!("{from} is set by the dependency"));
        println!("cargo::metadata={key}={dir}");
    }
}
