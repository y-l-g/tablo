//! Column widths: the declared shares, the chrome columns, and the table floor.

use std::borrow::Cow;

use super::super::{
    super::column::{ColumnWidth, NARROW_DEFAULT_PERCENT},
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

/// The table-level floor a wide column contributes to the table's
/// `min-width`, in whole rem.
///
/// A wide column declares no width, so a sum of declared widths alone would
/// let it crush to zero on a narrow viewport. Six rem keeps body text readable
/// and, summed across the wide columns, trips the wrapper's horizontal scroll
/// before the fixed layout crushes them.
const WIDE_COLUMN_MIN_REM: u8 = 6;

/// The most of the table the kind defaults claim together.
///
/// The defaults are shares of the table, and the columns that declare none
/// take what they leave: a total over 100% gives those columns no space at
/// all, and `table-fixed` renders a column with no space at zero width, header
/// text included. The budget keeps the rest of the table for them whatever the
/// column set.
pub(super) const DEFAULT_WIDTH_BUDGET_PERCENT: u8 = 60;

/// The share a kind default claims, scaled down when the table's defaults
/// together (`total`) exceed [`DEFAULT_WIDTH_BUDGET_PERCENT`].
fn scaled_default_percent(nominal: u8, total: u32) -> u8 {
    if total <= u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) {
        return nominal;
    }
    let scaled = u32::from(nominal) * u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) / total;
    // `scaled` is at most the budget, so the conversion cannot fail.
    u8::try_from(scaled).unwrap_or(DEFAULT_WIDTH_BUDGET_PERCENT)
}

/// The `style` value a kind default emits.
fn default_width_style(percent: u8) -> Cow<'static, str> {
    Cow::Owned(format!("width: {percent}%"))
}

/// The `style` a data column's cells carry: an explicit `Rem`/`Percent`
/// verbatim, a kind default scaled against the table's defaults (`total`), and
/// nothing for a wide column, which takes a share of what the declared ones
/// leave.
fn column_width_style(width: ColumnWidth, total: u32) -> Option<Cow<'static, str>> {
    width.explicit_css().or_else(|| {
        width
            .default_percent()
            .map(|nominal| default_width_style(scaled_default_percent(nominal, total)))
    })
}

/// The terms of a fixed-layout table's `min-width`: every share as emitted and
/// the lengths as one rem total.
///
/// With `w-full` the table never exceeds its container on its own, so without
/// the floor the wrapper's `overflow-x-auto` never scrolls; with it the table
/// keeps its measure on a narrow viewport and the wrapper scrolls.
#[derive(Default)]
struct MinWidth {
    percent: Vec<u8>,
    rem: u32,
}

impl MinWidth {
    /// A share of the table, as the column emits it.
    fn share(&mut self, percent: u8) {
        self.percent.push(percent);
    }

    /// A length, in whole rem.
    fn rem(&mut self, rem: u8) {
        self.rem += u32::from(rem);
    }

    /// A data column's term: its scaled share or its length, and
    /// [`WIDE_COLUMN_MIN_REM`] for a wide column, which declares nothing.
    fn column(&mut self, width: ColumnWidth, total: u32) {
        match width {
            ColumnWidth::Wide => self.rem(WIDE_COLUMN_MIN_REM),
            ColumnWidth::Narrow => {
                self.share(scaled_default_percent(NARROW_DEFAULT_PERCENT, total))
            }
            ColumnWidth::Rem(rem) => self.rem(rem),
            ColumnWidth::Percent(share) => self.share(share),
        }
    }

    /// The `min-width` style, emitted only when the sum carries a length:
    /// shares alone are a fraction of the container and can never overflow it.
    fn style(&self) -> Option<Cow<'static, str>> {
        (self.rem > 0).then(|| {
            let mut parts: Vec<String> = self
                .percent
                .iter()
                .map(|share| format!("{share}%"))
                .collect();
            parts.push(format!("{}rem", self.rem));
            if parts.len() == 1 {
                Cow::Owned(format!("min-width: {}", parts[0]))
            } else {
                Cow::Owned(format!("min-width: calc({})", parts.join(" + ")))
            }
        })
    }
}
