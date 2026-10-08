//! Resolves the app's Lucide icons once from the staged set.

use topcoat::icon::{IconData, iconify::iconify_icon};

/// Sortable column, neither direction active.
pub const ARROW_UP_DOWN: IconData = iconify_icon!("lucide:arrow-up-down");
/// Sortable column, sorted ascending.
pub const ARROW_UP: IconData = iconify_icon!("lucide:arrow-up");
/// Sortable column, sorted descending.
pub const ARROW_DOWN: IconData = iconify_icon!("lucide:arrow-down");
/// Toast success.
pub const CIRCLE_CHECK: IconData = iconify_icon!("lucide:circle-check");
/// Toast info.
pub const INFO: IconData = iconify_icon!("lucide:info");
/// Toast warning.
pub const TRIANGLE_ALERT: IconData = iconify_icon!("lucide:triangle-alert");
/// Toast error.
pub const OCTAGON_X: IconData = iconify_icon!("lucide:octagon-x");
/// Toast close button.
pub const X: IconData = iconify_icon!("lucide:x");
/// Theme toggle, shown in light mode.
pub const SUN: IconData = iconify_icon!("lucide:sun");
/// Theme toggle, shown in dark mode.
pub const MOON: IconData = iconify_icon!("lucide:moon");
/// Sign-out control.
pub const LOG_OUT: IconData = iconify_icon!("lucide:log-out");
/// Row action: view the record.
pub const EYE: IconData = iconify_icon!("lucide:eye");
/// Row and page action: edit the record.
pub const PENCIL: IconData = iconify_icon!("lucide:pencil");
/// Row and bulk action: delete.
pub const TRASH: IconData = iconify_icon!("lucide:trash");
/// Search input adornment.
pub const SEARCH: IconData = iconify_icon!("lucide:search");
/// Create action.
pub const PLUS: IconData = iconify_icon!("lucide:plus");
/// Page action: export the list as CSV.
pub const DOWNLOAD: IconData = iconify_icon!("lucide:download");
/// Back to the list.
pub const ARROW_LEFT: IconData = iconify_icon!("lucide:arrow-left");
/// Empty state.
pub const INBOX: IconData = iconify_icon!("lucide:inbox");
/// A link that leaves the panel (a public page).
pub const EXTERNAL_LINK: IconData = iconify_icon!("lucide:external-link");
/// A menu that switches between choices: the tenant switcher.
pub const CHEVRON_DOWN: IconData = iconify_icon!("lucide:chevron-down");
/// The selected choice in a menu.
pub const CHECK: IconData = iconify_icon!("lucide:check");

// Navigation icons for `NavigationItem::icon`.
/// Navigation: a dashboard or home page.
pub const LAYOUT_DASHBOARD: IconData = iconify_icon!("lucide:layout-dashboard");
/// Navigation: people.
pub const USERS: IconData = iconify_icon!("lucide:users");
/// Navigation: authors, writing.
pub const PEN_LINE: IconData = iconify_icon!("lucide:pen-line");
/// Navigation: documents, posts.
pub const FILE_TEXT: IconData = iconify_icon!("lucide:file-text");
/// Navigation: comments, messages.
pub const MESSAGE_SQUARE: IconData = iconify_icon!("lucide:message-square");
/// Navigation: media, images.
pub const IMAGE: IconData = iconify_icon!("lucide:image");
/// Navigation: activity, live data.
pub const ACTIVITY: IconData = iconify_icon!("lucide:activity");
