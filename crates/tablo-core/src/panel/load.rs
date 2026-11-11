//! [`Panel::load`]: what a panel reads once for a request, ahead of every handler, for the
//! policies, queries and conditions that cannot await.

use std::{any::TypeId, future::Future, sync::Arc};

use topcoat::{
    Result,
    context::{Cx, try_context},
};

use super::{Panel, state::current};
use crate::BoxFuture;

/// A value a panel loads once per request, for what decides without awaiting: a
/// [`Policy`](crate::Policy), a [`ResourceDef::query_fn`](crate::ResourceDef::query_fn), a
/// condition on a column or an action. [`Panel::load`] registers it and [`loaded`] reads it.
///
/// A policy is asked once per row and once per button, so it reads what the request loaded
/// instead of querying each time:
///
/// ```rust
/// # use tablo_core::{Ability, Load, Panel, Policy, TenantId, db::db, loaded, tenant_id};
/// # use topcoat::{Result, context::Cx};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Project { #[key] #[auto] id: uuid::Uuid, name: String }
/// #[derive(Debug, toasty::Model)]
/// struct Subscription {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     team: TenantId,
///     projects: i64,
/// }
///
/// /// How many projects the request's team may hold.
/// struct Plan {
///     projects: i64,
/// }
///
/// impl Load for Plan {
///     async fn load(cx: &Cx) -> Result<Self> {
///         let Some(team) = tenant_id(cx) else {
///             return Ok(Self { projects: 0 });
///         };
///         let mut db = db(cx)?;
///         let subscriptions = Subscription::filter(Subscription::fields().team().eq(team))
///             .exec(&mut db)
///             .await?;
///         Ok(Self {
///             projects: subscriptions.first().map_or(1, |held| held.projects),
///         })
///     }
/// }
///
/// fn project_policy() -> impl Policy<Project> {
///     |cx: &Cx, ability: Ability<'_, Project>| match ability {
///         Ability::Create => loaded::<Plan>(cx).is_some_and(|plan| plan.projects > 0),
///         _ => true,
///     }
/// }
///
/// Panel::new("app").load::<Plan>();
/// ```
///
/// `load` runs after the [auth gate](crate::auth::guard) admitted the request, so it reads the
/// signed-in user with [`auth::user`](crate::auth::user) and the tenant the request acts for with
/// [`tenant_id`](crate::tenant_id). On a panel whose authentication is disabled it runs on every
/// request. It does not run for the pages a signed-out visitor reaches, such as the login page.
///
/// It runs on every request the panel answers, the runtime requests of its pages included: keep it
/// to what the panel's policies read, in one or two statements.
pub trait Load: Sized + Send + Sync + 'static {
    /// Reads the value for the request.
    ///
    /// # Errors
    ///
    /// An `Err` fails the request before any handler runs.
    fn load(cx: &Cx) -> impl Future<Output = Result<Self>> + Send;
}

/// The `T` the request's panel [loaded](Panel::load). `None` when the panel does not load `T`,
/// and where no request loaded it: a signed-out page, a route outside the panel, or a context
/// built for a job with [`PanelHandle::context`](crate::PanelHandle::context).
///
/// A policy reads `None` as a refusal: `loaded::<Plan>(cx).is_some_and(..)`.
pub fn loaded<T: Load>(cx: &Cx) -> Option<&T> {
    try_context::<Loaded<T>>(cx).map(|Loaded(value)| &**value)
}

/// A [`Load`] value as request `Cx` carries it.
struct Loaded<T>(Arc<T>);

impl<T> Clone for Loaded<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// One [`Load`] of a panel, its type erased: `cx` with the value it loaded.
#[derive(Clone, Copy)]
pub(crate) struct LoadFn {
    loads: TypeId,
    run: for<'a> fn(&'a Cx) -> BoxFuture<'a, Result<Cx>>,
}

fn erased<T: Load>(cx: &Cx) -> BoxFuture<'_, Result<Cx>> {
    Box::pin(async move { Ok(cx.with(Loaded(Arc::new(T::load(cx).await?)))) })
}

impl Panel {
    /// Loads the [`Load`] value `T` once for each request the panel admits, ahead of every
    /// handler, for [`loaded`] to answer. Several values load in the order they are registered,
    /// so one reads those before it; registering a type twice loads it once.
    pub fn load<T: Load>(mut self) -> Self {
        let loads = TypeId::of::<T>();
        if !self.loads.iter().any(|load| load.loads == loads) {
            self.loads.push(LoadFn {
                loads,
                run: erased::<T>,
            });
        }
        self
    }
}

/// `cx` with every value the request's panel loads, in registration order.
pub(crate) async fn load(cx: Cx) -> Result<Cx> {
    let Some(panel) = current(&cx).map(Arc::clone) else {
        return Ok(cx);
    };
    let mut cx = cx;
    for load in &panel.loads {
        cx = (load.run)(&cx).await?;
    }
    Ok(cx)
}
