//! The app's Lucide icons, resolved once from the staged icon set.
//!
//! `iconify_icon!` resolves against the **compiling crate's** staged sets
//! (ADR-0007 stages Lucide in this crate's build script), so the consts live
//! here and `tablo-core` renders them with `icon` instead of staging a
//! second copy of the set.

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
/// Back to the list.
pub const ARROW_LEFT: IconData = iconify_icon!("lucide:arrow-left");
/// Empty state.
pub const INBOX: IconData = iconify_icon!("lucide:inbox");
/// A link that leaves the panel (a public page).
pub const EXTERNAL_LINK: IconData = iconify_icon!("lucide:external-link");

// Navigation icons an app can hand to `NavigationItem::icon` without staging
// its own icon set.

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
