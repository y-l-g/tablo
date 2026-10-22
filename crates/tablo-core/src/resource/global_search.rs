//! [`GlobalSearch`] and [`ResourceDef::global_search`]: how a resource's records show in its
//! panel's global search, and the search of one resource's records for a request.

use std::sync::Arc;

use topcoat::{Result, context::Cx};

use super::{Resource, ResourceDef, mounted};
use crate::{
    Lens,
    form::FormScalar,
    policy::Ability,
    toasty_compat::model::{ModelPath, root_field},
};

/// How a resource's records show in its panel's global search, as
/// [`ResourceDef::global_search`] declares it: the details listed under each result's title.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String }
/// # struct UserResource;
/// # impl tablo_core::Resource for UserResource {
/// #     type Model = User;
/// #     type Form = tablo_core::NoForm<User>;
/// #     fn declare() -> tablo_core::ResourceDef<Self> {
/// use tablo_core::{GlobalSearch, ResourceDef, lens};
///
/// ResourceDef::new()
///     .record_title(lens!(User.name))
///     .global_search(GlobalSearch::new().detail("Email", lens!(User.email)))
/// #     }
/// # }
/// ```
pub struct GlobalSearch<M> {
    details: Vec<Detail<M>>,
}

/// One labelled value under a result's title.
struct Detail<M> {
    label: String,
    read: Arc<dyn Fn(&M) -> String + Send + Sync>,
    /// The model's root field the value reads, which field access may hide.
    field: Option<String>,
}

impl<M: toasty::schema::Model + Send + Sync + 'static> GlobalSearch<M> {
    /// A result with its title alone.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lists the value `lens` reads under each result's title, labelled `label`, after the
    /// details already declared.
    ///
    /// A result leaves out an empty value, and every value of a field
    /// [field access](ResourceDef::field_access) hides from the request.
    #[must_use]
    pub fn detail<T>(mut self, label: impl Into<String>, lens: Lens<M, T>) -> Self
    where
        T: FormScalar + Send + Sync + 'static,
    {
        let field = root_field::<M>(&ModelPath::of(lens.path()));
        self.details.push(Detail {
            label: label.into(),
            read: Arc::new(move |record| lens.read(record).to_label()),
            field,
        });
        self
    }

    /// The `(label, value)` of each detail of `record` the request may see.
    fn details_of(&self, record: &M, hidden: &[String]) -> Vec<(String, String)> {
        self.details
            .iter()
            .filter(|detail| {
                detail
                    .field
                    .as_ref()
                    .is_none_or(|field| !hidden.contains(field))
            })
            .map(|detail| (detail.label.clone(), (detail.read)(record)))
            .filter(|(_, value)| !value.trim().is_empty())
            .collect()
    }
}

impl<M> Clone for GlobalSearch<M> {
    fn clone(&self) -> Self {
        Self {
            details: self.details.clone(),
        }
    }
}

impl<M> Clone for Detail<M> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            read: Arc::clone(&self.read),
            field: self.field.clone(),
        }
    }
}

impl<M> Default for GlobalSearch<M> {
    fn default() -> Self {
        Self {
            details: Vec::new(),
        }
    }
}

impl<M> std::fmt::Debug for GlobalSearch<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalSearch")
            .field(
                "details",
                &self
                    .details
                    .iter()
                    .map(|detail| &detail.label)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl<R: Resource> ResourceDef<R> {
    /// Declares how the resource's records show in the panel's global search: the details under
    /// each result's title. Without it a result shows its title alone.
    ///
    /// The search box in the panel's topbar searches every resource the panel mounts unless the
    /// resource opts out with [`without_global_search`](Self::without_global_search). It matches
    /// a record when each word of the term is in one of the table's searchable columns or in the
    /// [`record_title`](Self::record_title) column, which also titles each result. It lists only
    /// the records the resource's list would, and none to a request the list refuses: one the
    /// policy refuses `ViewAny`, or one without a tenant on a tenant-scoped resource. Each
    /// result opens the record's detail page, or its edit page on a resource with an empty
    /// [`detail`](Self::detail); a record the policy refuses `View`, or `Update` for the edit
    /// page, is left out.
    ///
    /// See [`Panel::without_global_search`](crate::Panel::without_global_search) for the panel's
    /// side.
    #[must_use]
    pub fn global_search(mut self, search: GlobalSearch<R::Model>) -> Self {
        self.global_search = Some(search);
        self
    }

    /// Leaves the resource's records out of the panel's global search.
    #[must_use]
    pub fn without_global_search(mut self) -> Self {
        self.global_search = None;
        self
    }
}

/// One record a global search found: its title, its key, the page it opens, and its details.
#[derive(Debug, Clone)]
pub(crate) struct SearchHit {
    pub(crate) title: String,
    /// The record's key as its URLs spell it.
    pub(crate) key: String,
    pub(crate) opens: Opens,
    pub(crate) details: Vec<(String, String)>,
}

/// The record page a result opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opens {
    Detail,
    Edit,
}

/// How many records past the asked limit one batch of a search reads, so the records the policy
/// refuses seldom cost a second batch.
const BATCH_FACTOR: usize = 4;

/// How many batches a search reads at most, which bounds its work when the policy refuses most of
/// the matching records.
const MAX_BATCHES: usize = 3;

/// Whether the request may list `R`'s records, so its results may show: the policy allows
/// `ViewAny`, and the request has a tenant when `R` is tenant-scoped.
pub(crate) fn searchable<R: Resource>(cx: &Cx) -> bool {
    mounted::<R>(cx).is_some_and(|mounted| lists(cx, &mounted))
}

/// Whether the request may list `mounted`'s records.
fn lists<R: Resource>(cx: &Cx, mounted: &super::Mounted<R>) -> bool {
    (!mounted.tenancy.is_scoped() || crate::tenancy::require_tenant(cx).is_ok())
        && mounted.can(cx, Ability::ViewAny)
}

/// Up to `limit` of `R`'s records matching every one of `words` that the request may open, each
/// with the page it opens; empty when the request may not list `R`, or `R` searches no column.
///
/// The policy decides per record which page it opens, so the search reads the matching records
/// in batches until `limit` of them open, the records run out, or [`MAX_BATCHES`] batches are
/// read.
pub(crate) fn search<'a, R: Resource>(
    cx: &'a Cx,
    words: &'a [String],
    limit: usize,
) -> crate::BoxFuture<'a, Result<Vec<SearchHit>>> {
    Box::pin(async move {
        let Some(mounted) = mounted::<R>(cx) else {
            return Ok(Vec::new());
        };
        let Some(search) = &mounted.global_search else {
            return Ok(Vec::new());
        };
        if !lists(cx, &mounted) {
            return Ok(Vec::new());
        }
        let Some(matched) = mounted.words_search_expr(cx, words) else {
            return Ok(Vec::new());
        };
        let mut db = crate::db::db(cx);
        let matched = crate::toasty_compat::case_insensitive_search(&db, matched);
        let order = mounted.table_for(cx).first_order_by();
        let mask = mounted.field_mask(cx);
        let batch = limit.saturating_mul(BATCH_FACTOR);
        let mut hits = Vec::new();
        for read in 0..MAX_BATCHES {
            let mut query = mounted.scoped_query(cx)?.filter(matched.clone());
            if let Some(order) = order.clone() {
                query = query.order_by(order);
            }
            let records = query
                .limit(batch)
                .offset(read * batch)
                .exec(&mut db)
                .await?;
            hits.extend(records.iter().filter_map(|record| {
                Some(SearchHit {
                    opens: opens::<R>(cx, &mounted, record)?,
                    key: mounted.table.key_of(record),
                    title: mounted.record_title(record, &crate::toasty_compat::pk::pk_text(record)),
                    details: search.details_of(record, mask.hidden()),
                })
            }));
            if hits.len() >= limit || records.len() < batch {
                break;
            }
        }
        hits.truncate(limit);
        Ok(hits)
    })
}

/// The page a result opens: the record's detail page, or its edit page on a resource without
/// one; `None` when the record's key has no URL, or the policy refuses `View`, which both pages
/// ask, or `Update` for the edit page.
fn opens<R: Resource>(cx: &Cx, mounted: &super::Mounted<R>, record: &R::Model) -> Option<Opens> {
    if !mounted.table.is_addressable() || !mounted.can(cx, Ability::View(record)) {
        return None;
    }
    if mounted.viewed() {
        Some(Opens::Detail)
    } else if <R::Form as crate::form::RecordForm>::HAS_FORM
        && mounted.can(cx, Ability::Update(record))
    {
        Some(Opens::Edit)
    } else {
        None
    }
}
