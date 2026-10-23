//! The hook contract between the shell's JS assets and the Rust that renders their markup.

use std::path::{Path, PathBuf};

/// The directory holding the hand-written shell JS assets.
pub fn assets_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join("crates/tablo-ui/assets")
}

/// Each shell JS asset's file under `assets/` and the `tablo-ui` constant wiring it into the
/// document head.
pub const ASSET_FILES: &[(&str, &str)] = &[
    ("sidebar.js", "SIDEBAR_JS"),
    ("theme.js", "THEME_JS"),
    ("selects.js", "SELECTS_JS"),
    ("repeaters.js", "REPEATERS_JS"),
    ("password.js", "PASSWORD_JS"),
    ("tabs.js", "TABS_JS"),
    ("rich_text.js", "RICH_TEXT_JS"),
];

/// One hook-contract entry: `js` appears in the asset's source and `rust` appears in the Rust
/// render sources.
pub struct AssetHook {
    /// The asset file under `assets/` that consumes the hook.
    pub asset: &'static str,
    /// The needle that must appear in the asset's source.
    pub js: &'static str,
    /// The needle that must appear in the Rust sources.
    pub rust: &'static str,
}

/// Track new hooks here as they land.
pub const ASSET_HOOKS: &[AssetHook] = &[
    AssetHook {
        asset: "password.js",
        js: "data-password-toggle",
        rust: "data-password-toggle",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "data-sidebar",
        rust: "data-sidebar",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "data-state",
        rust: "data-state",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "tablo_sidebar",
        rust: "tablo_sidebar",
    },
    AssetHook {
        asset: "theme.js",
        js: "data-theme-set",
        rust: "data-theme-set",
    },
    AssetHook {
        asset: "theme.js",
        js: "tablo_scheme",
        rust: "tablo_scheme",
    },
    AssetHook {
        asset: "theme.js",
        js: "tablo_theme",
        rust: "tablo_theme",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-select-filterable",
        rust: "data-select-filterable",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-filter",
        rust: "data-options-filter",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-field",
        rust: "data-options-field",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-server",
        rust: "data-options-server",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-combobox",
        rust: "data-options-combobox",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-list",
        rust: "data-options-list",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater",
        rust: "data-repeater",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-next",
        rust: "data-repeater-next",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-order",
        rust: "data-repeater-order",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-rows",
        rust: "data-repeater-rows",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-row",
        rust: "data-repeater-row",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-blank",
        rust: "data-repeater-blank",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-add",
        rust: "data-repeater-add",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-remove",
        rust: "data-repeater-remove",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-up",
        rust: "data-repeater-up",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-down",
        rust: "data-repeater-down",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "__row__",
        rust: "__row__",
    },
    AssetHook {
        asset: "tabs.js",
        js: "data-tabs",
        rust: "data-tabs",
    },
    AssetHook {
        asset: "tabs.js",
        js: "data-tab-panel",
        rust: "data-tab-panel",
    },
    AssetHook {
        asset: "tabs.js",
        js: "role=tablist",
        rust: "role=\"tablist\"",
    },
    AssetHook {
        asset: "tabs.js",
        js: "role=tab]",
        rust: "role=\"tab\"",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text",
        rust: "data-rich-text",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "tablo-tiptap",
        rust: "tablo-tiptap",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-tools",
        rust: "data-rich-text-tools",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-toolbar",
        rust: "data-rich-text-toolbar",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-tool",
        rust: "data-rich-text-tool",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-content",
        rust: "data-rich-text-content",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-source",
        rust: "data-rich-text-source",
    },
];
