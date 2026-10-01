//! Relationship option search endpoint (D2/D5):
//! `GET {parent_list_url}/options?field=&q=` for tables above the option cap.

use topcoat::{
    context::Cx,
    router::{Body, RouteFuture, error::forbidden},
    view::*,
};

use super::super::gate::gate;
use crate::{
    error::TabloError,
    resource::{Resource, clamp_query_term, declared},
    schema::{OptionLoadError, option_view},
};

/// Relationship option search endpoint (D2/D5).
///
/// `GET {parent_list_url}/options?field=&q=` — server-side narrowing for
/// tables above the option cap. `field` allow-lists to a declared searchable
/// relationship choice in `Resource::form` (400 otherwise); a non-searchable
/// choice keeps the cap error and never calls here. `q` is trimmed and
/// clamped to the shared query bound; empty `q` returns the bounded head.
///
/// Gates: `auth::guard` + `enforce_tenant::<R>` (parent), then the related
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
        gate::<R>(cx)?;
        let (field, q) = options_query(cx);
        let field = field.trim();
        if field.is_empty() {
            return Err(topcoat::router::error::bad_request("missing field").into());
        }
        let q = clamp_query_term(&q);
        let declared = declared::<R>(cx);
        let form = &declared.form;
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
        if !select.is_searchable() {
            return Err(topcoat::router::error::bad_request("not searchable").into());
        }
        match select.search_options(cx, &q).await {
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
                 cannot scope (GH #223)"
                    .to_string(),
            )
            .into()),
            Err(OptionLoadError::Overflow) => {
                let html = "<option value=\"\" disabled>Too many results — keep typing</option>"
                    .to_string();
                let res = http::Response::builder()
                    .status(200)
                    .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .header("x-content-type-options", "nosniff")
                    .body(Body::from(html))?;
                Ok(res)
            }
        }
    })
}

/// Parse `?field=` + `?q=` for the options endpoint (first-wins, like the
/// table state parser).
fn options_query(cx: &Cx) -> (String, String) {
    let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
        return (String::new(), String::new());
    };
    let query = parts.uri.query().unwrap_or("");
    let mut field = String::new();
    let mut q = String::new();
    let mut seen_field = false;
    let mut seen_q = false;
    for (k, v) in form_urlencoded::parse(query.as_bytes()) {
        if k == "field" && !seen_field {
            field = v.into_owned();
            seen_field = true;
        } else if k == "q" && !seen_q {
            q = v.into_owned();
            seen_q = true;
        }
    }
    (field, q)
}

#[cfg(test)]
mod tests;
