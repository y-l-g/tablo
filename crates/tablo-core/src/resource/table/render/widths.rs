//! Column widths: the declared shares, the chrome columns, and the table floor.

use std::borrow::Cow;

use super::super::{
    super::column::{MinWidth, column_width_style, default_width_style, scaled_default_percent},
    Table,
};

/// The share of the table the bulk-selection column claims: one
/// checkbox plus the cell's `p-3` padding at the widths a list is read at. A
/// percentage, not a length: the column keeps its share as the table narrows,
/// and the columns that declare none keep theirs.
pub(super) const BULK_COLUMN_PERCENT: u8 = 5;

/// The width every column of one render declares: one `style` value
/// per declared column, in column order, plus the two chrome columns.
/// `None` is a column that declares no width — a wide column, which takes a
/// share of what the declared ones leave.
///
/// `actions_min` is the actions column's content floor for its body cells
/// (the header carries the share *and* the floor); `table_min_width` is the
/// table-level floor — the sum of the declared widths — that lets the
/// wrapper's `overflow-x-auto` scroll on a narrow viewport instead of
/// crushing the cells.
pub(super) struct ColumnWidths {
    pub(super) cells: Vec<Option<Cow<'static, str>>>,
    pub(super) bulk: Option<Cow<'static, str>>,
    pub(super) actions: Option<Cow<'static, str>>,
    pub(super) actions_min: Option<Cow<'static, str>>,
    pub(super) table_min_width: Option<Cow<'static, str>>,
}

impl<M> Table<M> {
    /// Whether the table renders a row-actions column.
    pub(super) fn with_actions(&self) -> bool {
        self.delete_prefix.is_some() || self.edit_prefix.is_some() || self.view_prefix.is_some()
    }

    /// How many row links sit side by side in the actions column.
    fn action_link_count(&self) -> usize {
        usize::from(self.view_prefix.is_some())
            + usize::from(self.edit_prefix.is_some())
            + usize::from(self.delete_prefix.is_some())
    }

    /// The share of the table the row-actions column claims: the row
    /// links sit side by side and each is a fixed-size control, so the share
    /// grows with the number of links the table renders. The values hold the
    /// widest set at a 1280px window and the narrower sets inside it.
    fn actions_percent(&self) -> u8 {
        match self.action_link_count() {
            2 => 18,
            3.. => 25,
            _ => 12,
        }
    }

    /// The content floor of the row-actions column, in whole rem: one row of
    /// `Md` buttons plus the cell's `p-3` padding, by link count. The share
    /// above is a fraction of the table and shrinks with it, so on a narrow
    /// viewport the buttons would spill past the table and clip against the
    /// chrome's `overflow-hidden`; the floor keeps the column as wide as its
    /// buttons, and the table's `min-width` keeps the table as wide as its
    /// columns, so the wrapper scrolls instead.
    fn actions_min_rem(&self) -> u8 {
        match self.action_link_count() {
            2 => 11,
            3.. => 15,
            _ => 7,
        }
    }

    /// The width every column of this table declares.
    ///
    /// The kind defaults — a [`ColumnWidth::Narrow`] column, the bulk
    /// checkbox, the row actions — are shares of the table, scaled down
    /// together when their nominal total exceeds
    /// `DEFAULT_WIDTH_BUDGET_PERCENT`: the wide columns take what the
    /// declared ones leave, and a table that spends every percent on declared
    /// columns leaves them none. An explicit `Rem`/`Percent` is emitted as
    /// declared.
    ///
    /// The table-level `min-width` is the sum of those declarations: every
    /// share as emitted, every `Rem` verbatim, the actions column's content
    /// floor, and one `WIDE_COLUMN_MIN_REM` per wide column (which declares
    /// nothing and would otherwise crush to zero). With `w-full` the table
    /// never exceeds its container on its own, so without the floor the
    /// wrapper's `overflow-x-auto` never scrolls; with it the table keeps its
    /// measure on a narrow viewport and the wrapper scrolls. Emitted only
    /// when the sum carries a length — shares alone are a fraction of the
    /// container and can never overflow it.
    pub(super) fn column_widths(&self) -> ColumnWidths
    where
        M: toasty::schema::Model,
    {
        let bulk = self.bulk_enabled().then_some(BULK_COLUMN_PERCENT);
        let actions = self.with_actions().then(|| self.actions_percent());
        let actions_floor = self.with_actions().then(|| self.actions_min_rem());
        let total: u32 = bulk
            .into_iter()
            .chain(
                self.columns
                    .iter()
                    .filter_map(|col| col.column_width().default_percent()),
            )
            .chain(actions)
            .map(u32::from)
            .sum();
        let cells = self
            .columns
            .iter()
            .map(|col| column_width_style(col.column_width(), total))
            .collect();
        // The `min-width` terms, in layout order. A scaled share is the
        // emitted one, so the floor and the column agree.
        let mut min_width = MinWidth::default();
        if let Some(share) = bulk {
            min_width.share(scaled_default_percent(share, total));
        }
        for col in &self.columns {
            min_width.column(col.column_width(), total);
        }
        let mut actions_style = None;
        if let (Some(share), Some(floor)) = (actions, actions_floor) {
            let scaled = scaled_default_percent(share, total);
            min_width.share(scaled);
            min_width.rem(floor);
            actions_style = Some(Cow::Owned(format!(
                "width: {scaled}%; min-width: {floor}rem"
            )));
        }
        ColumnWidths {
            cells,
            bulk: bulk.map(|share| default_width_style(scaled_default_percent(share, total))),
            actions: actions_style,
            actions_min: actions_floor.map(|floor| Cow::Owned(format!("min-width: {floor}rem"))),
            table_min_width: min_width.style(),
        }
    }
}
