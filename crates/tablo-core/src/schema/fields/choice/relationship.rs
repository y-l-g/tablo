//! A relationship choice's loaders: the bounded option load, the search, and the check past
//! the cap.

use toasty::stmt::Expr;
use topcoat::{Result, context::Cx};

use super::super::super::relationship::{
    OptionLoadError, OptionSource, RelatedCheck, RelationshipChecker, RelationshipLoader,
    RelationshipSearchLoader, related_record_check, related_records, related_records_search,
};
use crate::BoxFuture;

/// A relationship's three loaders: the bounded option load, the server-side
/// search, and the targeted existence check past the cap.
#[derive(Clone)]
pub(crate) struct Relationship {
    pub(super) load: RelationshipLoader,
    pub(super) search: RelationshipSearchLoader,
    pub(super) check: RelationshipChecker,
    /// The source's type, named in declaration errors.
    pub(super) source: &'static str,
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
            source: std::any::type_name::<R>(),
            model: std::any::TypeId::of::<R::Model>(),
            model_id: <R::Model as toasty::schema::Model>::id(),
            available: R::available,
            tenant_scoped_model: |cx| {
                R::requires_tenant(cx).then(<R::Model as toasty::schema::Model>::id)
            },
            composite: crate::toasty_compat::pk::pk_is_composite::<R::Model>(),
        }
    }

    /// The bounded option load, shared by the choice and the relationship filter.
    pub(crate) fn load(&self) -> RelationshipLoader {
        self.load.clone()
    }

    /// Whether the user may name every one of `keys`: each is among the rows the bounded option
    /// load lists, or, past its cap, a row the source's tenant-scoped query holds and lets them
    /// view. A refused load admits none.
    pub(crate) async fn admits(&self, cx: &Cx, keys: &[String]) -> bool {
        match (self.load)(cx).await {
            Ok(options) => keys
                .iter()
                .all(|key| options.iter().any(|(value, _)| value == key)),
            Err(OptionLoadError::Overflow) => {
                let mut db = crate::db::db(cx);
                for key in keys {
                    let check = (self.check)(cx, key.clone(), None, &mut db).await;
                    if !matches!(check, Ok(RelatedCheck::FoundViewable)) {
                        return false;
                    }
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Whether the source's primary key is composite, which no option value can spell.
    pub(crate) fn has_composite_source(&self) -> bool {
        self.composite
    }

    /// The source's type, when the request's panel cannot load from it.
    pub(crate) fn unavailable_source(&self, cx: &Cx) -> Option<&'static str> {
        (!(self.available)(cx)).then_some(self.source)
    }
}

fn check_record<'a, R>(
    cx: &'a Cx,
    value: String,
    scope: Option<Expr<bool>>,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<RelatedCheck, OptionLoadError>>
where
    R: OptionSource,
{
    Box::pin(related_record_check::<R>(cx, value, scope, ex))
}
