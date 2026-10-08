//! Renders the styled components `tablo-core` uses; apps depend on this crate
//! instead of running `topcoat ui add`.

pub mod components;
pub mod icons;

// Mirrors the vendored registry components verbatim (sync with
// `cargo xtask sync-topcoat-ui`); owned code lives in `components/composites/`.
pub use components::{
    composites::{
        empty_state::empty_state,
        error_state::error_state,
        page::{page, page_actions, page_content, page_description, page_header, page_title},
        theme::theme_init_script,
        toast::{
            toast, toast_close, toast_content, toast_description, toast_icon, toast_title, toaster,
        },
    },
    primitives::{
        alert::{AlertVariant, alert, alert_description, alert_title},
        alert_dialog::alert_dialog,
        button::{ButtonSize, ButtonVariant, button, button_variants},
        card::{card, card_content, card_description, card_footer, card_header, card_title},
        checkbox::checkbox,
        dialog::{
            dialog, dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        },
        field::{
            FieldLegendVariant, FieldOrientation, field, field_content, field_description,
            field_error, field_group, field_label, field_legend, field_separator, field_set,
            field_title,
        },
        input::input,
        label::label,
        pagination::{
            pagination, pagination_content, pagination_ellipsis, pagination_item, pagination_link,
            pagination_next, pagination_previous,
        },
        select::select,
        separator::{SeparatorOrientation, separator},
        sheet::{SheetSide, sheet, sheet_content},
        sidebar::{
            SidebarCollapsible, SidebarMenuButtonSize, SidebarMenuButtonVariant, SidebarSide,
            SidebarVariant, sidebar, sidebar_content, sidebar_footer, sidebar_group,
            sidebar_group_action, sidebar_group_content, sidebar_group_label, sidebar_header,
            sidebar_input, sidebar_inset, sidebar_menu, sidebar_menu_action, sidebar_menu_badge,
            sidebar_menu_button, sidebar_menu_button_variants, sidebar_menu_item,
            sidebar_menu_skeleton, sidebar_menu_sub, sidebar_menu_sub_button,
            sidebar_menu_sub_item, sidebar_provider, sidebar_rail, sidebar_separator,
            sidebar_trigger,
        },
        skeleton::skeleton,
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
