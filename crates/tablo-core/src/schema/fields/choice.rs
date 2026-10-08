//! The choice control: a `<select>` over static options or a relationship's
//! rows, with an optional filter input.

use tablo_ui::{input as ui_input, select as ui_select};
use toasty::stmt::Expr;
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::relationship::{
        OptionLoadError, OptionSource, RelatedCheck, RelationshipCheckFuture, RelationshipChecker,
        RelationshipLoadFuture, RelationshipLoader, RelationshipSearchLoader, related_record_check,
        related_records, related_records_search,
    },
    Field, FieldChrome, render_field,
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
    /// The field whose value narrows the relationship's rows, set by `depends_on`.
    pub(super) parent: Option<Parent>,
}

/// What narrows a dependent choice's options: the related rows whose column equals the value the
/// field `watched` posts.
#[derive(Clone)]
pub(crate) struct Parent {
    pub(super) watched: String,
    /// The model the column belongs to, which must be the relationship's source model.
    pub(super) model: std::any::TypeId,
    /// The predicate selecting the rows for a trimmed, non-empty parent value, or `None` when the
    /// column's type does not parse it.
    pub(super) scope: ParentScope,
}

/// A dependent choice's predicate for one parent value.
pub(crate) type ParentScope = std::sync::Arc<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

/// The rows a dependent choice offers for one parent value.
enum Scope {
    /// Not a dependent choice: every row.
    All,
    /// The rows the predicate selects.
    Within(Expr<bool>),
    /// A blank parent, or one the column does not parse: no row.
    Nothing,
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
    /// The source's model, which a dependent choice's column must belong to.
    model: std::any::TypeId,
    /// Whether the request's panel can load from the source.
    available: fn(&Cx) -> bool,
    /// The source model when its rows are tenant-owned, so a key the write
    /// re-checks cannot name another tenant's row.
    tenant_scoped_model: fn(&Cx) -> Option<crate::toasty_compat::model::ModelId>,
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
    /// The loaders for source `R`, projecting each row to its primary key and `label`.
    pub(super) fn new<R>(label: impl Fn(&Cx, &R::Model) -> String + Send + Sync + 'static) -> Self
    where
        R: OptionSource + 'static,
    {
        let project = std::sync::Arc::new(move |cx: &Cx, record: &R::Model| {
            (crate::toasty_compat::pk::pk_text(record), label(cx, record))
        });
        let search_project = project.clone();
        let load = std::sync::Arc::new(move |cx: &Cx| {
            let project = project.clone();
            let cx = cx.clone();
            Box::pin(async move {
                let records = related_records::<R>(&cx, crate::tenancy::tenant_id(&cx))
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records.iter().map(|record| project(&cx, record)).collect())
            }) as RelationshipLoadFuture
        }) as RelationshipLoader;
        let search = std::sync::Arc::new(move |cx: &Cx, q: String, scope: Option<Expr<bool>>| {
            let project = search_project.clone();
            let cx = cx.clone();
            Box::pin(async move {
                let records = related_records_search::<R>(&cx, q, scope)
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records.iter().map(|record| project(&cx, record)).collect())
            }) as RelationshipLoadFuture
        }) as RelationshipSearchLoader;
        let check = std::sync::Arc::new(check_record::<R>) as RelationshipChecker;
        Self {
            load,
            search,
            check,
            source: std::any::type_name::<R>(),
            model: std::any::TypeId::of::<R::Model>(),
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
    scope: Option<Expr<bool>>,
    ex: &'a mut dyn toasty::Executor,
) -> RelationshipCheckFuture<'a>
where
    R: OptionSource,
{
    Box::pin(related_record_check::<R>(cx, value, scope, ex))
}

impl ChoiceControl {
    pub(crate) fn is_searchable(&self) -> bool {
        self.searchable
    }

    /// The label of the static option storing `value`.
    pub(crate) fn label_of(&self, value: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(stored, _)| stored == value)
            .map(|(_, label)| label.as_str())
    }

    /// Whether the choice declares neither options nor a relationship: its `<select>` offers
    /// nothing, and validation, with no option to check against, would accept any value.
    pub(crate) fn offers_nothing(&self) -> bool {
        self.options.is_empty() && self.relationship.is_none()
    }

    pub(crate) fn is_relationship(&self) -> bool {
        self.relationship.is_some()
    }

    /// The key of the field whose value narrows a dependent choice's options.
    pub(crate) fn parent_key(&self) -> Option<&str> {
        self.parent.as_ref().map(|parent| parent.watched.as_str())
    }

    /// Whether `depends_on` names a column of another model than the relationship's source, or
    /// the choice has no relationship to narrow.
    pub(crate) fn misdeclared_parent(&self) -> bool {
        self.parent.as_ref().is_some_and(|parent| {
            self.relationship
                .as_ref()
                .is_none_or(|relationship| relationship.model != parent.model)
        })
    }

    /// The rows the choice offers while its parent posts `parent`.
    fn scope(&self, parent: Option<&str>) -> Scope {
        let Some(declared) = &self.parent else {
            return Scope::All;
        };
        let value = parent.unwrap_or("").trim();
        if value.is_empty() {
            return Scope::Nothing;
        }
        match (declared.scope)(value) {
            Some(expr) => Scope::Within(expr),
            None => Scope::Nothing,
        }
    }

    /// Whether the options come from a model with a composite primary key.
    pub(crate) fn has_composite_source(&self) -> bool {
        self.relationship
            .as_ref()
            .is_some_and(|relationship| relationship.composite)
    }

    /// The model the options come from, when its rows are tenant-owned.
    pub(crate) fn tenant_scoped_model(
        &self,
        cx: &Cx,
    ) -> Option<crate::toasty_compat::model::ModelId> {
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

    /// Searches options server-side among those the parent value `parent` offers, answering
    /// `Overflow` past the option cap.
    pub(crate) async fn search_options(
        &self,
        cx: &Cx,
        q: &str,
        parent: Option<&str>,
    ) -> Result<Vec<(String, String)>, OptionLoadError> {
        let Some(relationship) = &self.relationship else {
            return Ok(self.options.clone());
        };
        match self.scope(parent) {
            Scope::All => (relationship.search)(cx, q.to_string(), None).await,
            Scope::Within(scope) => (relationship.search)(cx, q.to_string(), Some(scope)).await,
            Scope::Nothing => Ok(Vec::new()),
        }
    }

    /// The options the choice offers while its parent posts `parent`: a dependent choice's bounded
    /// head of the rows its parent selects.
    async fn load_options(
        &self,
        cx: &Cx,
        parent: Option<&str>,
    ) -> Result<Vec<(String, String)>, OptionLoadError> {
        match (&self.relationship, self.scope(parent)) {
            (Some(relationship), Scope::All) => (relationship.load)(cx).await,
            (Some(relationship), Scope::Within(scope)) => {
                (relationship.search)(cx, String::new(), Some(scope)).await
            }
            (Some(_), Scope::Nothing) => Ok(Vec::new()),
            (None, _) => Ok(self.options.clone()),
        }
    }

    /// Checks whether a non-empty `value` matches an option the parent value `parent` offers.
    pub(super) async fn validate_exists(
        &self,
        cx: &Cx,
        label: &str,
        value: &str,
        parent: Option<&str>,
    ) -> Vec<String> {
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
        let scope = match self.scope(parent) {
            Scope::All => None,
            Scope::Within(scope) => Some(scope),
            Scope::Nothing => return vec![format!("{label} is invalid")],
        };
        let failure = match self.load_options(cx, parent).await {
            Ok(opts) if opts.iter().any(|(v, _)| v == trimmed) => return Vec::new(),
            Ok(_) => "is invalid",
            Err(OptionLoadError::Denied) => "is not available",
            Err(OptionLoadError::Overflow) if self.searchable => {
                let mut db = crate::db::db(cx);
                match (relationship.check)(cx, trimmed.to_string(), scope, &mut db).await {
                    Ok(RelatedCheck::FoundViewable) => return Vec::new(),
                    Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "is invalid",
                    Err(error) => load_failure(&error),
                }
            }
            Err(error) => load_failure(&error),
        };
        vec![format!("{label} {failure}")]
    }

    /// Re-checks a submitted relationship key in the write's transaction, among the rows the
    /// parent value `parent` selects.
    pub(super) async fn recheck(
        &self,
        cx: &Cx,
        label: &str,
        value: &str,
        parent: Option<&str>,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<String> {
        let trimmed = value.trim();
        let Some(relationship) = &self.relationship else {
            return Vec::new();
        };
        if trimmed.is_empty() {
            return Vec::new();
        }
        let scope = match self.scope(parent) {
            Scope::All => None,
            Scope::Within(scope) => Some(scope),
            Scope::Nothing => return vec![format!("{label} is invalid")],
        };
        let failure = match (relationship.check)(cx, trimmed.to_string(), scope, ex).await {
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
    /// Renders a choice field's select, offering what its parent's value `parent` selects.
    pub(super) async fn render_choice<'a>(
        &self,
        choice: &ChoiceControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        parent: Option<&str>,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        let required = self.required;
        let searchable = choice.searchable;
        let current = value.unwrap_or("").trim().to_string();
        let loaded = choice.load_options(cx, parent).await;
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
        // A dependent choice fetches its options again when its parent changes, and a searchable
        // one fetches as the user types past the cap.
        let dependent = choice.parent_key().map(str::to_string);
        let parent_value = dependent
            .as_ref()
            .map(|_| parent.unwrap_or("").trim().to_string());
        let options_field = (overflow_searchable || dependent.is_some()).then(|| name.clone());
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
                data-options-parent=(dependent)
                data-options-parent-value=(parent_value)
            },
            control,
        )
    }
}

#[cfg(test)]
mod tests;
