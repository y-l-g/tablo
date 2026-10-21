//! Authorizes resource abilities, denying by default.
//!
//! Combines [`Allow`], [`Deny`], [`ReadOnly`], [`when`], [`Abilities`] and closures with
//! `and`/`or`.
//!
//! ```rust
//! # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
//! # struct Post { #[key] #[auto] id: uuid::Uuid, locked: bool }
//! # use tablo_core::{Ability, Policy, when};
//! # use topcoat::context::Cx;
//! # fn not_suspended(cx: &Cx) -> bool { true }
//! fn post_policy() -> impl Policy<Post> {
//!     // No write of any kind to a locked post: an update, a delete, a restore or an action.
//!     when(not_suspended).and(|_cx: &Cx, ability: Ability<'_, Post>| {
//!         ability.written_record().is_none_or(|post| !post.locked)
//!     })
//! }
//! ```
//!
//! A closure ending in `_ => true` allows every ability it does not name, the ones a resource
//! gains later included. Match every variant so that a new one fails to compile, read
//! [`Ability::written_record`] to refuse writes by the record, or build with [`Abilities`], which
//! denies what it does not declare.
//!
//! [`can`](crate::can) asks a resource's policy from app code, and [`can_list`](crate::can_list)
//! answers whether the current request may open a resource's list at all.

use derive_where::derive_where;
use topcoat::context::Cx;

mod abilities;

pub use abilities::Abilities;

/// One thing a policy is asked to allow; record abilities are asked once per
/// loaded row, and a record that cannot be viewed cannot be written by guessing
/// its key.
#[derive(Debug)]
#[derive_where(Clone, Copy)]
pub enum Ability<'a, M> {
    /// Open the list, export it, and offer the records as relationship options.
    ViewAny,
    /// See one record.
    View(&'a M),
    /// Open the create form and submit it.
    Create,
    /// Edit one record.
    Update(&'a M),
    /// Delete at all.
    DeleteAny,
    /// Delete one record.
    Delete(&'a M),
    /// Restore trashed records at all, on a resource that
    /// [soft-deletes](crate::ResourceDef::soft_delete).
    RestoreAny,
    /// Restore one trashed record.
    Restore(&'a M),
    /// Delete trashed records for good at all, on a resource that
    /// [soft-deletes](crate::ResourceDef::soft_delete). Allowed only with
    /// [`DeleteAny`](Self::DeleteAny).
    ForceDeleteAny,
    /// Delete one trashed record for good. Allowed only with [`Delete`](Self::Delete) of it, so a
    /// policy refusing a record's delete refuses its force delete too.
    ForceDelete(&'a M),
    /// Run the custom [`Action`](crate::Action) whose [`NAME`](crate::Action::NAME) is `action`
    /// at all, before [`Run`](Self::Run) is asked of each record.
    RunAny {
        /// The action's [`NAME`](crate::Action::NAME).
        action: &'static str,
    },
    /// Run the custom [`Action`](crate::Action) whose [`NAME`](crate::Action::NAME) is `action`
    /// on one record.
    Run {
        /// The action's [`NAME`](crate::Action::NAME).
        action: &'static str,
        /// The record it runs on.
        record: &'a M,
    },
    /// Run the [`HeaderAction`](crate::HeaderAction) whose `NAME` is `action` from the list's
    /// header. It acts on no record, so nothing else of the policy is asked but `ViewAny`.
    RunHeader {
        /// The action's [`NAME`](crate::HeaderAction::NAME).
        action: &'static str,
    },
}

impl<'a, M> Ability<'a, M> {
    /// The record the ability names, if any.
    pub fn record(self) -> Option<&'a M> {
        match self {
            Self::View(record)
            | Self::Update(record)
            | Self::Delete(record)
            | Self::Restore(record)
            | Self::ForceDelete(record)
            | Self::Run { record, .. } => Some(record),
            Self::ViewAny
            | Self::Create
            | Self::DeleteAny
            | Self::RestoreAny
            | Self::ForceDeleteAny
            | Self::RunAny { .. }
            | Self::RunHeader { .. } => None,
        }
    }

    /// Whether the ability only reads.
    pub fn is_read(self) -> bool {
        matches!(self, Self::ViewAny | Self::View(_))
    }

    /// The record the ability writes: an update, a delete, a restore, a force delete or an
    /// action's run on it. `None` for a read and for an ability over no record.
    ///
    /// A policy refusing every write to some records reads it, and so refuses the writes a
    /// resource gains later too:
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
    /// # use tablo_core::Ability;
    /// # use topcoat::context::Cx;
    /// fn keep_root(_cx: &Cx, ability: Ability<'_, User>) -> bool {
    ///     ability
    ///         .written_record()
    ///         .is_none_or(|user| user.name != "root")
    /// }
    /// ```
    pub fn written_record(self) -> Option<&'a M> {
        match self {
            Self::Update(record)
            | Self::Delete(record)
            | Self::Restore(record)
            | Self::ForceDelete(record)
            | Self::Run { record, .. } => Some(record),
            Self::ViewAny
            | Self::View(_)
            | Self::Create
            | Self::DeleteAny
            | Self::RestoreAny
            | Self::ForceDeleteAny
            | Self::RunAny { .. }
            | Self::RunHeader { .. } => None,
        }
    }
}

/// Decides which [`Ability`]s the current user has over a resource's records.
///
/// A closure `|cx: &Cx, ability: Ability<'_, M>| -> bool` is a policy, and the
/// building blocks combine with `and` and `or`.
/// [`ResourceDef::policy_fn`](crate::ResourceDef::policy_fn) takes a closure without naming its
/// argument types.
pub trait Policy<M>: Send + Sync + 'static {
    /// Whether the current user may do `ability`.
    fn allows(&self, cx: &Cx, ability: Ability<'_, M>) -> bool;
}

macro_rules! combinators {
    ($($ty:ident $(<$($param:ident),+>)?),+ $(,)?) => {$(
        impl$(<$($param),+>)? $ty$(<$($param),+>)? {
            /// Allows what both `self` and `other` allow.
            pub fn and<P>(self, other: P) -> And<Self, P> {
                And(self, other)
            }

            /// Allows what either `self` or `other` allows.
            pub fn or<P>(self, other: P) -> Or<Self, P> {
                Or(self, other)
            }
        }
    )+};
}

combinators!(Allow, Deny, ReadOnly, When<F>, And<A, B>, Or<A, B>, Abilities<M>);

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

/// Allows listing and viewing, and no write: no create, update, delete or custom action.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOnly;

impl<M> Policy<M> for ReadOnly {
    fn allows(&self, _cx: &Cx, ability: Ability<'_, M>) -> bool {
        ability.is_read()
    }
}

/// Allows every ability while `predicate` holds for the request.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff { #[key] #[auto] id: uuid::Uuid, editor: bool }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid }
/// # use tablo_core::{PanelUser, Policy, ReadOnly, auth, when};
/// # use topcoat::context::Cx;
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { String::new() }
/// #     fn display_name(&self) -> &str { "" }
/// # }
/// fn editor(cx: &Cx) -> bool {
///     auth::user::<Staff>(cx).is_some_and(|staff| staff.editor)
/// }
///
/// fn post_policy() -> impl Policy<Post> {
///     ReadOnly.or(when(editor))
/// }
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

#[cfg(test)]
mod tests;
