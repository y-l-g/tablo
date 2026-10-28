//! The policy's view of an action: matching an [`Ability`] against an action type, and the
//! [`Abilities`](crate::policy::Abilities) builder's rules for one.

use topcoat::context::Cx;

use super::Action;
use crate::{policy::Ability, resource::Resource};

impl<M> Ability<'_, M> {
    /// Whether the ability is [`RunAny`](Ability::RunAny) or [`Run`](Ability::Run) for the action
    /// `A`.
    ///
    /// It compares against `A::NAME`, so renaming the action keeps the policy matching it, and a
    /// type that is not an action of the resource does not compile. `R` is inferred when `A` is an
    /// action of one resource.
    ///
    /// ```rust
    /// # use tablo_core::{Ability, Action, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, published: bool }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// # struct Publish;
    /// # impl Action<PostResource> for Publish {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "publish";
    /// #     async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
    /// # }
    /// # fn is_editor(_cx: &Cx) -> bool { true }
    /// fn post_policy(cx: &Cx, ability: Ability<'_, Post>) -> bool {
    ///     match ability {
    ///         Ability::Run { record, .. } if ability.is_action::<Publish, _>() => !record.published,
    ///         Ability::ViewAny | Ability::View(_) => true,
    ///         _ => is_editor(cx),
    ///     }
    /// }
    /// ```
    pub fn is_action<A, R>(self) -> bool
    where
        A: Action<R>,
        R: Resource<Model = M>,
    {
        matches!(self, Ability::RunAny { action } | Ability::Run { action, .. } if action == A::NAME)
    }
}

impl<M> crate::policy::Abilities<M> {
    /// Who may run the record action `A` on a record: [`Run`](Ability::Run) with `A`'s
    /// [`NAME`](Action::NAME). It also allows [`RunAny`](Ability::RunAny) for `A`, which
    /// [`run_any`](Self::run_any) narrows. An action no `run` declares is refused.
    ///
    /// ```rust
    /// # use tablo_core::{Abilities, Action, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, published: bool }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// # struct Publish;
    /// # impl Action<PostResource> for Publish {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "publish";
    /// #     async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
    /// # }
    /// let policy = Abilities::<Post>::new()
    ///     .view_any(|_cx| true)
    ///     .view(|_cx, _post| true)
    ///     .run::<Publish, _>(|_cx, post| !post.published);
    /// # let _ = policy;
    /// ```
    #[must_use]
    pub fn run<A, R>(mut self, check: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self
    where
        A: Action<R>,
        R: Resource<Model = M>,
    {
        self.set_action_record(A::NAME, Box::new(check));
        self
    }

    /// Who may run the record action `A` at all: [`RunAny`](Ability::RunAny) with `A`'s
    /// [`NAME`](Action::NAME). Defaults to whether [`run`](Self::run) declares `A`.
    #[must_use]
    pub fn run_any<A, R>(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self
    where
        A: Action<R>,
        R: Resource<Model = M>,
    {
        self.set_action_any(A::NAME, Box::new(check));
        self
    }
}
