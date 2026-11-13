//! What a related form shows: the child's form inside its owner's, as a wizard's step or a
//! section after the owner's fields.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, attributes, view},
};

use super::{Related, Shown};
use crate::{
    form::VERSION_FIELD,
    resource::{Resource, mounted},
    schema::{ExtraStep, Source},
    t,
    topcoat_compat::async_page,
};

/// Renders the section of each related form the request may write: what the submission posted
/// for it, else the record's child, else the form's defaults.
pub(crate) fn render_related<'a>(cx: &'a Cx, related: &[Related]) -> Vec<ExtraStep<'a>> {
    related
        .iter()
        .filter_map(|related| {
            let opened = related.opened.as_ref()?;
            let errored = related
                .posted
                .as_ref()
                .is_some_and(|posted| !posted.errors.is_empty());
            let shown = Shown {
                key: related.key.clone(),
                label: related.label.clone(),
                foreign_key: related.foreign_key.clone(),
                editing: opened.record.is_some(),
                version: opened.version,
                values: match (&related.posted, &opened.record) {
                    (Some(posted), _) => Some(posted.values.clone()),
                    (None, Some(_)) => Some(opened.stored.clone()),
                    (None, None) => None,
                },
                errors: related
                    .posted
                    .as_ref()
                    .map(|posted| posted.errors.clone())
                    .unwrap_or_default(),
            };
            Some(ExtraStep {
                id: format!("related-{}", related.key),
                label: related.label.clone(),
                errored,
                body: (related.form.render)(cx, shown),
            })
        })
        .collect()
}

/// Renders `C`'s form as a section of its owner's form, each control posting under `{key}.`
/// and the foreign key renders none.
pub(super) fn render<C: Resource>(cx: &Cx, shown: Shown) -> BoxView<'_> {
    async_page(async move {
        let Some(child) = mounted::<C>(cx) else {
            return Ok(().boxed());
        };
        let Shown {
            key,
            label,
            foreign_key,
            editing,
            version,
            values,
            errors,
        } = shown;
        let mask = child.form_mask(cx).hiding(&foreign_key);
        // A key the section renders no control for carries its error nowhere, so the section
        // says only that the save cannot pass, as the owner's form does.
        let unseen = errors.iter().any(|error| mask.hides(&error.key));
        let version = version.filter(|_| editing).map(|version| {
            let name = format!("{key}.{VERSION_FIELD}");
            view! { <input type="hidden" name=(name) value=(version.to_string())> }
        });
        let values = mask.shown(&values.unwrap_or_else(|| child.form.defaults()));
        let framed = !child.form.frames_itself(&mask);
        let source = Source::form(&values, &errors)
            .editing(editing)
            .masked(mask)
            .scoped(key.clone())
            .posting_under(format!("{key}."));
        // A form that draws its own sections shows them as they are; any other is framed in a
        // section titled after the relation, whose sections are then headings.
        let source = if framed {
            source.within_panel()
        } else {
            source
        };
        let body = child.form.render(cx, source).await?;
        Ok(view! {
            <div class="contents" data-related-form=(key)>
                if unseen {
                    tablo_ui::alert(
                        variant: tablo_ui::AlertVariant::Destructive,
                        attrs: attributes! { role="alert" data-unseen-error="" },
                        tablo_ui::alert_title((t!(cx, "tablo-form-unseen")))
                        tablo_ui::alert_description(
                            (t!(cx, "tablo-form-unseen-description"))
                        )
                    )
                }
                if let Some(version) = version {
                    (version)
                }
                (crate::schema::titled(cx, framed.then_some(label), body))
            </div>
        }
        .boxed())
    })
}
