//! Column widths: the declared shares, the chrome columns, and the table floor.

use std::borrow::Cow;

use super::{ColumnHead, Frame};
use crate::table::column::{ColumnWidth, NARROW_DEFAULT_PERCENT};

/// The share of the table the bulk-selection column claims.
pub(super) const BULK_COLUMN_PERCENT: u8 = 5;

/// The width every column of one render declares, in column order, plus the chrome columns.
pub(super) struct ColumnWidths {
    pub(super) cells: Vec<Option<Cow<'static, str>>>,
    pub(super) bulk: Option<Cow<'static, str>>,
    pub(super) actions: Option<Cow<'static, str>>,
    pub(super) table_min_width: Option<Cow<'static, str>>,
}

/// A length in sixteenths of a rem, a pixel at the default root size, which every width term is a
/// whole multiple of.
type Units = u32;

const UNITS_PER_REM: Units = 16;

/// The width of one icon button, [`ButtonSize::Icon`](tablo_ui::ButtonSize)'s `size-9`.
const ICON_BUTTON: Units = 36;
/// The width a labelled button takes beside its text: `ButtonSize::Sm`'s `px-3` and its border.
const LABEL_PADDING: Units = 26;
/// The width between two controls, `gap-1`.
const GAP: Units = 4;
/// The width a table cell's own padding takes, `p-3`.
const CELL_PADDING: Units = 24;
/// The width one character of a button label takes at `text-sm`, the measured average rounded up:
/// an estimate under it wraps the controls, which their cell allows.
const LABEL_CHAR: Units = 7;
/// The width one character of a header takes at `text-sm`, a little wider than a button's: a header
/// neither wraps nor truncates, it paints over the column beside it.
const HEAD_CHAR: Units = 8;
/// The width a table head's own padding takes, `px-3`.
const HEAD_PADDING: Units = 24;
/// The width a sortable head's icon and its gap take.
const SORT_ICON: Units = 20;
/// The floor a wide column claims, whatever its header says.
const WIDE_COLUMN_MIN: Units = 96;

impl Frame<'_> {
    /// Whether the table renders a row-actions column.
    pub(super) fn with_actions(&self) -> bool {
        self.delete_prefix.is_some()
            || self.edit_prefix.is_some()
            || self.view_prefix.is_some()
            || !self.row_actions.is_empty()
    }

    /// The icon buttons the column renders: View, Edit and Delete, each when the table wires it.
    fn action_icon_count(&self) -> Units {
        let buttons = usize::from(self.view_prefix.is_some())
            + usize::from(self.edit_prefix.is_some())
            + usize::from(self.delete_prefix.is_some());
        Units::try_from(buttons).unwrap_or(Units::MAX)
    }

    /// Count the row links side by side in the actions column, counting a labeled custom action as
    /// two.
    fn action_link_count(&self) -> usize {
        usize::from(self.view_prefix.is_some())
            + usize::from(self.edit_prefix.is_some())
            + usize::from(self.delete_prefix.is_some())
            + 2 * self.row_actions.len()
    }

    /// Claim the share of the table the row-actions column takes by link count.
    fn actions_percent(&self) -> u8 {
        match self.action_link_count() {
            2 => 12,
            3.. => 15,
            _ => 8,
        }
    }

    /// The width the row-actions column needs: each labelled button at its label's width, each
    /// icon button, the gaps between them, and the cell's own padding.
    fn actions_width(&self) -> Units {
        let labels: Units = self
            .row_actions
            .iter()
            .map(|label| LABEL_PADDING + chars(label, LABEL_CHAR))
            .sum();
        let icons = self.action_icon_count() * ICON_BUTTON;
        let controls = Units::try_from(self.row_actions.len())
            .unwrap_or(Units::MAX)
            .saturating_add(self.action_icon_count());
        CELL_PADDING + labels + icons + GAP * controls.saturating_sub(1)
    }

    /// Resolve the width every column of this table declares.
    pub(super) fn column_widths(&self) -> ColumnWidths {
        let bulk = self.bulk_enabled().then_some(BULK_COLUMN_PERCENT);
        let actions = self.with_actions().then(|| self.actions_width());
        let total: u32 = bulk
            .into_iter()
            .chain(
                self.columns
                    .iter()
                    .filter_map(|col| col.width.default_percent()),
            )
            .chain(self.with_actions().then(|| self.actions_percent()))
            .map(u32::from)
            .sum();
        let cells = self
            .columns
            .iter()
            .map(|col| column_width_style(col, total))
            .collect();
        let mut min_width = MinWidth::default();
        if let Some(share) = bulk {
            min_width.share(scaled_default_percent(share, total));
        }
        for col in &self.columns {
            min_width.column(col, total);
        }
        let actions_style = actions.map(|width| {
            min_width.length(width);
            Cow::Owned(format!("width: {}", rem(width)))
        });
        ColumnWidths {
            cells,
            bulk: bulk.map(|share| default_width_style(scaled_default_percent(share, total))),
            actions: actions_style,
            table_min_width: min_width.style(),
        }
    }
}

/// The width `text` takes, at `per_char` a character, and twice that past ASCII: a wide script's
/// glyph is about two Latin columns, and a single control cannot wrap.
fn chars(text: &str, per_char: Units) -> Units {
    text.chars()
        .map(|c| if c.is_ascii() { per_char } else { per_char * 2 })
        .sum()
}

/// The floor a wide column claims: its header's text and padding, never less than
/// [`WIDE_COLUMN_MIN`]. Its cells truncate, so its header is the widest thing it must show.
fn wide_floor(head: &ColumnHead<'_>) -> Units {
    let header =
        chars(head.label, HEAD_CHAR) + HEAD_PADDING + if head.sortable { SORT_ICON } else { 0 };
    header.max(WIDE_COLUMN_MIN)
}

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

/// Resolve a data column's cell `style`: its explicit CSS, its floor for a wide column, which a
/// fixed layout otherwise leaves at whatever the declared columns leave, or its scaled kind
/// default.
fn column_width_style(head: &ColumnHead<'_>, total: u32) -> Option<Cow<'static, str>> {
    head.width.explicit_css().or_else(|| match head.width {
        ColumnWidth::Wide => Some(Cow::Owned(format!("width: {}", rem(wide_floor(head))))),
        _ => head
            .width
            .default_percent()
            .map(|nominal| default_width_style(scaled_default_percent(nominal, total))),
    })
}

/// A length as a CSS `rem` value.
fn rem(units: Units) -> String {
    let fraction = units % UNITS_PER_REM;
    if fraction == 0 {
        return format!("{}rem", units / UNITS_PER_REM);
    }
    let fraction = format!("{:04}", fraction * 625);
    let fraction = fraction.trim_end_matches('0');
    format!("{}.{fraction}rem", units / UNITS_PER_REM)
}

/// Accumulate a fixed-layout table's `min-width` terms.
#[derive(Default)]
struct MinWidth {
    percent: Vec<u8>,
    units: Units,
}

impl MinWidth {
    /// A share of the table, as the column emits it.
    fn share(&mut self, percent: u8) {
        self.percent.push(percent);
    }

    /// A length, in sixteenths of a rem.
    fn length(&mut self, units: Units) {
        self.units += units;
    }

    /// A data column's term: its scaled share or its length, and the header
    /// floor for a wide column, which declares nothing.
    fn column(&mut self, head: &ColumnHead<'_>, total: u32) {
        match head.width {
            ColumnWidth::Wide => self.length(wide_floor(head)),
            ColumnWidth::Narrow => {
                self.share(scaled_default_percent(NARROW_DEFAULT_PERCENT, total))
            }
            ColumnWidth::Rem(rem) => self.length(Units::from(rem) * UNITS_PER_REM),
            ColumnWidth::Percent(share) => self.share(share),
        }
    }

    /// The `min-width` style, emitted only when the sum carries a length:
    /// shares alone are a fraction of the container and can never overflow it.
    fn style(&self) -> Option<Cow<'static, str>> {
        (self.units > 0).then(|| {
            let mut parts: Vec<String> = self
                .percent
                .iter()
                .map(|share| format!("{share}%"))
                .collect();
            parts.push(rem(self.units));
            if parts.len() == 1 {
                Cow::Owned(format!("min-width: {}", parts[0]))
            } else {
                Cow::Owned(format!("min-width: calc({})", parts.join(" + ")))
            }
        })
    }
}

#[cfg(test)]
mod tests;
