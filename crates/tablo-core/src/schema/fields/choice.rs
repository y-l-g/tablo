//! The choice control: a `<select>` over static options or a relationship's
//! rows, with an optional filter input.

use tablo_ui::{input as ui_input, select as ui_select};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        relationship::{
            OptionLoadError, OptionSource, RelatedCheck, RelatedPrimaryKey,
            RelationshipCheckFuture, RelationshipChecker, RelationshipLoadFuture,
            RelationshipLoader, RelationshipSearchLoader, related_record_check, related_records,
            related_records_search,
        },
        tree::Mode,
    },
    Field, FieldChrome, ValueKind, render_field, render_value,
};

/// What a choice field declares beyond presence.
#[derive(Clone, Default)]
pub(crate) struct ChoiceControl {
    pub(super) searchable: bool,
    /// Whether this is an embedded enum's variant control: it renders
    /// `data-variant-select`, which `variant.js` follows to show only the
    /// chosen variant's group, and a read-only page shows the variant's name.
    pub(super) discriminant: bool,
    /// Static `(value, label)` options.
    pub(super) options: Vec<(String, String)>,
    pub(super) relationship: Option<Relationship>,
}

/// A relationship's three loaders: the bounded option load, the server-side
/// search, and the targeted existence check past the cap.
#[derive(Clone)]
pub(crate) struct Relationship {
    load: RelationshipLoader,
    search: RelationshipSearchLoader,
    check: RelationshipChecker,
}

impl Relationship {
    /// The loaders for source `R`, projecting each row to its key and label.
    pub(super) fn new<R>(
        value: impl Fn(&R::Model) -> RelatedPrimaryKey<R> + Send + Sync + 'static,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
        RelatedPrimaryKey<R>: std::fmt::Display,
    {
        let project = std::sync::Arc::new(move |record: &R::Model| {
            (value(record).to_string(), label(record))
        });
        let search_project = project.clone();
        let load = std::sync::Arc::new(move |cx: &Cx| {
            let project = project.clone();
            let cx = cx.clone();
            Box::pin(async move {
                // `#[memoize(as_ref)]` hands back a borrow, so clone the
                // (small) error back into this future's owned result.
                let records = related_records::<R>(&cx, crate::tenancy::tenant_id(&cx))
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records.iter().map(|record| project(record)).collect())
            }) as RelationshipLoadFuture
        }) as RelationshipLoader;
        let search = std::sync::Arc::new(move |cx: &Cx, q: String| {
            let project = search_project.clone();
            let cx = cx.clone();
            Box::pin(async move {
                let records = related_records_search::<R>(&cx, q)
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records.iter().map(|record| project(record)).collect())
            }) as RelationshipLoadFuture
        }) as RelationshipSearchLoader;
        let check = std::sync::Arc::new(move |cx: &Cx, v: String| {
            let cx = cx.clone();
            Box::pin(async move {
                related_record_check::<R>(&cx, v)
                    .await
                    .map_err(|error| error.clone())
            }) as RelationshipCheckFuture
        }) as RelationshipChecker;
        Self {
            load,
            search,
            check,
        }
    }
}

impl ChoiceControl {
    pub(crate) fn is_searchable(&self) -> bool {
        self.searchable
    }

    pub(crate) fn is_relationship(&self) -> bool {
        self.relationship.is_some()
    }

    /// Server-side option search for the endpoint (D1/D5).
    ///
    /// Reuses the related table's searchable columns and bounds to
    /// `MAX_RELATIONSHIP_OPTIONS`. Returns `Overflow` when the filtered set
    /// still exceeds the cap (the caller renders "keep typing").
    pub(crate) async fn search_options(
        &self,
        cx: &Cx,
        q: &str,
    ) -> Result<Vec<(String, String)>, OptionLoadError> {
        match &self.relationship {
            Some(relationship) => (relationship.search)(cx, q.to_string()).await,
            None => Ok(self.options.clone()),
        }
    }

    async fn load_options(&self, cx: &Cx) -> Result<Vec<(String, String)>, OptionLoadError> {
        match &self.relationship {
            Some(relationship) => (relationship.load)(cx).await,
            None => Ok(self.options.clone()),
        }
    }

    /// Existence-only check: whether a non-empty `value` matches an option.
    ///
    /// A loader failure is a field error rather than an empty-options
    /// passthrough that would fail at FK write time. A policy denial reads
    /// "not available" — retrying cannot fix a permission decision, and
    /// "invalid" would misattribute it to the submitted value. An overflowed
    /// load uses the targeted check for a searchable field (a legitimate key
    /// beyond the cap validates) and keeps the retry error otherwise.
    pub(super) async fn validate_exists(&self, cx: &Cx, label: &str, value: &str) -> Vec<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let Some(relationship) = &self.relationship else {
            if !self.options.is_empty() && !self.options.iter().any(|(v, _)| v == trimmed) {
                return vec![format!("{label} is invalid")];
            }
            return Vec::new();
        };
        let failure = match (relationship.load)(cx).await {
            Ok(opts) if opts.iter().any(|(v, _)| v == trimmed) => return Vec::new(),
            Ok(_) => "is invalid",
            Err(OptionLoadError::Denied) => "is not available",
            Err(OptionLoadError::Overflow) if self.searchable => {
                match (relationship.check)(cx, trimmed.to_string()).await {
                    Ok(RelatedCheck::FoundViewable) => return Vec::new(),
                    Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "is invalid",
                    Err(error) => load_failure(&error),
                }
            }
            Err(error) => load_failure(&error),
        };
        vec![format!("{label} {failure}")]
    }
}

/// The wording of a failed option load.
fn load_failure(error: &OptionLoadError) -> &'static str {
    match error {
        OptionLoadError::Denied => "is not available",
        OptionLoadError::LoadFailed | OptionLoadError::Overflow => "could not load options, retry",
        // A misdeclaration is permanent: retrying cannot fix it, so it is
        // reported without the retry wording.
        OptionLoadError::Misdeclared => "could not load options",
    }
}

impl Field {
    /// Render a choice field: a read-only option label in `Mode::View`, the
    /// select (and its filter) otherwise.
    pub(super) async fn render_choice<'a>(
        &self,
        choice: &ChoiceControl,
        cx: &'a Cx,
        value: Option<&str>,
        errors: &[String],
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            // View mode resolves a static option label and never loads
            // options: a detail page renders one record, so a relationship's
            // option load would be a query per page, and its scoped/denied
            // paths police a *choice* the page is not offering. A relationship
            // therefore shows its stored key, and the detail page's own
            // includes are what make a related record readable.
            let stored = value.unwrap_or("").trim();
            let named = choice
                .options
                .iter()
                .find(|(v, _)| v == stored)
                .map(|(_, label)| label.clone());
            // A variant control reads as the variant's **name** — `Published`,
            // never the `3` the column holds (ADR-0016). A value the schema
            // does not declare has no name to show and renders nothing.
            if choice.discriminant {
                return match named {
                    Some(name) => render_value(cx, &self.label, Some(&name), ValueKind::Prose),
                    None => Ok(().boxed()),
                };
            }
            let shown = named.unwrap_or_else(|| stored.to_string());
            return render_value(cx, &self.label, Some(&shown), ValueKind::Prose);
        }
        let name = self.name.clone();
        let required = self.required;
        let searchable = choice.searchable;
        let current = value.unwrap_or("").trim().to_string();
        let loaded = choice.load_options(cx).await;
        // A policy denial does not re-render the stored value: the related
        // rows are not viewable, so neither is their label, and the submit
        // fails closed with "not available". The denial also surfaces on GET
        // (when the caller carries no error yet): the select has no options to
        // pick, so the empty control explains itself. A failed or overflowed
        // load keeps the stored key selectable, so an edit does not blank the
        // relation into a required error; a searchable field then degrades to
        // type-to-search with a hint, a non-searchable one to the retry path.
        let denied = matches!(&loaded, Err(OptionLoadError::Denied));
        let overflowed = matches!(&loaded, Err(OptionLoadError::Overflow));
        let overflow_searchable = overflowed && searchable && choice.is_relationship();
        let keep_current_value = matches!(
            &loaded,
            Err(OptionLoadError::LoadFailed)
                | Err(OptionLoadError::Overflow)
                | Err(OptionLoadError::Misdeclared)
        );
        let mut options = loaded.unwrap_or_default();
        if keep_current_value && !current.is_empty() && !options.iter().any(|(v, _)| v == &current)
        {
            options.push((current.clone(), current.clone()));
        }
        let chrome = FieldChrome::new(
            &name,
            errors,
            denied.then(|| format!("{} is not available", self.label)),
        );
        let mut option_views: Vec<BoxView<'a>> = Vec::new();
        let empty_selected = current.is_empty();
        option_views.push(
            view! {
                cx =>
                <option value="" selected=(empty_selected)>"-- Select --"</option>
            }
            .boxed(),
        );
        for (val, lab) in &options {
            let selected = current == *val;
            let val_c = val.clone();
            let lab_c = lab.clone();
            option_views.push(
                view! {
                    cx =>
                    <option value=(val_c) selected=(selected)>(lab_c)</option>
                }
                .boxed(),
            );
        }
        // The `select` primitive brings the same `aria-invalid` error styling
        // and focus ring as the `input` primitive, plus the chevron and the
        // customizable picker.
        let list_id = format!("{name}-options-list");
        let filter_label = format!("Filter {} options", self.label);
        // Server fetch only past the cap: a bounded searchable set keeps the
        // client-side label filter, so `data-options-server` follows the
        // overflow state. `selects.js` branches on it.
        let options_field = overflow_searchable.then(|| name.clone());
        let options_server = overflow_searchable.then_some("true");
        let variant_of = choice.discriminant.then(|| name.clone());
        let overflow_hint = "Too many options — type to search".to_string();
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let control = view! {
            cx =>
            if searchable {
                // The filter input and its suggestion list are one combobox.
                // The list is what makes the filter visible: the native
                // `<select>` popup is browser chrome the script cannot narrow
                // (the primitive opts into `appearance: base-select`, where
                // `option[hidden]` has no effect), so `selects.js` renders its
                // own filtered list here, keeps `aria-expanded` and
                // `aria-activedescendant` in step, and writes the chosen value
                // onto the select. Without the script the input is inert and
                // the plain select keeps working.
                <div class="relative" data-options-combobox="">
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            role="combobox"
                            aria-expanded="false"
                            aria-controls=(list_id.clone())
                            aria-autocomplete="list"
                            aria-label=(filter_label.clone())
                            placeholder="Filter…"
                            data-options-filter=""
                            class="h-9"
                            autocomplete="off"
                        }
                    )
                    <ul
                        id=(list_id.clone())
                        data-options-list=""
                        role="listbox"
                        aria-label=(filter_label.clone())
                        hidden=""
                        class="absolute z-20 mt-1 max-h-60 w-full overflow-y-auto rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-sm"
                    ></ul>
                </div>
            }
            if overflow_searchable {
                <div class="text-xs text-muted-foreground">(overflow_hint)</div>
            }
            ui_select(
                attrs: attributes! {
                    id=(name.clone())
                    name=(name.clone())
                    required=(required)
                    aria-required=(required.then_some("true"))
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                    data-variant-select=(variant_of)
                },
                for opt in option_views {
                    (opt)
                }
            )
        }
        .boxed();
        render_field(
            cx,
            &chrome,
            &self.label,
            required,
            attributes! {
                cx =>
                data-select-filterable=""
                data-options-field=(options_field)
                data-options-server=(options_server)
            },
            control,
        )
    }
}

#[cfg(test)]
mod tests;
