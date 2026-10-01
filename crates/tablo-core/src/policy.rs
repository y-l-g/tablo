//! Authorization: what a resource lets the current user do.
//!
//! A [`Resource`] answers every check through one value, its
//! [`policy`](Resource::policy). A policy is asked one [`Ability`] at a time —
//! listing, viewing a record, creating, updating a record, deleting at all,
//! deleting a record — and answers `true` to allow it. The default policy is
//! [`Deny`], so a resource that declares none exposes no data and no mutation.
//!
//! Policies compose. [`Allow`], [`Deny`], [`ReadOnly`] and [`when`] are the
//! building blocks, each with `and` and `or` to combine it with another
//! policy, and a closure over the context and the ability is a policy too:
//!
//! ```ignore
//! fn policy() -> impl Policy<Post> {
//!     when(not_suspended).and(|_cx: &Cx, ability: Ability<'_, Post>| match ability {
//!         Ability::Update(post) | Ability::Delete(post) => !post.locked,
//!         _ => true,
//!     })
//! }
//! ```
//!
//! [`can`] asks a resource's policy from app code, and [`can_list`] answers
//! whether the current request may open a resource's list at all.

use topcoat::context::Cx;

use crate::resource::Resource;

/// One thing a policy is asked to allow.
///
/// The record-free abilities decide what a page offers before any row loads;
/// the record abilities are asked once per loaded row. The panel asks
/// [`DeleteAny`](Self::DeleteAny) before [`Delete`](Self::Delete), and
/// [`View`](Self::View) together with [`Update`](Self::Update) and
/// [`Delete`](Self::Delete), so a record that cannot be viewed cannot be
/// written by guessing its key.
#[derive(Debug)]
pub enum Ability<'a, M> {
    /// Open the list, export it, and offer the records as relationship
    /// options. Asked before any row loads, so it cannot read one.
    ViewAny,
    /// See one record: its detail and edit pages, its export row, its
    /// relationship option, its row actions.
    View(&'a M),
    /// Open the create form and submit it.
    Create,
    /// Edit one record.
    Update(&'a M),
    /// Delete at all: decides whether the list renders the delete controls,
    /// and gates the delete routes before any record loads.
    DeleteAny,
    /// Delete one record.
    Delete(&'a M),
}

impl<M> Clone for Ability<'_, M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for Ability<'_, M> {}

impl<'a, M> Ability<'a, M> {
    /// The record the ability is asked about, when it names one.
    pub fn record(self) -> Option<&'a M> {
        match self {
            Self::View(record) | Self::Update(record) | Self::Delete(record) => Some(record),
            Self::ViewAny | Self::Create | Self::DeleteAny => None,
        }
    }

    /// Whether the ability only reads: [`ViewAny`](Self::ViewAny) or
    /// [`View`](Self::View).
    pub fn is_read(self) -> bool {
        matches!(self, Self::ViewAny | Self::View(_))
    }
}

/// Decides which [`Ability`]s the current user has over a resource's records.
///
/// A closure `|cx: &Cx, ability: Ability<'_, M>| -> bool` is a policy, so a
/// resource with one rule per ability matches on the ability. The building
/// blocks in this module combine with their `and` and `or` methods; they are
/// inherent rather than trait methods because a block such as [`Allow`] is a
/// policy over every model, and a trait method would leave the model to
/// infer.
pub trait Policy<M>: Send + Sync + 'static {
    /// Whether the current user may do `ability`.
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool;
}

/// `and` and `or` on a building block.
macro_rules! combinators {
    ($($ty:ident $(<$($param:ident),+>)?),+ $(,)?) => {$(
        impl$(<$($param),+>)? $ty$(<$($param),+>)? {
            /// A policy that allows what both `self` and `other` allow.
            pub fn and<P>(self, other: P) -> And<Self, P> {
                And(self, other)
            }

            /// A policy that allows what either `self` or `other` allows.
            pub fn or<P>(self, other: P) -> Or<Self, P> {
                Or(self, other)
            }
        }
    )+};
}

combinators!(Allow, Deny, ReadOnly, When<F>, And<A, B>, Or<A, B>);

impl<M, F> Policy<M> for F
where
    F: Fn(&Cx, Ability<'_, M>) -> bool + Send + Sync + 'static,
{
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool {
        self(cx, ability)
    }
}

/// Allows every ability.
#[derive(Debug, Clone, Copy, Default)]
pub struct Allow;

impl<M> Policy<M> for Allow {
    fn allows(&self, _cx: &Cx, _ability: Ability<'_, M>) -> bool {
        true
    }
}

/// Allows nothing: the default policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct Deny;

impl<M> Policy<M> for Deny {
    fn allows(&self, _cx: &Cx, _ability: Ability<'_, M>) -> bool {
        false
    }
}

/// Allows listing and viewing, and no write.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOnly;

impl<M> Policy<M> for ReadOnly {
    fn allows(&self, _cx: &Cx, ability: Ability<'_, M>) -> bool {
        ability.is_read()
    }
}

/// A policy that allows every ability while `predicate` holds for the request.
///
/// The building block for a rule about the request rather than a record —
/// the user, the tenant — which applies to every ability alike:
///
/// ```ignore
/// fn editor(cx: &Cx) -> bool {
///     auth::user::<Staff>(cx).is_some_and(|staff| staff.editor)
/// }
///
/// fn policy() -> impl Policy<Post> { ReadOnly.or(when(editor)) }
/// ```
pub fn when<F>(predicate: F) -> When<F>
where
    F: Fn(&Cx) -> bool + Send + Sync + 'static,
{
    When(predicate)
}

/// The policy [`when`] returns.
#[derive(Debug, Clone, Copy)]
pub struct When<F>(F);

impl<M, F> Policy<M> for When<F>
where
    F: Fn(&Cx) -> bool + Send + Sync + 'static,
{
    fn allows(&self, cx: &Cx, _ability: Ability<'_, M>) -> bool {
        (self.0)(cx)
    }
}

/// The policy `and` returns: allows what both sides allow.
#[derive(Debug, Clone, Copy)]
pub struct And<A, B>(A, B);

impl<M, A: Policy<M>, B: Policy<M>> Policy<M> for And<A, B> {
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool {
        self.0.allows(cx, ability) && self.1.allows(cx, ability)
    }
}

/// The policy `or` returns: allows what either side allows.
#[derive(Debug, Clone, Copy)]
pub struct Or<A, B>(A, B);

impl<M, A: Policy<M>, B: Policy<M>> Policy<M> for Or<A, B> {
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool {
        self.0.allows(cx, ability) || self.1.allows(cx, ability)
    }
}

/// Whether `R`'s policy allows `ability` for the current request.
///
/// The question every panel handler asks, for app code that renders or writes
/// `R`'s records itself. It does not check the panel's sign-in or `R`'s tenant
/// requirement: [`can_list`] adds both for the list.
pub fn can<R: Resource>(cx: &Cx, ability: Ability<'_, R::Model>) -> bool {
    R::policy().allows(cx, ability)
}

/// Whether the current request may open `R`'s list: the panel's sign-in when
/// it requires one, a tenant when `R` is tenant-scoped, and
/// [`Ability::ViewAny`].
///
/// The list handler refuses exactly what this refuses, so a page that links to
/// a list — a dashboard tile — checks this rather than restating the three.
pub fn can_list<R: Resource>(cx: &Cx) -> bool {
    crate::panel::gate::gate::<R>(cx).is_ok() && can::<R>(cx, Ability::ViewAny)
}

#[cfg(test)]
mod tests;
