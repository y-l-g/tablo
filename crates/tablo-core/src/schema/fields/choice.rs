//! The choice control: a `<select>` over static options or a relationship's
//! rows, with an optional filter input.

use tablo_ui::{input as ui_input, select as ui_select};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        relationship::{
            OptionLoadError, OptionSource, RelatedCheck, RelationshipCheckFuture,
            RelationshipChecker, RelationshipLoadFuture, RelationshipLoader,
            RelationshipSearchLoader, related_record_check, related_records,
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
    /// Whether this is an embedded enum's variant control.
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
    /// The source's type, named in declaration errors.
    source: &'static str,
    /// Whether the request's panel can load from the source.
    available: fn(&Cx) -> bool,
    /// The source model when its rows are tenant-owned, so a key the write
    /// re-checks cannot name another tenant's row.
    tenant_scoped_model: fn(&Cx) -> Option<toasty::schema::app::ModelId>,
    /// Whether the source's primary key is composite, which no option value can spell.
    composite: bool,
}

/// One `<option>` with its value and label.
pub(crate) fn option_view<'a>(
    cx: &'a Cx,
    value: String,
    label: String,
    selected: bool,
) -> BoxView<'a> {
    view! { cx => <option value=(value) selected=(selected)>(label)</option> }.boxed()
}

impl Relationship {
    /// The loaders for source `R`, projecting each row to its primary key and label.
    pub(super) fn new<R>(label: impl Fn(&R::Model) -> String + Send + Sync + 'static) -> Self
    where
        R: OptionSource + 'static,
    {
        let project = std::sync::Arc::new(move |record: &R::Model| {
            (crate::toasty_compat::pk::pk_text(record), label(record))
        });
        let search_project = project.clone();
        let load = std::sync::Arc::new(move |cx: &Cx| {
            let project = project.clone();
            let cx = cx.clone();
            Box::pin(async move {
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
        let check = std::sync::Arc::new(check_record::<R>) as RelationshipChecker;
        Self {
            load,
            search,
            check,
            source: std::any::type_name::<R>(),
            available: R::available,
            tenant_scoped_model: |cx| {
                R::requires_tenant(cx).then(<R::Model as toasty::schema::Model>::id)
            },
            composite: crate::toasty_compat::pk::pk_is_composite::<R::Model>(),
        }
    }
}

fn check_record<'a, R>(
    cx: &'a Cx,
    value: String,
    ex: &'a mut dyn toasty::Executor,
) -> RelationshipCheckFuture<'a>
where
    R: OptionSource,
{
    Box::pin(related_record_check::<R>(cx, value, ex))
}

impl ChoiceControl {
    pub(crate) fn is_searchable(&self) -> bool {
        self.searchable
    }

    pub(crate) fn is_relationship(&self) -> bool {
        self.relationship.is_some()
    }

    /// Whether the options come from a model with a composite primary key.
    pub(crate) fn has_composite_source(&self) -> bool {
        self.relationship
            .as_ref()
            .is_some_and(|relationship| relationship.composite)
    }

    /// The model the options come from, when its rows are tenant-owned.
    pub(crate) fn tenant_scoped_model(&self, cx: &Cx) -> Option<toasty::schema::app::ModelId> {
        self.relationship
            .as_ref()
            .and_then(|relationship| (relationship.tenant_scoped_model)(cx))
    }

    /// The option source's type when the request's panel cannot load from it.
    pub(crate) fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
        self.relationship
            .as_ref()
            .filter(|relationship| !(relationship.available)(cx))
            .map(|relationship| relationship.source)
    }

    /// Searches options server-side, answering `Overflow` past the option cap.
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

    /// Checks whether a non-empty `value` matches an option.
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
                let mut db = crate::db::db(cx);
                match (relationship.check)(cx, trimmed.to_string(), &mut db).await {
                    Ok(RelatedCheck::FoundViewable) => return Vec::new(),
                    Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "is invalid",
                    Err(error) => load_failure(&error),
                }
            }
            Err(error) => load_failure(&error),
        };
        vec![format!("{label} {failure}")]
    }

    /// Re-checks a submitted relationship key in the write's transaction.
    pub(super) async fn recheck(
        &self,
        cx: &Cx,
        label: &str,
        value: &str,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<String> {
        let trimmed = value.trim();
        let Some(relationship) = &self.relationship else {
            return Vec::new();
        };
        if trimmed.is_empty() {
            return Vec::new();
        }
        let failure = match (relationship.check)(cx, trimmed.to_string(), ex).await {
            Ok(RelatedCheck::FoundViewable) => return Vec::new(),
            Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "is invalid",
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
        OptionLoadError::Misdeclared => "could not load options",
    }
}

impl Field {
    /// Renders a choice field's select and its read-only label.
    pub(super) async fn render_choice<'a>(
        &self,
        choice: &ChoiceControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            // Never loads options in view; a relationship shows its stored key.
            let stored = value.unwrap_or("").trim();
            let named = choice
                .options
                .iter()
                .find(|(v, _)| v == stored)
                .map(|(_, label)| label.clone());
            // A variant control reads as the variant's name.
            if choice.discriminant {
                return match named {
                    Some(name) => render_value(cx, self.label_str(), Some(&name), ValueKind::Prose),
                    None => Ok(().boxed()),
                };
            }
            let shown = named.unwrap_or_else(|| stored.to_string());
            return render_value(cx, self.label_str(), Some(&shown), ValueKind::Prose);
        }
        let name = self.name().to_string();
        let required = self.required;
        let searchable = choice.searchable;
        let current = value.unwrap_or("").trim().to_string();
        let loaded = choice.load_options(cx).await;
        // A denial hides the stored label and fails closed.
        // A failed load keeps the stored key selectable.
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
            error,
            denied.then(|| format!("{} is not available", self.label_str())),
        );
        let mut option_views: Vec<BoxView<'a>> = vec![option_view(
            cx,
            String::new(),
            "-- Select --".to_string(),
            current.is_empty(),
        )];
        for (val, lab) in &options {
            option_views.push(option_view(cx, val.clone(), lab.clone(), current == *val));
        }
        let list_id = format!("{name}-options-list");
        let filter_label = format!("Filter {} options", self.label_str());
        // Fetches from the server only past the cap.
        let options_field = overflow_searchable.then(|| name.clone());
        let options_server = overflow_searchable.then_some("true");
        let overflow_hint = "Too many options — type to search".to_string();
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let control = view! {
            cx =>
            if searchable {
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
            self.label_str(),
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
