//! The prebuilt stylesheet `tablo-ui` ships stays the one Tablo's sources build.

use std::path::Path;

/// What `tablo_build::default_stylesheet` built for this checkout's sources.
const BUILT: &str = include_str!(concat!(env!("OUT_DIR"), "/tablo-default.css"));

/// The committed copy `tablo::ui::DEFAULT_STYLESHEET` serves.
const SHIPPED: &str = include_str!("../../../crates/tablo-ui/assets/tablo.css");

#[test]
fn the_shipped_stylesheet_is_the_one_tablos_sources_build() {
    if BUILT.is_empty() {
        // The build script could not run Tailwind, offline, and warned.
        return;
    }
    if std::env::var_os("TABLO_BLESS").is_some() {
        let shipped =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/tablo-ui/assets/tablo.css");
        std::fs::write(shipped, BUILT).expect("the shipped stylesheet is rewritten");
        return;
    }
    assert!(
        SHIPPED == BUILT,
        "crates/tablo-ui/assets/tablo.css is stale: regenerate it with \
         `TABLO_BLESS=1 cargo test -p showcase --test it default_stylesheet`"
    );
}

#[test]
fn the_shipped_stylesheet_holds_classes_only_tablo_writes() {
    // Tablo writes `tabular-nums` from its column and pager sources.
    assert!(
        SHIPPED.contains(".tabular-nums{"),
        "tabular-nums is missing"
    );
}

#[test]
fn the_shipped_stylesheet_carries_the_default_theme() {
    for token in ["--primary:", "--sidebar-accent:"] {
        assert!(SHIPPED.contains(token), "{token} is missing");
    }
    assert!(SHIPPED.contains(".dark{"), "the dark tokens are missing");
}

/// An app on the prebuilt stylesheet can wear any theme `Brand::theme` picks.
#[test]
fn the_shipped_stylesheet_carries_every_theme() {
    for theme in ["amber", "graphite", "iris"] {
        assert!(
            SHIPPED.contains(&format!("[data-tablo-theme=\"{theme}\"]"))
                || SHIPPED.contains(&format!("[data-tablo-theme={theme}]")),
            "the {theme} theme is missing"
        );
    }
}
