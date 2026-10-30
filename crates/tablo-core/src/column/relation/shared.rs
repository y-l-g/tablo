//! The builders, the [`Column`] impl and the sealed internals both relation columns share.

use super::*;

/// The builders and the [`Column`] impl both relation columns share.
macro_rules! relation_column {
    ($ty:ident) => {
        impl<M> $ty<M> {
            /// Replace the label the field's name gives it, and name the column after the label,
            /// as a [`ComputedColumn`](crate::ComputedColumn) is: two columns over one relation
            /// need two labels.
            pub fn label(mut self, label: impl Into<crate::Text>) -> Self {
                self.relation.relabel(label.into());
                self
            }

            /// Declare this column's width.
            pub fn width(mut self, width: ColumnWidth) -> Self {
                self.relation.width = width;
                self
            }

            toggle_builders!(relation.toggle);
        }

        impl<M> Column<M> for $ty<M>
        where
            M: toasty::schema::Model + Send + Sync + 'static,
        {
            fn name(&self) -> &str {
                &self.relation.name
            }

            fn label(&self) -> crate::Text {
                self.relation.label()
            }

            fn text(&self, cx: &Cx, row: &M) -> String {
                self.relation.text(self.loaded_text(cx, row))
            }

            fn column_width(&self) -> ColumnWidth {
                self.relation.width
            }

            fn toggle(&self) -> ColumnToggle {
                self.relation.toggle
            }

            fn includes(&self) -> Includes<M> {
                self.relation.includes.clone()
            }

            fn reads(&self) -> Option<crate::lens::Reads<M>> {
                Some(self.relation.reads.clone())
            }

            fn internals(&self) -> &dyn crate::column::ColumnInternals<M> {
                self
            }
        }

        impl<M> crate::column::ColumnInternals<M> for $ty<M>
        where
            M: toasty::schema::Model + Send + Sync + 'static,
        {
            fn restrict<'a>(
                &'a self,
                cx: &'a Cx,
                rows: &'a [M],
                ex: &'a mut dyn Executor,
            ) -> BoxFuture<'a, Result<()>> {
                match &self.relation.restrict {
                    Some(restrict) => restrict(cx, rows, ex),
                    None => Box::pin(async { Ok(()) }),
                }
            }

            fn masked(&self, cx: &Cx) -> bool {
                self.relation
                    .masked
                    .as_ref()
                    .is_some_and(|masked| masked(cx))
            }

            fn related(&self) -> Related {
                self.relation.related
            }

            fn misdeclared(&self) -> Option<DeclarationErrorKind> {
                self.relation.misdeclared.clone()
            }

            fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
                self.relation
                    .source
                    .filter(|(_, available)| !available(cx))
                    .map(|(source, _)| source)
            }
        }
    };
}

relation_column!(RelationColumn);
relation_column!(CountColumn);
