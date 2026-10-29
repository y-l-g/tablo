//! Renders the styled components `tablo-core` uses; apps depend on this crate
//! instead of running `topcoat ui add`.

pub mod components;
pub mod icons;

// The primitives under `components/primitives/` are copied verbatim from `topcoat-ui-registry`
// (sync with `cargo xtask sync-topcoat-ui`); owned code lives in `components/composites/`.
//
// Only the primitives a custom page builds from are public API. The rest are `#[doc(hidden)]`:
// `tablo-core` renders with them, but a registry change to them never moves Tablo's semver. The
// sync's `PUBLIC_PRIMITIVES` list decides which is which.
#[doc(hidden)]
pub use components::primitives::{
    alert_dialog::alert_dialog,
    avatar::{AvatarSize, avatar, avatar_fallback, avatar_image},
    breadcrumb::{
        breadcrumb, breadcrumb_ellipsis, breadcrumb_item, breadcrumb_link, breadcrumb_list,
        breadcrumb_page, breadcrumb_separator,
    },
    dialog::{
        dialog, dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
    },
    dropdown_menu::{
        dropdown_menu, dropdown_menu_content, dropdown_menu_item, dropdown_menu_label,
        dropdown_menu_separator, dropdown_menu_sub, dropdown_menu_sub_content,
        dropdown_menu_sub_trigger, dropdown_menu_trigger,
    },
    floating::{FLOATING_SCOPE, FloatingAlign, FloatingAnchor, FloatingSide, floating_variants},
    pagination::{
        pagination, pagination_content, pagination_ellipsis, pagination_item, pagination_link,
        pagination_next, pagination_previous,
    },
    sheet::{SheetSide, sheet, sheet_content},
    sidebar::{
        SidebarCollapsible, SidebarMenuButtonSize, SidebarMenuButtonVariant, SidebarSide,
        SidebarVariant, sidebar, sidebar_content, sidebar_footer, sidebar_group,
        sidebar_group_action, sidebar_group_content, sidebar_group_label, sidebar_header,
        sidebar_input, sidebar_inset, sidebar_menu, sidebar_menu_action, sidebar_menu_badge,
        sidebar_menu_button, sidebar_menu_button_variants, sidebar_menu_item,
        sidebar_menu_skeleton, sidebar_menu_sub, sidebar_menu_sub_button, sidebar_menu_sub_item,
        sidebar_provider, sidebar_rail, sidebar_separator, sidebar_trigger,
    },
};
pub use components::{
    composites::{
        empty_state::empty_state,
        error_state::error_state,
        page::{page, page_actions, page_content, page_description, page_header, page_title},
        stat::stat,
        toast::{
            toast, toast_close, toast_content, toast_description, toast_icon, toast_title, toaster,
        },
    },
    primitives::{
        alert::{AlertVariant, alert, alert_description, alert_title},
        badge::{BadgeVariant, badge, badge_variants},
        button::{ButtonSize, ButtonVariant, button, button_variants},
        card::{card, card_content, card_description, card_footer, card_header, card_title},
        checkbox::checkbox,
        field::{
            FieldLegendVariant, FieldOrientation, field, field_content, field_description,
            field_error, field_group, field_label, field_legend, field_separator, field_set,
            field_title,
        },
        input::input,
        label::label,
        radio_group::{radio_group, radio_group_item},
        select::select,
        separator::{SeparatorOrientation, separator},
        skeleton::skeleton,
        switch::switch,
        table::{
            table, table_body, table_caption, table_cell, table_footer, table_head, table_header,
            table_row,
        },
        textarea::textarea,
    },
};

pub const SIDEBAR_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/sidebar.js");
pub const THEME_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/theme.js");
pub const SELECTS_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/selects.js");
pub const REPEATERS_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/repeaters.js");
pub const FILE_LISTS_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/file_lists.js");
pub const PASSWORD_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/password.js");
pub const TABS_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/tabs.js");
pub const FORMS_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/forms.js");
pub const RICH_TEXT_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/rich_text.js");
pub const CHOICE_CREATE_JS: topcoat::asset::Asset =
    topcoat::asset::asset!("../assets/choice_create.js");
/// The TipTap bundle `rich_text.js` imports where a page holds a rich text control (ADR-0032):
/// no page loads it otherwise.
pub const TIPTAP_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/tiptap/tiptap.js");

/// The scripts every panel page loads, in order.
pub const SHELL_SCRIPTS: [topcoat::asset::Asset; 10] = [
    SIDEBAR_JS,
    THEME_JS,
    SELECTS_JS,
    CHOICE_CREATE_JS,
    REPEATERS_JS,
    FILE_LISTS_JS,
    PASSWORD_JS,
    TABS_JS,
    FORMS_JS,
    RICH_TEXT_JS,
];
