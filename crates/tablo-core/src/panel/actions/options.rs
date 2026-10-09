//! Relationship option search endpoint (D2/D5):
//! `GET {parent_list_url}/options?field=&q=&parent=` for tables above the option cap and for
//! dependent choices, and `GET {url}/-/actions/{name}/options?field=&q=` for an action's input.

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{forbidden, not_found},
        path_param_segment,
    },
    view::*,
};

use super::{super::gate::gate, mutation::header_allowed};
use crate::{
    error::TabloError,
    query_term::clamp_query_term,
    resource::{InputSpec, Resource},
    schema::{OptionLoadError, Schema, option_view},
};

/// Relationship option search endpoint (D2/D5).
///
/// `GET {parent_list_url}/options?field=&q=&parent=` — server-side narrowing for
/// tables above the option cap, and a dependent choice's options for its parent's value.
/// `field` allow-lists to a declared relationship choice in the resource's form that is
/// searchable or dependent (400 otherwise); a non-searchable, independent choice keeps the cap
/// error and never calls here. `q` is trimmed and clamped to the shared query bound; empty `q`
/// returns the bounded head. `parent` is the dependent choice's parent value: a blank one
/// answers no option.
///
/// Gates: `auth::guard` + `enforce_tenant` (parent), then the related
/// gates inside the search (`ViewAny` + tenant + `View` filtering
/// before labels). Parent form policy (`Create` / `View`+`Update`)
/// stays on the form pages themselves: requiring parent `ViewAny` here
/// would lock create-only users out of a form they may use, and adds no
/// visibility the related list does not already expose. `Denied` → 403, driver failure → 500,
/// filtered overflow → 200 with a "keep typing" hint option (client keeps its hint element).
/// Success → 200 `text/html` with `<option>` markup, bounded to
/// `MAX_RELATIONSHIP_OPTIONS`, values are typed PK strings, labels escaped.
pub(crate) fn resource_options<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        options_response(cx, &resource.form).await
    })
}

/// The option search of a resource action's input: `GET {list}/-/actions/{name}/options`, for
/// a header action, or a custom action on any place.
///
/// Asks what the action's POST asks before it loads a record: a header action's list checks, or
/// a custom action's resource-wide ability. The search then gates the related rows as the form's
/// own route does.
pub(crate) fn resource_action_options<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        let name = path_param_segment(cx, "action");
        let input = if let Some(header) = resource.header_actions.find(name) {
            if !header_allowed(cx, &resource, header) {
                return Err(forbidden().into());
            }
            header.input
        } else if let Some(action) = resource.actions.find(name) {
            if !resource.can(cx, action.resource_wide) {
                return Err(forbidden().into());
            }
            action.input
        } else {
            return Err(not_found().into());
        };
        input_options(cx, input).await
    })
}

/// Answers the option search of the input `spec` of an action.
pub(crate) async fn input_options(
    cx: &Cx,
    spec: InputSpec,
) -> topcoat::Result<http::Response<Body>> {
    options_response(cx, &(spec.schema)()).await
}

/// Answers the option search for the relationship field of `form` the query names.
async fn options_response(cx: &Cx, form: &Schema) -> topcoat::Result<http::Response<Body>> {
    let OptionsQuery { field, q, parent } = options_query(cx);
    let field = field.trim();
    if field.is_empty() {
        return Err(topcoat::router::error::bad_request("missing field").into());
    }
    let q = clamp_query_term(&q);
    let Some(select) = form
        .fields()
        .find(|declared| declared.name() == field)
        .and_then(|declared| declared.as_choice())
    else {
        return Err(topcoat::router::error::bad_request("unknown field").into());
    };
    if !select.is_relationship() {
        return Err(topcoat::router::error::bad_request("not a relationship select").into());
    }
    // A multiple choice filters the options it rendered, and fetches none.
    if select.is_multiple() || (!select.is_searchable() && select.parent_key().is_none()) {
        return Err(topcoat::router::error::bad_request("not searchable").into());
    }
    match select.search_options(cx, &q, Some(&parent)).await {
        Ok(opts) => {
            let options: Vec<_> = opts
                .into_iter()
                .map(|(value, label)| option_view(cx, value, label, false))
                .collect();
            let html = view! {
                cx =>
                for option in options {
                    (option)
                }
            }
            .single()
            .await?
            .render(cx);
            let res = http::Response::builder()
                .status(200)
                .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header("x-content-type-options", "nosniff")
                .body(Body::from(html))?;
            Ok(res)
        }
        Err(OptionLoadError::Denied) => Err(forbidden().into()),
        Err(OptionLoadError::LoadFailed) => {
            Err(TabloError::Infrastructure("option search failed").into())
        }
        // Permanent: the related resource cannot be scoped at
        // all, so the search cannot succeed until the declaration is
        // fixed — a 500 that says so, not a retry.
        Err(OptionLoadError::Misdeclared) => Err(TabloError::Declaration(
            "option search unavailable: the related resource requires a tenant the framework \
             cannot scope"
                .to_string(),
        )
        .into()),
        // The header tells a dependent choice's script to search the server from now on.
        Err(OptionLoadError::Overflow) => {
            let html =
                "<option value=\"\" disabled>Too many results — keep typing</option>".to_string();
            let res = http::Response::builder()
                .status(200)
                .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header("x-content-type-options", "nosniff")
                .header("x-options-overflow", "true")
                .body(Body::from(html))?;
            Ok(res)
        }
    }
}

/// The options endpoint's query parameters.
#[derive(Default)]
struct OptionsQuery {
    field: String,
    q: String,
    parent: String,
}

/// Parse `?field=`, `?q=` and `?parent=` for the options endpoint (first-wins, like the table
/// state parser).
fn options_query(cx: &Cx) -> OptionsQuery {
    let mut out = OptionsQuery::default();
    let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
        return out;
    };
    let query = parts.uri.query().unwrap_or("");
    let (mut seen_field, mut seen_q, mut seen_parent) = (false, false, false);
    for (k, v) in form_urlencoded::parse(query.as_bytes()) {
        let (slot, seen) = match k.as_ref() {
            "field" => (&mut out.field, &mut seen_field),
            "q" => (&mut out.q, &mut seen_q),
            "parent" => (&mut out.parent, &mut seen_parent),
            _ => continue,
        };
        if !*seen {
            *slot = v.into_owned();
            *seen = true;
        }
    }
    out
}

#[cfg(test)]
mod tests;
