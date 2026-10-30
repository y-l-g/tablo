//! A relationship choice's loaders: the bounded option load, the search, and the checks past
//! the cap.

use toasty::stmt::Expr;
use topcoat::{Result, context::Cx};

use super::super::super::relationship::{
    OptionLoadError, OptionSource, RelationshipChecker, RelationshipLoader,
    RelationshipSearchLoader, held_keys, related_record_check, related_records,
    related_records_search,
};
use crate::BoxFuture;

/// Reads which of a filter's keys name a row the source offers, past the cap.
type HeldKeys =
    for<'a> fn(
        &'a Cx,
        &'a [String],
    ) -> BoxFuture<'a, Result<std::collections::HashSet<String>, OptionLoadError>>;

/// A relationship's loaders: the bounded option load, the server-side search, the targeted
/// existence check past the cap, and the keys a filter may name past it.
#[derive(Clone)]
pub(crate) struct Relationship {
    pub(super) load: RelationshipLoader,
    pub(super) search: RelationshipSearchLoader,
    pub(super) check: RelationshipChecker,
    /// Which of a relationship filter's keys the user may name, past the cap.
    pub(super) held: HeldKeys,
    /// The source's type, named in declaration errors.
    pub(super) source: &'static str,
    /// The source's type, which a panel's field access masks the options of when it is a mounted
    /// resource.
    source_type: std::any::TypeId,
    /// The source's model, which a dependent choice's column must belong to.
    pub(super) model: std::any::TypeId,
    /// The source's model in the app schema, which a many-to-many field's target must be.
    pub(super) model_id: crate::toasty_compat::model::ModelId,
    /// Whether the request's panel can load from the source.
    pub(super) available: fn(&Cx) -> bool,
    /// The source model when its rows are tenant-owned, so a key the write
    /// re-checks cannot name another tenant's row.
    pub(super) tenant_scoped_model: fn(&Cx) -> Option<crate::toasty_compat::model::ModelId>,
    /// Whether the source's primary key is composite, which no option value can spell.
    pub(super) composite: bool,
    /// Whether the options carry a custom label rather than the source's own, which a panel
    /// cannot see the fields of.
    custom_label: bool,
    /// The source model's name, which a declaration error names it by.
    model_name: &'static str,
    /// Whether the source has a create page a creatable choice could open.
    pub(super) creates: bool,
    /// The source's create page, when the user may create there.
    pub(super) create_page: fn(&Cx) -> Option<String>,
}

impl Relationship {
    /// The loaders for source `R`, projecting each row to its primary key and `label`.
    pub(crate) fn new<R>(label: impl Fn(&Cx, &R::Model) -> String + Send + Sync + 'static) -> Self
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
            }) as BoxFuture<'static, _>
        }) as RelationshipLoader;
        let search = std::sync::Arc::new(move |cx: &Cx, q: String, scope: Option<Expr<bool>>| {
            let project = search_project.clone();
            let cx = cx.clone();
            Box::pin(async move {
                let records = related_records_search::<R>(&cx, q, scope)
                    .await
                    .map_err(|error| error.clone())?;
                Ok(records.iter().map(|record| project(&cx, record)).collect())
            }) as BoxFuture<'static, _>
        }) as RelationshipSearchLoader;
        let check = std::sync::Arc::new(check_record::<R>) as RelationshipChecker;
        Self {
            load,
            search,
            check,
            held: |cx, keys| {
                Box::pin(async move {
                    held_keys::<R>(cx, crate::tenancy::tenant_id(cx), keys)
                        .await
                        .cloned()
                        .map_err(Clone::clone)
                })
            },
            source: std::any::type_name::<R>(),
            source_type: std::any::TypeId::of::<R>(),
            model: std::any::TypeId::of::<R::Model>(),
            model_id: <R::Model as toasty::schema::Model>::id(),
            available: R::available,
            tenant_scoped_model: |cx| {
                R::requires_tenant(cx).then(<R::Model as toasty::schema::Model>::id)
            },
            composite: crate::toasty_compat::pk::pk_is_composite::<R::Model>(),
            custom_label: false,
            model_name: crate::naming::type_short_name::<R::Model>(),
            creates: R::creates(),
            create_page: R::create_page,
        }
    }

    /// The bounded option load, shared by the choice and the relationship filter.
    pub(crate) fn load(&self) -> RelationshipLoader {
        self.load.clone()
    }

    /// Whether the user may name every one of `keys`: each is among the rows the bounded option
    /// load lists, or, past its cap, a row the source's scoped query holds, which one query
    /// reads. A key matches as the option value spells it. A source the
    /// request may not load from admits none.
    ///
    /// # Errors
    ///
    /// The database failing the load or the check.
    pub(crate) async fn admits(&self, cx: &Cx, keys: &[String]) -> Result<bool> {
        let shown: std::collections::HashSet<String> = match (self.load)(cx).await {
            Ok(options) => options.into_iter().map(|(value, _)| value).collect(),
            Err(OptionLoadError::Overflow) => match (self.held)(cx, keys).await {
                Ok(keys) => keys,
                Err(error) => return refusal(error),
            },
            Err(error) => return refusal(error),
        };
        Ok(keys.iter().all(|key| shown.contains(key)))
    }

    /// Whether the source's primary key is composite, which no option value can spell.
    pub(crate) fn has_composite_source(&self) -> bool {
        self.composite
    }

    /// Labels the options by a custom label rather than the source's own.
    pub(crate) fn with_custom_label(mut self) -> Self {
        self.custom_label = true;
        self
    }

    /// The source model's name when the options show records under a label a panel cannot mask,
    /// over a model `restricted` says a mounted resource restricts a field of: a custom label, or a
    /// source `is_mounted` does not say is one of the panel's resources.
    pub(crate) fn unmasked_model(
        &self,
        is_mounted: impl Fn(std::any::TypeId) -> bool,
        restricted: impl Fn(crate::toasty_compat::model::ModelId) -> bool,
    ) -> Option<&'static str> {
        let unmasked = self.custom_label || !is_mounted(self.source_type);
        (unmasked && restricted(self.model_id)).then_some(self.model_name)
    }

    /// The source's type, when the request's panel cannot load from it.
    pub(crate) fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
        (!(self.available)(cx)).then_some(self.source)
    }
}

/// A refused load admits nothing; a failed one fails the request.
fn refusal(error: OptionLoadError) -> Result<bool> {
    match error {
        OptionLoadError::LoadFailed => Err(crate::error::unavailable(
            "relationship filter check failed",
        )),
        _ => Ok(false),
    }
}

fn check_record<'a, R>(
    cx: &'a Cx,
    value: String,
    scope: Option<Expr<bool>>,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<bool, OptionLoadError>>
where
    R: OptionSource,
{
    Box::pin(related_record_check::<R>(cx, value, scope, ex))
}
