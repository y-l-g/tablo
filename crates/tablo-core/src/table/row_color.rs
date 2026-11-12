//! [`Table::row_color`]: a tint per row.

use std::sync::Arc;

use derive_where::derive_where;

use super::Table;
use crate::{Color, Reads};

/// The color a row is tinted in, and the fields it reads.
#[derive_where(Clone, Default)]
pub(crate) struct RowColor<M> {
    color: Option<crate::column::ReadFn<M, Option<Color>>>,
    /// The fields `color` reads, when declared.
    pub(super) reads: Option<Reads<M>>,
    /// Whether `color` reads a copy of the row without its relations.
    bare: bool,
}

impl<M> Table<M> {
    /// Tint each row in `color(row)`, such as an overdue invoice in [`Color::Danger`]; a row it
    /// gives `None` stays plain.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
    /// # struct Invoice { #[key] #[auto] id: uuid::Uuid, number: String, overdue: bool }
    /// use tablo_core::{Color, Reads, Table, TextColumn, lens};
    ///
    /// Table::new(TextColumn::new(lens!(Invoice.number)))
    ///     .row_color(|invoice: &Invoice| invoice.overdue.then_some(Color::Danger))
    ///     .row_color_reading(Reads::of(lens!(Invoice.overdue)));
    /// ```
    ///
    /// The closure may read any field, so a resource that
    /// [restricts a field](crate::ResourceDef::field_access) refuses the table until
    /// [`row_color_reading`](Self::row_color_reading) declares the ones it reads, and a user who
    /// may not view one of them sees every row plain.
    pub fn row_color(
        mut self,
        color: impl Fn(&M) -> Option<Color> + Send + Sync + 'static,
    ) -> Self {
        self.row_color.color = Some(Arc::new(color));
        self
    }

    /// Declare the fields the [`row_color`](Self::row_color) reads. A color declaring it reads
    /// none of the row's relations, beside a column loading one, reads a copy of the row without
    /// them, which [`PanelModel`](crate::PanelModel) builds: it then mounts beside a column
    /// loading a record of a model a resource restricts a field of.
    pub fn row_color_reading(mut self, reads: Reads<M>) -> Self {
        self.row_color.reads = Some(reads);
        self
    }

    /// The color `row` is tinted in.
    pub(crate) fn color_of(&self, row: &M) -> Option<Color> {
        self.row_color.color.as_ref().and_then(|color| color(row))
    }

    /// Whether the table tints its rows by a color that reads the row's relations as loaded.
    pub(crate) fn row_color_reads_row(&self) -> bool {
        self.row_color.color.is_some() && !self.row_color.bare
    }

    /// Have the row color read `bare(row)` when it declares it reads none of the row's relations.
    pub(super) fn bare_row_color(&mut self, bare: fn(&M) -> M)
    where
        M: toasty::schema::Model + 'static,
    {
        if !crate::column::reads_own_fields(self.row_color.reads.as_ref()) {
            return;
        }
        if let Some(color) = self.row_color.color.take() {
            self.row_color.color = Some(Arc::new(move |row: &M| color(&bare(row))));
            self.row_color.bare = true;
        }
    }

    /// Whether the table tints its rows by a color that does not declare what it reads.
    pub(crate) fn row_color_reads_undeclared(&self) -> bool {
        self.row_color.color.is_some() && self.row_color.reads.is_none()
    }
}
