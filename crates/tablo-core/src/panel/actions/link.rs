//! A many-to-many relation's writes, which its table on the owner's detail page posts to the
//! owner's routes under `{list}/{id}/-/relations/{relation}`, `{relation}` the related resource's
//! slug:
//!
//! - `POST …/-/actions/attach` links the record its input names;
//! - `POST …/-/actions/detach` unlinks the selection its `ids` carry, and `POST
//!   …/{related}/-/actions/detach` one row's record;
//! - `GET …/-/actions/attach/options` searches the input's choice.
//!
//! Each asks of the owner what its edit asks: `View` and `Update`, on the record loaded through
//! the tenant-scoped query inside the write's transaction. The related records answer to their own
//! resource: the input's choice offers, and the write re-checks, only the records its policy lets
//! the user view in the request's tenant, and a detach refuses a record it does not.

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{bad_request, forbidden, not_found, see_other},
        path_param_segment,
    },
    view::BoxView,
};

use super::{
    super::{
        forms::{FormChrome, parse_form_body},
        gate::{gate, landing_url},
        relations::{ATTACH, DETACH, linked_relation},
        write::commit_write,
    },
    fetch::find_by_key,
    mutation::{InputPage, MAX_BULK_IDS, Pending, parse_bulk_ids, read_input},
    options::input_options,
};
use crate::{
    db::db,
    notification::{Notification, set_notification},
    policy::Ability,
    resource::{Committed, Links, Mounted, Resource},
    table::{action_options_url, relation_url},
    topcoat_compat::async_page,
};

/// `POST {list}/{id}/-/relations/{relation}/-/actions/{action}`: the attach dialog's submission,
/// or the bulk bar's Detach with the selection in `ids`.
pub(crate) fn relation_list_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let links = linked_relation(cx, &resource).ok_or_else(not_found)?;
        match path_param_segment(cx, "action") {
            ATTACH => attach(cx, &resource, links, body).await,
            DETACH => detach(cx, &resource, links, body, None).await,
            _ => Err(not_found().into()),
        }
    })
}

/// `POST {list}/{id}/-/relations/{relation}/{related}/-/actions/detach`: one row's Detach.
pub(crate) fn relation_record_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let links = linked_relation(cx, &resource).ok_or_else(not_found)?;
        if path_param_segment(cx, "action") != DETACH {
            return Err(not_found().into());
        }
        let related = path_param_segment(cx, "related").to_string();
        detach(cx, &resource, links, body, Some(related)).await
    })
}

/// `GET {list}/{id}/-/relations/{relation}/-/actions/attach/options`: the attach dialog's option
/// search, for a user who may attach.
pub(crate) fn relation_action_options<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        let links = linked_relation(cx, &resource).ok_or_else(not_found)?;
        if path_param_segment(cx, "action") != ATTACH {
            return Err(not_found().into());
        }
        owner(cx, &resource, &mut db(cx)).await?;
        input_options(cx, links.input).await
    })
}

/// The owner the `{id}` path segment names, loaded through `ex`: 404 unless it is in scope, 403
/// unless the user may view and update it.
async fn owner<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    ex: &mut dyn toasty::Executor,
) -> topcoat::Result<R::Model> {
    let id = path_param_segment(cx, "id").to_string();
    let owner = find_by_key(cx, resource, &id, ex).await?;
    if !resource.can(cx, Ability::View(&owner)) || !resource.can(cx, Ability::Update(&owner)) {
        return Err(forbidden().into());
    }
    Ok(owner)
}

/// Links the record the attach dialog chose, or renders the dialog's input as a page with what
/// refused it.
async fn attach<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    links: &Links<R::Model>,
    body: Body,
) -> Result<BoxView<'a>, topcoat::Error> {
    let parts = parse_form_body(cx, body).await?;
    crate::csrf::verify(cx, &parts.values)?;
    // An owner out of scope or closed to the user answers before the input is checked.
    owner(cx, resource, &mut db(cx)).await?;
    let pending = read_input(cx, ATTACH, &links.input, &parts.values, &parts.lists).await?;
    let id = path_param_segment(cx, "id").to_string();
    let home = format!(
        "{}/{}",
        resource.url,
        crate::topcoat_compat::href::encode_path_segment(&id)
    );
    let chrome = || {
        let url = relation_url(&resource.url, &id, path_param_segment(cx, "relation"));
        FormChrome::action(
            "Attach".to_string(),
            "Attach".to_string(),
            false,
            Vec::new(),
            landing_url(cx, &home),
            action_options_url(&url, ATTACH),
        )
    };
    let (input, values) = match pending {
        Pending::Submitted { input, values } => (input, values),
        Pending::Page(page) => return page.render(cx, &links.input, chrome()).await,
        Pending::Ready(_) => return Err(bad_request("attach takes a record").into()),
    };
    let mut db = db(cx);
    let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
    // The authoritative load observes the write's snapshot.
    let owner = owner(cx, resource, &mut tx).await?;
    // A record deleted, moved to another tenant or hidden since the input validated is refused.
    let errors = (links.input.schema)()
        .recheck_relationships(cx, &values, &mut tx)
        .await;
    if !errors.is_empty() {
        // The page's choices may query: release the connection first.
        drop(tx);
        return InputPage { values, errors }
            .render(cx, &links.input, chrome())
            .await;
    }
    let key = *input
        .downcast::<String>()
        .expect("the attach input parses the key it posts");
    let written = (links.attach)(cx, &owner, key, &mut tx)
        .await
        .map(|()| owner);
    commit_write(
        cx,
        resource,
        tx,
        written,
        Committed::attached,
        "Attached",
        "attach the record",
    )
    .await
}

/// Unlinks `related`, or the selection the POST's `ids` carry.
async fn detach<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    links: &Links<R::Model>,
    body: Body,
    related: Option<String>,
) -> Result<BoxView<'a>, topcoat::Error> {
    let values = parse_form_body(cx, body).await?.values;
    crate::csrf::verify(cx, &values)?;
    let keys = match related {
        Some(key) => vec![key],
        None => {
            let ids = parse_bulk_ids(values.get("ids").map_or("", String::as_str), MAX_BULK_IDS);
            if ids.is_empty() {
                set_notification(cx, Notification::error("Select at least one row first"));
                return Err(see_other(landing_url(cx, &resource.url)).into());
            }
            if ids.len() > MAX_BULK_IDS {
                return Err(bad_request(format!("too many ids (max {MAX_BULK_IDS})")).into());
            }
            ids
        }
    };
    let mut db = db(cx);
    let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
    let owner = owner(cx, resource, &mut tx).await?;
    let count = keys.len();
    let written = (links.detach)(cx, &owner, keys, &mut tx)
        .await
        .map(|()| owner);
    let noun = if count == 1 { "record" } else { "records" };
    commit_write(
        cx,
        resource,
        tx,
        written,
        Committed::detached,
        format!("Detached {count} {noun}"),
        "detach the records",
    )
    .await
}
