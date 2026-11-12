//! The header actions a [`ResourceDef`] declares, each with the badge its button may show.

use toasty::stmt::Expr;

use crate::{HeaderAction, Resource, ResourceDef};

impl<R: Resource> ResourceDef<R> {
    /// Adds the [`HeaderAction`] `A`, which acts on no record, to the list page's header, after
    /// the ones already declared.
    ///
    /// The policy decides who may run it, through [`Ability::ViewAny`](crate::Ability::ViewAny)
    /// and [`Ability::RunHeader`](crate::Ability::RunHeader) with its name, which no other action
    /// of the resource may share. An `A::NAME` that is not a single URL path segment does not
    /// compile.
    ///
    /// ```rust,compile_fail,E0080
    /// # use tablo_core::{HeaderAction, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// struct RecountTags;
    ///
    /// impl HeaderAction for RecountTags {
    ///     type Input = ();
    ///     const NAME: &'static str = "recount/tags";
    ///
    ///     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError<std::convert::Infallible>> {
    ///         Ok(tablo_core::Outcome::done())
    ///     }
    /// }
    ///
    /// let _def = ResourceDef::<PostResource>::new().header_action::<RecountTags>();
    /// ```
    #[must_use]
    pub fn header_action<A: HeaderAction>(mut self) -> Self {
        self.header_actions = self.header_actions.add::<A>();
        self
    }

    /// Declares the header action `A`, as [`header_action`](Self::header_action) does, and badges
    /// its button with the number of rows of [`scoped_query`](crate::scoped_query) that `filter`
    /// matches, as [`navigation_badge`](Self::navigation_badge) badges the sidebar. The count runs
    /// each time the list renders, and a zero shows no badge.
    ///
    /// ```rust
    /// # use tablo_core::{HeaderAction, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, draft: bool }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// # struct PublishDrafts;
    /// # impl HeaderAction for PublishDrafts {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "publish-drafts";
    /// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError<std::convert::Infallible>> {
    /// #         Ok(tablo_core::Outcome::done())
    /// #     }
    /// # }
    /// // "Publish drafts 3" while three drafts wait.
    /// let _def = ResourceDef::<PostResource>::new()
    ///     .header_action_with_badge::<PublishDrafts>(Post::fields().draft().eq(true));
    /// ```
    #[must_use]
    pub fn header_action_with_badge<A: HeaderAction>(mut self, filter: Expr<bool>) -> Self {
        self.header_badges.push((A::NAME, filter));
        self.header_action::<A>()
    }
}
