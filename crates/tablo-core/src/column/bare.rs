//! A column reading a copy of its row without the row's relations: what a panel mounts in place of
//! a closure over the row that declares it reads none.

use std::sync::Arc;

use toasty::stmt::{Expr, OrderByExpr};
use topcoat::{context::Cx, view::BoxView};

use super::{
    Alignment, BoxColumn, Column, ColumnInternals, ColumnToggle, ColumnWidth, Editor, Includes,
    Related,
};

/// `inner`, reading `bare(row)` wherever it reads a row.
pub(crate) struct Bare<M> {
    inner: BoxColumn<M>,
    bare: fn(&M) -> M,
}

/// Whether a closure declaring it reads `reads` reads none of the row's relations: it then reads a
/// copy of the row without them, and the page's loads cannot reach it.
pub(crate) fn reads_own_fields<M: toasty::schema::Model>(
    reads: Option<&crate::lens::Reads<M>>,
) -> bool {
    reads.is_some_and(|reads| {
        crate::toasty_compat::model::read_relations::<M>(reads.fields())
            .is_some_and(|models| models.is_empty())
    })
}

/// `column`, reading `bare(row)` when it is a closure over its row that loads nothing and declares
/// it reads none of the row's relations; `column` itself otherwise.
pub(crate) fn bare_column<M>(column: BoxColumn<M>, bare: fn(&M) -> M) -> BoxColumn<M>
where
    M: toasty::schema::Model + 'static,
{
    let reads_row = column.internals().related() == Related::Row
        && column.includes().into_vec().is_empty()
        && reads_own_fields(column.reads().as_ref());
    if reads_row {
        Arc::new(Bare {
            inner: column,
            bare,
        })
    } else {
        column
    }
}

impl<M> Column<M> for Bare<M> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn label(&self) -> crate::Text {
        self.inner.label()
    }

    fn text(&self, cx: &Cx, row: &M) -> String {
        self.inner.text(cx, &(self.bare)(row))
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        self.inner.cell(cx, &(self.bare)(row))
    }

    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        self.inner.entry(cx, &(self.bare)(row))
    }

    fn column_width(&self) -> ColumnWidth {
        self.inner.column_width()
    }

    fn toggle(&self) -> ColumnToggle {
        self.inner.toggle()
    }

    fn alignment(&self) -> Alignment {
        self.inner.alignment()
    }

    fn search_expr(&self, term: &str) -> Option<Expr<bool>> {
        self.inner.search_expr(term)
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.inner.order_by(descending)
    }

    fn includes(&self) -> Includes<M> {
        self.inner.includes()
    }

    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        self.inner.reads()
    }

    fn internals(&self) -> &dyn ColumnInternals<M> {
        self
    }
}

impl<M> ColumnInternals<M> for Bare<M> {
    fn restrict<'a>(
        &'a self,
        cx: &'a Cx,
        rows: &'a [M],
        ex: &'a mut dyn toasty::Executor,
    ) -> crate::BoxFuture<'a, topcoat::Result<()>> {
        self.inner.internals().restrict(cx, rows, ex)
    }

    fn masked(&self, cx: &Cx) -> bool {
        self.inner.internals().masked(cx)
    }

    /// The copy holds no related record.
    fn related(&self) -> Related {
        Related::None
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.inner.internals().misdeclared()
    }

    fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
        self.inner.internals().unavailable_source(cx)
    }

    fn edits(&self) -> Option<Editor> {
        self.inner.internals().edits()
    }

    fn edit_options(&self) -> Vec<(String, crate::Text)> {
        self.inner.internals().edit_options()
    }

    fn editor<'a>(&self, cx: &'a Cx, row: &M) -> Option<BoxView<'a>> {
        self.inner.internals().editor(cx, &(self.bare)(row))
    }
}
