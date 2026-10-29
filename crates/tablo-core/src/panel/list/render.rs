//! [`WiredTable`] HTML rendering: the entry points plus the chrome.
//!
//! The model-typed code stops at the projection: the table's rows become [`rows::RowView`]s,
//! its filters become controls, and its declaration and wiring become a [`Frame`]. Everything
//! after renders from those, so the markup compiles once rather than once per model.
//!
//! A page encodes its row-action URL base once.
//!
//! The entry points and the table chrome live in `core`, the header row in
//! `head`, row projection and rendering in `rows`, the zero-rows cell in
//! `empty`, the search and bulk bars in `toolbar`, the tab bar in `tabs`,
//! the filter menu in `filter_menu`, pagination in `pager`, the confirmation
//! dialogs in `dialog`, and the streamed placeholder in `skeleton`.

mod core;
mod dialog;
mod empty;
mod filter_menu;
mod head;
mod pager;
mod rows;
mod skeleton;
mod tabs;
mod toolbar;

use topcoat::{
    context::Cx,
    icon::IconData,
    runtime::Event,
    view::{Attributes, StaticClass, attributes, class},
};

use super::{TableSignals, WiredTable};
use crate::{
    panel::url::with_return,
    schema::Schema,
    table::{ActionLayout, Alignment, ColumnWidth, TableState, query_of},
};

/// What rendering reads from a [`WiredTable`], without its model: the declared columns and
/// toolbars, and the action wiring.
pub(super) struct Frame<'t> {
    /// The columns the reader's choice shows.
    columns: Vec<ColumnHead<'t>>,
    /// The column picker, for a reader who may keep a choice.
    picker: Option<ColumnPicker<'t>>,
    search: bool,
    filter_menu: bool,
    /// The page sizes the reader may pick; empty offers no choice.
    page_sizes: Vec<usize>,
    /// The groupings the reader may pick, by `?group_by=` value and label; empty offers none.
    groupings: Vec<(&'t str, crate::Text)>,
    delete_prefix: Option<&'t str>,
    edit_prefix: Option<&'t str>,
    view_prefix: Option<&'t str>,
    actions_prefix: Option<&'t str>,
    bulk_delete: bool,
    bulk_actions: Vec<BulkAction<'t>>,
    /// The custom actions asking for input, each rendering its dialog.
    inputs: Vec<InputAction<'t>>,
    /// The labels of the custom actions each row may show.
    row_actions: Vec<&'t str>,
    /// Whether a row's actions sit in a menu rather than side by side.
    action_menu: bool,
    return_to: Option<&'t str>,
    /// Whether the bulk bar may select every record the list's query selects.
    select_matching: bool,
}

/// One column's header, width and alignment.
struct ColumnHead<'t> {
    name: &'t str,
    label: crate::Text,
    sortable: bool,
    width: ColumnWidth,
    alignment: Alignment,
}

/// The toolbar's menu of the toggleable columns, posting the reader's choice.
struct ColumnPicker<'t> {
    /// The route the choice posts to.
    url: &'t str,
    columns: Vec<PickerColumn<'t>>,
}

/// One toggleable column in the [`ColumnPicker`].
struct PickerColumn<'t> {
    name: &'t str,
    label: crate::Text,
    /// Whether the reader's choice shows it.
    shown: bool,
}

/// One custom action the bulk bar offers.
struct BulkAction<'t> {
    name: &'static str,
    label: &'t str,
    /// Whether the confirmation dialog asks first; an action with input confirms in its own.
    confirm: bool,
    /// The question the confirmation dialog asks.
    prompt: &'t str,
    icon: Option<IconData>,
    destructive: bool,
    /// Whether its button opens the action's input dialog.
    input: bool,
}

/// One custom action asking for input, whose dialog the table renders once for its rows and its
/// bulk bar.
struct InputAction<'t> {
    name: &'static str,
    label: &'t str,
    confirm: bool,
    input: fn() -> Schema,
}

impl<M> WiredTable<M> {
    /// This table's [`Frame`].
    pub(super) fn frame(&self) -> Frame<'_>
    where
        M: toasty::schema::Model,
    {
        Frame {
            columns: self
                .visible_columns()
                .map(|col| ColumnHead {
                    name: col.name(),
                    label: col.label(),
                    sortable: col.is_sortable(),
                    width: col.column_width(),
                    alignment: col.alignment(),
                })
                .collect(),
            picker: self.columns_url().map(|url| ColumnPicker {
                url,
                columns: self
                    .declared_columns()
                    .iter()
                    .filter(|col| col.toggle().is_toggleable())
                    .map(|col| PickerColumn {
                        name: col.name(),
                        label: col.label(),
                        shown: self.shows(col),
                    })
                    .collect(),
            }),
            search: self.search_enabled(),
            filter_menu: self.filter_menu_enabled(),
            page_sizes: self.page_sizes(),
            groupings: self
                .declared_groupings()
                .iter()
                .map(|group| (group.name(), group.label()))
                .collect(),
            delete_prefix: self.delete_prefix(),
            edit_prefix: self.edit_prefix(),
            view_prefix: self.view_prefix(),
            actions_prefix: self.actions_prefix(),
            bulk_delete: self.bulk_delete_enabled(),
            bulk_actions: self
                .bulk_custom_actions()
                .map(|action| BulkAction {
                    name: action.name,
                    label: &action.label,
                    confirm: action.confirm && action.input.is_none(),
                    prompt: &action.bulk_prompt,
                    icon: action.icon.clone(),
                    destructive: action.destructive,
                    input: action.input.is_some(),
                })
                .collect(),
            inputs: self
                .row_custom_actions()
                .chain(self.bulk_custom_actions().filter(|action| !action.row))
                .filter_map(|action| {
                    Some(InputAction {
                        name: action.name,
                        label: &action.label,
                        confirm: action.confirm,
                        input: action.input?,
                    })
                })
                .collect(),
            row_actions: self
                .row_custom_actions()
                .map(|action| action.label.as_str())
                .collect(),
            action_menu: self.row_action_layout() == ActionLayout::Menu,
            return_to: self.return_to(),
            select_matching: self.select_matching(),
        }
    }
}

impl Frame<'_> {
    /// Whether the table renders a row-actions column.
    fn with_actions(&self) -> bool {
        self.delete_prefix.is_some()
            || self.edit_prefix.is_some()
            || self.view_prefix.is_some()
            || !self.row_actions.is_empty()
    }

    /// The column that takes the table's spare width: the first that
    /// [`Grow`](ColumnWidth::Grow)s.
    fn slack_column(&self) -> Option<usize> {
        self.columns
            .iter()
            .position(|col| col.width == ColumnWidth::Grow)
    }

    /// Whether the table offers bulk selection.
    fn bulk_enabled(&self) -> bool {
        self.bulk_delete || !self.bulk_actions.is_empty()
    }

    /// `url`, sending the write it starts back where the table was wired to return.
    fn action_url(&self, url: String) -> String {
        match self.return_to {
            Some(target) => with_return(&url, target),
            None => url,
        }
    }
}

/// The chrome columns, bulk selection and row actions: as narrow as their controls, which never
/// wrap.
const CHROME_CELL_CLASS: &str = "w-px whitespace-nowrap";

/// The toolbar row above the table: the search field, the bulk actions, the column picker and the
/// filter menu. Its padding lines the search field up with the first column.
const BAR_CLASS: StaticClass =
    class!("flex flex-wrap items-center gap-2 border-b border-border px-4 py-3 sm:px-6");

/// A row's View or Edit link: muted until the reader reaches for it.
const ROW_ICON_CLASS: StaticClass = class!(
    "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground outline-none",
    "transition-colors hover:bg-foreground/5 hover:text-foreground focus-visible:ring-2",
    "focus-visible:ring-ring [&_svg]:size-4",
);

/// A row's Delete: the danger color only under the pointer, so a column of them stays quiet.
const ROW_DELETE_CLASS: StaticClass = class!(
    "inline-flex size-8 items-center justify-center rounded-md text-muted-foreground outline-none",
    "transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:ring-2",
    "focus-visible:ring-ring [&_svg]:size-4",
);

/// The search input's box: its width, and the anchor for the icon inside it.
const SEARCH_FIELD_CLASS: StaticClass = class!("relative w-full sm:w-72");

/// The magnifier inside the search input's leading padding.
const SEARCH_ICON_CLASS: StaticClass = class!(
    "pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
);

/// A table's own card: the surface every panel card shares (`tablo_ui::card`'s
/// border, fill and shadow), clipped so the rows keep its rounded corners.
pub(crate) const TABLE_CARD_CLASS: StaticClass = class!(
    "overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-sm"
);

/// A secondary link inside a bar ("Clear filters", "Clear search").
const QUIET_LINK_CLASS: StaticClass = class!("text-sm text-muted-foreground hover:text-foreground");

/// A link in the empty-table message ("Clear search", "Back to first page").
const EMPTY_LINK_CLASS: StaticClass =
    class!("text-sm font-medium text-foreground underline underline-offset-4");

/// The DOM id of one piece of a table's chrome, distinct per table on the page.
pub(super) fn table_dom_id(state: &TableState, suffix: &str) -> String {
    format!("{}-{suffix}", state.prefix.as_deref().unwrap_or("table"))
}

/// A table link's attributes: `href`, and a plain click that writes the link's query to the
/// table's `query` signal instead of navigating, so the page reruns with the new state in place.
/// A modified click opens `href` the browser's way.
pub(super) fn live_link(cx: &Cx, url: String, signals: &TableSignals) -> Attributes {
    let (query, all) = (signals.query.clone(), signals.all.clone());
    let next = query_of(&url).to_string();
    attributes! {
        cx =>
        href=(url)
        @click=$(|e: Event| {
            if e.ctrl_key {
                return;
            }
            if e.meta_key {
                return;
            }
            if e.shift_key {
                return;
            }
            if e.alt_key {
                return;
            }
            e.prevent_default();
            query.set(next.clone());
            all.set(false);
        })
    }
}
