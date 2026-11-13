//! A schema rendered as a table repeater's row: each top-level block in a cell of its own, under
//! the header its field gives.

use topcoat::{Result, context::Cx, view::BoxView};

use super::{Schema, Source, tree::Node};

impl<F> Schema<F> {
    /// Renders each top-level block of the schema apart, from `source`: a table repeater's cells.
    ///
    /// # Errors
    ///
    /// A misdeclared schema fails with its errors rather than render.
    pub(crate) async fn render_cells<'a>(
        &self,
        cx: &'a Cx,
        source: Source<'_>,
    ) -> Result<Vec<BoxView<'a>>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let source = source.watching(self.watched());
        let mut cells = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            cells.push(node.render(cx, &self.fields, &source).await?);
        }
        Ok(cells)
    }

    /// The header of each cell [`render_cells`](Self::render_cells) renders: a field's label and
    /// whether it is required, nothing for a block.
    pub(crate) fn cell_labels(&self) -> Vec<(crate::Text, bool)> {
        self.nodes
            .iter()
            .map(|node| match node {
                Node::Field(index) => {
                    let field = &self.fields[*index];
                    (field.label(), field.is_required())
                }
                _ => (crate::Text::from(""), false),
            })
            .collect()
    }
}
