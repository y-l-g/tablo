//! A schema rendered with steps it does not declare, such as a related form's, walked after its
//! wizard's own.

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::{
    ExtraStep, Schema, Source,
    tree::{Node, render_nodes},
};

impl<F> Schema<F> {
    /// Renders the schema as [`render`](Self::render) does, walking `steps` after the steps of
    /// its wizard: one at the top level that no condition hides. Without one, `steps` come back
    /// for the caller to render after the schema.
    ///
    /// # Errors
    ///
    /// A misdeclared schema fails with its errors rather than render.
    pub(crate) async fn render_with_steps<'a>(
        &self,
        cx: &'a Cx,
        source: Source<'_>,
        steps: Vec<ExtraStep<'a>>,
    ) -> Result<(BoxView<'a>, Vec<ExtraStep<'a>>)> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let source = source.watching(self.watched());
        let at = self.nodes.iter().position(|node| match node {
            Node::Wizard(wizard) => {
                wizard.condition.is_none() && node.shows_any(&self.fields, &source)
            }
            _ => false,
        });
        let Some(at) = at.filter(|_| !steps.is_empty()) else {
            let view = render_nodes(cx, &self.nodes, &self.fields, &source).await?;
            return Ok((view, steps));
        };
        let mut steps = Some(steps);
        let mut views = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            views.push(match (node, steps.take_if(|_| index == at)) {
                (Node::Wizard(wizard), Some(steps)) => {
                    Box::pin(wizard.render_with(cx, &self.fields, &source, steps)).await?
                }
                _ => node.render(cx, &self.fields, &source).await?,
            });
        }
        let view = view! {
            for view in views {
                (view)
            }
        }
        .boxed();
        Ok((view, Vec::new()))
    }
}
