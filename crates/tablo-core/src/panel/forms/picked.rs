//! The page a create lands on when a creatable choice's dialog frames the create page:
//! `GET {list}/-/picked?pick=1&key=`.
//!
//! The dialog's script reads the record's key and title off it and makes the record the choice.
//! The page offers the record only as a relationship choice would: it refuses a user the
//! resource's policy denies `ViewAny`, as the list and the option search do, and loads the record
//! through the resource's scoped query, so a key typed into the URL tells nothing a choice would
//! not.

use topcoat::{
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, ViewExt, attributes, view},
};

use super::super::gate::gate;
use crate::{Ability, resource::Resource, schema::OptionSource, t, topcoat_compat::async_page};

/// Renders the record the create saved as the choice it offers, or says it cannot be chosen.
pub(crate) fn resource_picked<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        // The save's success flash belongs to this page, which the dialog closes over: the choice
        // showing the record says it.
        let _ = crate::flash::take_notification(cx);
        let key = picked_key(cx);
        let option = chosen::<R>(cx, &key).await?.map(|label| (key, label));
        Ok(render_picked(cx, option))
    })
}

/// The record's title when the user may choose it: a row of the resource's scoped query.
async fn chosen<R: Resource>(cx: &Cx, key: &str) -> topcoat::Result<Option<String>> {
    let Some(expr) = crate::toasty_compat::pk::pk_eq_expr::<R::Model>(key.trim()) else {
        return Ok(None);
    };
    let record = <R as OptionSource>::scoped_query(cx)?
        .filter(expr)
        .first()
        .exec(&mut crate::db::db(cx)?)
        .await
        .map_err(crate::error::unavailable)?;
    Ok(record.map(|record| <R as OptionSource>::label(cx, &record)))
}

/// The `?key=` the create's landing carries.
fn picked_key(cx: &Cx) -> String {
    let query = topcoat::router::request::uri(cx)
        .query()
        .unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .find(|(name, _)| name == "key")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

/// The page carrying `option`, the record's key and title, as `data-picked-value` and
/// `data-picked-label`; without one, it says the record was created and closes the dialog.
fn render_picked(cx: &Cx, option: Option<(String, String)>) -> BoxView<'_> {
    match option {
        Some((value, label)) => view! {
            cx =>
            tablo_ui::alert(
                attrs: attributes! {
                    data-picked=""
                    data-picked-value=(value)
                    data-picked-label=(label.clone())
                },
                tablo_ui::alert_title(
                    (t!(cx, "tablo-field-option-created", label = label))
                )
            )
        }
        .boxed(),
        None => view! {
            cx =>
            <div class="flex flex-col gap-4">
                tablo_ui::alert(
                    attrs: attributes! { data-picked-hidden="" },
                    tablo_ui::alert_title((t!(cx, "tablo-field-option-unchoosable")))
                    tablo_ui::alert_description(
                        (t!(cx, "tablo-field-option-unchoosable-description"))
                    )
                )
                <div>
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Outline,
                        attrs: attributes! { type="button" data-pick-cancel="" },
                        (t!(cx, "tablo-close"))
                    )
                </div>
            </div>
        }
        .boxed(),
    }
}
