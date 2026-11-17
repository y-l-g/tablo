//! The registry components Tablo vendors, and the ones among them that are public API.

/// The registry components Tablo vendors into `primitives/`; add a name here and run `cargo xtask
/// sync-topcoat-ui`. The components that carry Tablo's look (buttons, badges, cards, the form
/// controls, tables and alerts) are Tablo's own, in `composites/` (ADR-0007); `button` and
/// `input` stay here only because the vendored sidebar and pagination build on them.
pub const VENDORED_PRIMITIVES: &[&str] = &[
    "alert_dialog",
    "avatar",
    "breadcrumb",
    "button",
    "dialog",
    "dropdown_menu",
    "field",
    "floating",
    "input",
    "label",
    "pagination",
    "separator",
    "sheet",
    "sidebar",
    "skeleton",
];

/// The vendored primitives that are part of `tablo-ui`'s public API: the ones a custom page builds
/// from. `primitives.rs` declares every other one `#[doc(hidden)]`, so a registry change to it
/// never moves Tablo's semver; `tablo-core` still uses it.
pub const PUBLIC_PRIMITIVES: &[&str] = &["field", "label", "separator", "skeleton"];
