//! Column widths: the declared shares, the chrome columns, and the table floor.

use std::borrow::Cow;

use super::Frame;
use crate::table::column::{ColumnWidth, NARROW_DEFAULT_PERCENT};

/// The share of the table the bulk-selection column claims.
pub(super) const BULK_COLUMN_PERCENT: u8 = 5;

/// The width every column of one render declares, in column order, plus the chrome columns.
pub(super) struct ColumnWidths {
    pub(super) cells: Vec<Option<Cow<'static, str>>>,
    pub(super) bulk: Option<Cow<'static, str>>,
    pub(super) actions: Option<Cow<'static, str>>,
    pub(super) actions_min: Option<Cow<'static, str>>,
    pub(super) table_min_width: Option<Cow<'static, str>>,
}

impl Frame<'_> {
    /// Whether the table renders a row-actions column.
    pub(super) fn with_actions(&self) -> bool {
        self.delete_prefix.is_some()
            || self.edit_prefix.is_some()
            || self.view_prefix.is_some()
            || self.row_actions > 0
    }

    /// Count the row links side by side in the actions column, counting a labeled custom action as
    /// two.
    fn action_link_count(&self) -> usize {
        usize::from(self.view_prefix.is_some())
            + usize::from(self.edit_prefix.is_some())
            + usize::from(self.delete_prefix.is_some())
            + 2 * self.row_actions
    }

    /// Claim the share of the table the row-actions column takes by link count.
    fn actions_percent(&self) -> u8 {
        match self.action_link_count() {
            2 => 12,
            3.. => 15,
            _ => 8,
        }
    }

    /// Floor the row-actions column to its buttons by link count.
    fn actions_min_rem(&self) -> u8 {
        match self.action_link_count() {
            2 => 7,
            3.. => 9,
            _ => 4,
        }
    }

    /// Resolve the width every column of this table declares.
    pub(super) fn column_widths(&self) -> ColumnWidths {
        let bulk = self.bulk_enabled().then_some(BULK_COLUMN_PERCENT);
        let actions = self.with_actions().then(|| self.actions_percent());
        let actions_floor = self.with_actions().then(|| self.actions_min_rem());
        let total: u32 = bulk
            .into_iter()
            .chain(
                self.columns
                    .iter()
                    .filter_map(|col| col.width.default_percent()),
            )
            .chain(actions)
            .map(u32::from)
            .sum();
        let cells = self
            .columns
            .iter()
            .map(|col| column_width_style(col.width, total))
            .collect();
        let mut min_width = MinWidth::default();
        if let Some(share) = bulk {
            min_width.share(scaled_default_percent(share, total));
        }
        for col in &self.columns {
            min_width.column(col.width, total);
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

/// Contribute six rem per wide column to the table's `min-width`.
const WIDE_COLUMN_MIN_REM: u8 = 6;

/// Cap the kind defaults' combined share of the table so undeclared columns keep space.
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

/// Resolve a data column's cell `style` from its explicit CSS or scaled kind default.
fn column_width_style(width: ColumnWidth, total: u32) -> Option<Cow<'static, str>> {
    width.explicit_css().or_else(|| {
        width
            .default_percent()
            .map(|nominal| default_width_style(scaled_default_percent(nominal, total)))
    })
}

/// Accumulate a fixed-layout table's `min-width` terms.
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
