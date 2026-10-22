//! The registry components Tablo vendors, and the ones among them that are public API.

/// The registry components Tablo vendors into `primitives/`; add a name here and run `cargo xtask
/// sync-topcoat-ui`.
pub const VENDORED_PRIMITIVES: &[&str] = &[
    "alert",
    "alert_dialog",
    "avatar",
    "badge",
    "breadcrumb",
    "button",
    "card",
    "checkbox",
    "dialog",
    "dropdown_menu",
    "field",
    "floating",
    "input",
    "label",
    "pagination",
    "radio_group",
    "select",
    "separator",
    "sheet",
    "sidebar",
    "skeleton",
    "switch",
    "table",
    "textarea",
];

/// The vendored primitives that are part of `tablo-ui`'s public API: the ones a custom page builds
/// from. `primitives.rs` declares every other one `#[doc(hidden)]`, so a registry change to it
/// never moves Tablo's semver; `tablo-core` still uses it.
pub const PUBLIC_PRIMITIVES: &[&str] = &[
    "alert",
    "badge",
    "button",
    "card",
    "checkbox",
    "field",
    "input",
    "label",
    "radio_group",
    "select",
    "separator",
    "skeleton",
    "switch",
    "table",
    "textarea",
];
