//! Resolves the app's Lucide icons once from the staged set.

use topcoat::icon::{IconData, iconify::iconify_icon};

/// The user menu's trigger.
pub const CHEVRONS_UP_DOWN: IconData = iconify_icon!("lucide:chevrons-up-down");
/// The system theme.
pub const MONITOR: IconData = iconify_icon!("lucide:monitor");
/// A row's action menu.
pub const ELLIPSIS: IconData = iconify_icon!("lucide:ellipsis");
/// Sortable column, neither direction active.
pub const ARROW_UP_DOWN: IconData = iconify_icon!("lucide:arrow-up-down");
/// Sortable column, sorted ascending.
pub const ARROW_UP: IconData = iconify_icon!("lucide:arrow-up");
/// Sortable column, sorted descending.
pub const ARROW_DOWN: IconData = iconify_icon!("lucide:arrow-down");
/// Toast success, and a `true` boolean cell.
pub const CIRCLE_CHECK: IconData = iconify_icon!("lucide:circle-check");
/// A `false` boolean cell.
pub const CIRCLE_X: IconData = iconify_icon!("lucide:circle-x");
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
/// The user menu's profile link.
pub const USER: IconData = iconify_icon!("lucide:user");
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
/// A button that opens a list below it: a filter's checklist.
pub const CHEVRON_DOWN: IconData = iconify_icon!("lucide:chevron-down");
/// The selected choice in a menu.
pub const CHECK: IconData = iconify_icon!("lucide:check");
/// Relation action: link a record to the one the page shows.
pub const LINK: IconData = iconify_icon!("lucide:link");
/// The notification bell in the topbar.
pub const BELL: IconData = iconify_icon!("lucide:bell");
/// Marking every notification read.
pub const CHECK_CHECK: IconData = iconify_icon!("lucide:check-check");
/// Marking a notification unread.
pub const MAIL: IconData = iconify_icon!("lucide:mail");
/// The table's column picker.
pub const COLUMNS_3: IconData = iconify_icon!("lucide:columns-3");

// Rich text toolbar tools.
/// Rich text: bold.
pub const BOLD: IconData = iconify_icon!("lucide:bold");
/// Rich text: italic.
pub const ITALIC: IconData = iconify_icon!("lucide:italic");
/// Rich text: underline.
pub const UNDERLINE: IconData = iconify_icon!("lucide:underline");
/// Rich text: strikethrough.
pub const STRIKETHROUGH: IconData = iconify_icon!("lucide:strikethrough");
/// Rich text: inline code.
pub const CODE: IconData = iconify_icon!("lucide:code");
/// Rich text: a level-1 heading.
pub const HEADING_1: IconData = iconify_icon!("lucide:heading-1");
/// Rich text: a level-2 heading.
pub const HEADING_2: IconData = iconify_icon!("lucide:heading-2");
/// Rich text: a level-3 heading.
pub const HEADING_3: IconData = iconify_icon!("lucide:heading-3");
/// Rich text: a bulleted list.
pub const LIST: IconData = iconify_icon!("lucide:list");
/// Rich text: a numbered list.
pub const LIST_ORDERED: IconData = iconify_icon!("lucide:list-ordered");
/// Rich text: a quotation.
pub const TEXT_QUOTE: IconData = iconify_icon!("lucide:text-quote");
/// Rich text: a code block.
pub const SQUARE_CODE: IconData = iconify_icon!("lucide:square-code");
/// Rich text: a horizontal rule.
pub const MINUS: IconData = iconify_icon!("lucide:minus");
/// Rich text: undo.
pub const UNDO_2: IconData = iconify_icon!("lucide:undo-2");
/// Rich text: redo.
pub const REDO_2: IconData = iconify_icon!("lucide:redo-2");

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
/// Navigation: categories, tags.
pub const TAG: IconData = iconify_icon!("lucide:tag");
/// Navigation: media, images.
pub const IMAGE: IconData = iconify_icon!("lucide:image");
/// Navigation: activity, live data.
pub const ACTIVITY: IconData = iconify_icon!("lucide:activity");
