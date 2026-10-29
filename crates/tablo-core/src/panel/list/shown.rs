//! The list state a request's list page shows, which [`list_query`] reads for its widgets.

use std::{
    any::TypeId,
    sync::{Arc, OnceLock},
};

use toasty::stmt::{List, Query};
use topcoat::{
    Result,
    context::{Cx, try_context},
    router::error::forbidden,
};

use crate::{panel::gate::gate, policy::Ability, resource::Resource, table::TableState};

/// The resource whose list page the request renders, and the state that page shows. The panel
/// puts an empty one on each request, and the list page fills it before its widgets render.
#[derive(Clone, Default)]
pub(crate) struct ShownList(Arc<OnceLock<(TypeId, TableState)>>);

impl ShownList {
    /// Record that the request renders `R`'s list in `state`. A request renders one list page,
    /// so a second record changes nothing.
    pub(crate) fn record<R: Resource>(cx: &Cx, state: &TableState) {
        if let Some(Self(shown)) = try_context::<Self>(cx) {
            let _ = shown.set((TypeId::of::<R>(), state.clone()));
        }
    }

    /// The state `R`'s list page shows in this request; `None` outside it.
    fn of<R: Resource>(cx: &Cx) -> Option<&TableState> {
        let Self(shown) = try_context::<Self>(cx)?;
        let (resource, state) = shown.get()?;
        (*resource == TypeId::of::<R>()).then_some(state)
    }
}

/// `R`'s records as its list shows them in this request: the [`scoped_query`](crate::scoped_query)
/// the list reads, narrowed by its tab, search and filters, unordered. A
/// [`list_widget`](crate::ResourceDef::list_widget) reading it shows figures that follow the list,
/// and reruns with it as the reader searches, filters or switches tabs.
///
/// Outside `R`'s list page, as on a dashboard, it reads the list's first tab with no search or
/// filter. Like the list, it leaves out a filter it cannot apply, and the trashed records unless
/// the list's trash control asks for them. Count what it selects with
/// [`db::count`](crate::db::count).
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, published: bool }
/// # struct PostResource;
/// # impl tablo_core::Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = tablo_core::NoForm<Post>;
/// #     fn declare() -> tablo_core::ResourceDef<Self> { tablo_core::ResourceDef::new() }
/// # }
/// use tablo_core::{Stat, Widget, db, list_query};
/// use topcoat::{Result, context::Cx, view::View};
///
/// struct Published;
///
/// impl Widget for Published {
///     async fn render(cx: &Cx) -> Result<impl View> {
///         let published = list_query::<PostResource>(cx)
///             .await?
///             .filter(Post::fields().published().eq(true));
///         let count = db::count(cx, published).await?;
///         Ok(tablo_core::stats(
///             cx,
///             vec![Stat::new("Published", count.to_string())],
///         ))
///     }
/// }
/// ```
///
/// A tab and a filter can ask one column for two values. Toasty then folds the query's filter to
/// `false`, and panics planning an `include` over it: count the query, or ask it for a row, before
/// loading its rows with their relations.
///
/// # Errors
///
/// 403 when the request may not open `R`'s list, the check [`can_list`](crate::can_list) makes,
/// which fails the whole page the widget is on: a widget on another page asks `can_list` first.
/// The database failing a filter's check.
pub async fn list_query<R: Resource>(cx: &Cx) -> Result<Query<List<R::Model>>> {
    let resource = gate::<R>(cx)?;
    if !resource.can(cx, Ability::ViewAny) {
        return Err(forbidden().into());
    }
    let table = resource.table_for(cx);
    let state = table.normalize_state(&ShownList::of::<R>(cx).cloned().unwrap_or_default());
    let refused = table.refused_filters(cx, &state).await?;
    let query = resource.list_query(cx, &state)?;
    Ok(table.narrow(cx, &crate::db::db(cx)?, query, &state, &refused))
}
