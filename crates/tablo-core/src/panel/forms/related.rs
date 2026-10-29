//! A `has_one` relation's form: the related resource's own form inside its owner's, posting
//! under the child's slug, and written after the owner in the owner's transaction.

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use toasty::stmt::{Expr, List, Query};
use topcoat::{
    Result,
    context::Cx,
    router::error::bad_request,
    view::{BoxView, ViewExt, attributes, view},
};

use super::{
    common::FormParts,
    render::VERSION_FIELD,
    submit::{
        Unposted, complete, named_fields, parse_form, prepare_submission, recheck_relationships,
        refusal,
    },
    unique::check_unique,
};
use crate::{
    BoxFuture, ModelColumns as _,
    form::{FieldErrors, FormValue, FormValues, Posted, RecordForm},
    panel::{gate::enforce_tenant, state::current},
    policy::Ability,
    resource::{Change, Mounted, Resource, WriteError, mounted, require_mounted},
    schema::Source,
    t,
    toasty_compat::pk::{pk_filter, pk_text},
    topcoat_compat::async_page,
};

/// What a committed write still owes a related resource: its `after_commit` hook.
pub(crate) type AfterCommit = Box<dyn for<'c> FnOnce(&'c Cx) -> BoxFuture<'c, ()> + Send>;

/// A registered resource's form as the related form of other resources' `has_one` relations,
/// each step monomorphized over that resource.
#[derive(Clone, Copy)]
pub(crate) struct ChildForm {
    open: OpenFn,
    prepare: for<'a> fn(&'a Cx, &'a Opened, FormParts) -> BoxFuture<'a, Result<Prepared>>,
    write: WriteFn,
    render: for<'a> fn(&'a Cx, Shown) -> BoxView<'a>,
}

/// Opens the child the scope selects, given the foreign key's form key.
type OpenFn = for<'a> fn(
    &'a Cx,
    Option<Expr<bool>>,
    &'a str,
    &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<Option<Opened>>>;

/// Writes a prepared submission as the child the scope selects, given the foreign key's form key
/// and the owner's key.
type WriteFn = for<'a> fn(
    &'a Cx,
    Prepared,
    Expr<bool>,
    &'a str,
    String,
    &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<Written>>;

impl ChildForm {
    /// `C`'s form as a related form.
    pub(crate) fn of<C: Resource>() -> Self {
        Self {
            open: |cx, scope, foreign_key, ex| Box::pin(open::<C>(cx, scope, foreign_key, ex)),
            prepare: |cx, opened, parts| Box::pin(prepare::<C>(cx, opened, parts)),
            write: |cx, prepared, scope, foreign_key, owner, ex| {
                Box::pin(write::<C>(cx, prepared, scope, foreign_key, owner, ex))
            },
            render: render::<C>,
        }
    }
}

/// The related record a request may write through the section: the record's child, or `None`
/// for the one the save creates, with its form values.
pub(crate) struct Opened {
    record: Option<Box<dyn Any + Send + Sync>>,
    stored: FormValues,
    /// The child's URL id, `None` for the one the save creates.
    id: Option<String>,
    /// The child's `#[version]`, which the section posts back.
    version: Option<u64>,
}

/// A related form's submission, read before the owner's transaction opens.
pub(crate) struct Prepared {
    values: FormValues,
    errors: FieldErrors,
    named: HashSet<String>,
    /// The id of the child it was read against, `None` for a create.
    against: Option<String>,
    /// The version the section posted.
    version: Option<u64>,
}

/// How a related form's write ended.
pub(crate) enum Written {
    /// It wrote the related record and ran its `before_commit`.
    Done(AfterCommit),
    /// A rule, `validate_record` or the record fn refused it: the transaction must roll back.
    Refused(Prepared),
    /// The child changed since the submission was read: another save wrote it, created it, or
    /// took it out of the request's reach. It holds the child as it now stands.
    Stale(Prepared, Option<Opened>),
}

/// How the related forms' writes ended.
pub(crate) enum RelatedWrite {
    /// Every one wrote: the hooks the commit still owes.
    Done(Vec<AfterCommit>),
    /// One refused: the transaction must roll back.
    Refused,
    /// One's child changed since the form opened: the transaction must roll back, and the page
    /// says so.
    Stale,
}

/// What a section renders.
pub(crate) struct Shown {
    key: String,
    label: crate::Text,
    foreign_key: String,
    editing: bool,
    version: Option<u64>,
    values: Option<FormValues>,
    errors: FieldErrors,
}

/// One `has_one` relation of an owner, resolved for the request.
pub(crate) struct Related {
    /// The relation's index among the owner's.
    index: usize,
    /// The child's slug: its controls post under `{key}.`.
    key: String,
    label: crate::Text,
    foreign_key: String,
    form: ChildForm,
    /// `None` when no section renders: the request may not write the child.
    opened: Option<Opened>,
    /// What the submission posted under the prefix, once read.
    posted: Option<Prepared>,
}

impl Related {
    fn prefix(&self) -> String {
        format!("{}.", self.key)
    }
}

/// Each `has_one` relation of `resource`, opened for `owner`, or for the record a create makes.
///
/// # Errors
///
/// The driver's error.
pub(crate) async fn open_related<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    owner: Option<&R::Model>,
    ex: &mut dyn toasty::Executor,
) -> Result<Vec<Related>> {
    let Some(panel) = current(cx) else {
        return Ok(Vec::new());
    };
    let mut related = Vec::new();
    for (index, relation) in resource.relations.iter().enumerate() {
        let (Some(foreign_key), Some(child)) =
            (relation.has_one_key(), panel.children.get(&relation.child))
        else {
            continue;
        };
        let scope = owner.map(|owner| relation.bind(owner).0);
        let opened = (child.form.open)(cx, scope, foreign_key, &mut *ex).await?;
        related.push(Related {
            index,
            key: child.slug.clone(),
            label: relation
                .label
                .clone()
                .unwrap_or_else(|| child.label.clone()),
            foreign_key: foreign_key.to_string(),
            form: child.form,
            opened,
            posted: None,
        });
    }
    Ok(related)
}

/// Takes what `parts` posts under each related form's prefix out of it, and reads it as that
/// form's submission. A related form the submission posts nothing for stays unread, so its
/// record stays as it is.
///
/// # Errors
///
/// 400 for a key posted under the prefix of a form that renders no section, or under its
/// foreign key, which the owner fills; else what reading the submission refuses.
pub(crate) async fn read_related(
    cx: &Cx,
    parts: &mut FormParts,
    related: &mut [Related],
) -> Result<()> {
    for related in related.iter_mut() {
        let prefix = related.prefix();
        let (posted, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut parts.pairs)
            .into_iter()
            .partition(|(name, _)| name.starts_with(&prefix));
        parts.pairs = kept;
        if posted.is_empty() {
            continue;
        }
        let Some(opened) = &related.opened else {
            return Err(bad_request(format!("`{}` takes no submission", related.key)).into());
        };
        let mut version = None;
        let pairs: Vec<(String, String)> = posted
            .into_iter()
            .map(|(name, value)| (name[prefix.len()..].to_string(), value))
            .filter(|(name, value)| {
                let posts_version = name == VERSION_FIELD;
                if posts_version {
                    version = Some(value.clone());
                }
                !posts_version
            })
            .collect();
        let version = version
            .map(|version| version.parse::<u64>())
            .transpose()
            .map_err(|error| bad_request(error.to_string()))?;
        if pairs.iter().any(|(name, _)| *name == related.foreign_key) {
            return Err(bad_request(format!(
                "`{prefix}{}` is filled by the record the form saves",
                related.foreign_key
            ))
            .into());
        }
        let parts = FormParts {
            pairs,
            files: HashMap::new(),
            file_part_names: HashSet::new(),
        };
        let mut prepared = (related.form.prepare)(cx, opened, parts).await?;
        prepared.against = opened.id.clone();
        prepared.version = version;
        related.posted = Some(prepared);
    }
    Ok(())
}

/// Whether every related form the submission posted passes its controls' rules.
pub(crate) fn related_valid(related: &[Related]) -> bool {
    related
        .iter()
        .filter_map(|related| related.posted.as_ref())
        .all(|posted| posted.errors.is_empty())
}

/// Writes each related form the submission posted, for the record `owner` the owner's write
/// left, through `ex`.
///
/// Once one refuses or finds its child changed, the caller rolls the transaction back and
/// renders the forms again.
///
/// # Errors
///
/// The driver's error, or a related resource's hook's.
pub(crate) async fn write_related<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    related: &mut [Related],
    owner: &R::Model,
    ex: &mut dyn toasty::Executor,
) -> Result<RelatedWrite> {
    let mut after = Vec::new();
    for related in related.iter_mut() {
        let Some(prepared) = related.posted.take() else {
            continue;
        };
        let (scope, key) = resource.relations[related.index].bind(owner);
        match (related.form.write)(cx, prepared, scope, &related.foreign_key, key, &mut *ex).await?
        {
            Written::Done(hook) => after.push(hook),
            Written::Refused(prepared) => {
                related.posted = Some(prepared);
                return Ok(RelatedWrite::Refused);
            }
            // The section renders on the child as it now stands, so saving again overwrites.
            Written::Stale(prepared, opened) => {
                related.posted = Some(prepared);
                related.opened = opened;
                return Ok(RelatedWrite::Stale);
            }
        }
    }
    Ok(RelatedWrite::Done(after))
}

/// Renders the section of each related form the request may write: what the submission posted
/// for it, else the record's child, else the form's defaults.
pub(crate) fn render_related<'a>(cx: &'a Cx, related: &[Related]) -> Vec<BoxView<'a>> {
    related
        .iter()
        .filter_map(|related| {
            let opened = related.opened.as_ref()?;
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
            Some((related.form.render)(cx, shown))
        })
        .collect()
}

/// The child `scope` selects for `C`, or the one a create makes when `scope` is `None`, if the
/// request may write it.
///
/// Every child row the owner holds counts, whoever may see it: a second one, or one outside
/// `C`'s query, renders no section rather than let a save create another.
async fn open<C: Resource>(
    cx: &Cx,
    scope: Option<Expr<bool>>,
    foreign_key: &str,
    ex: &mut dyn toasty::Executor,
) -> Result<Option<Opened>> {
    let Some(child) = mounted::<C>(cx) else {
        return Ok(None);
    };
    let mask = child.form_mask(cx);
    if enforce_tenant(cx, &child).is_err() || mask.hides(foreign_key) || mask.locks(foreign_key) {
        return Ok(None);
    }
    let creates = || {
        (child.creates && child.can_create(cx)).then(|| Opened {
            record: None,
            stored: FormValues::new(),
            id: None,
            version: None,
        })
    };
    let Some(scope) = scope else {
        return Ok(creates());
    };
    let held = Query::<List<C::Model>>::all()
        .filter(scope)
        .exec(&mut *ex)
        .await
        .map_err(crate::error::unavailable)?;
    let held = match held.as_slice() {
        [] => return Ok(creates()),
        [held] => held,
        _ => return Ok(None),
    };
    let Ok(query) = child.scoped_query(cx) else {
        return Ok(None);
    };
    let record = <C::Form as RecordForm>::includes()
        .into_vec()
        .into_iter()
        .fold(query.filter(pk_filter(held)), |query, include| {
            query.include(include)
        })
        .first()
        .exec(&mut *ex)
        .await
        .map_err(crate::error::unavailable)?;
    Ok(record
        .filter(|record| {
            child.can(cx, Ability::View(record)) && child.can(cx, Ability::Update(record))
        })
        .map(|record| Opened {
            stored: <C::Form as RecordForm>::hydrate(cx, &record),
            id: Some(pk_text(&record)),
            version: record.version(),
            record: Some(Box::new(record)),
        }))
}

/// Reads `parts` as `C`'s create or edit submission.
async fn prepare<C: Resource>(cx: &Cx, opened: &Opened, parts: FormParts) -> Result<Prepared> {
    let child = require_mounted::<C>(cx)?;
    let record = opened
        .record
        .as_ref()
        .map(|record| child_record::<C>(record.as_ref()));
    let submission = prepare_submission(cx, &child, parts, record, Unposted::Missing).await?;
    Ok(Prepared {
        values: submission.values,
        errors: submission.errors,
        named: submission.named,
        against: None,
        version: None,
    })
}

/// Writes `prepared` as `C`'s record belonging to the owner `owner` spells, the child `scope`
/// selects, opened again inside the transaction.
///
/// A child that is no longer the one `prepared` was read against, or whose version moved, is
/// stale: writing would apply a create's defaults over another save's child, or overwrite it.
async fn write<C: Resource>(
    cx: &Cx,
    prepared: Prepared,
    scope: Expr<bool>,
    foreign_key: &str,
    owner: String,
    ex: &mut dyn toasty::Executor,
) -> Result<Written> {
    let child = require_mounted::<C>(cx)?;
    let opened = match open::<C>(cx, Some(scope), foreign_key, &mut *ex).await? {
        Some(opened)
            if opened.id == prepared.against
                && (prepared.version.is_none() || opened.version == prepared.version) =>
        {
            opened
        }
        reopened => return Ok(Written::Stale(prepared, reopened)),
    };
    let Prepared {
        mut values,
        mut errors,
        named,
        ..
    } = prepared;
    let schema = Arc::clone(&child.form);
    complete(&schema, &mut values, &named, &opened.stored);
    values.insert(foreign_key, FormValue::Text(owner.clone()));
    if opened.record.is_some() {
        schema
            .keep_unseen_links(cx, &mut values, &opened.stored, &mut *ex)
            .await;
    }
    errors.extend(check_unique(cx, &child, &schema, &values, &opened.stored, &mut *ex).await?);
    let form = parse_form(cx, &child, &values, &mut errors)?;
    // The owner fills the foreign key with the record this request just wrote: it is no link
    // the user picked, and the owner's query may not list it yet.
    let mut linked = opened.stored.clone();
    linked.insert(foreign_key, FormValue::Text(owner));
    recheck_relationships(cx, &schema, &values, &linked, &mut errors, &mut *ex).await;
    let refused = |values, errors| {
        Written::Refused(Prepared {
            values,
            errors,
            named: HashSet::new(),
            against: None,
            version: None,
        })
    };
    let Some(form) = form.filter(|_| errors.is_empty()) else {
        return Ok(refused(values, errors));
    };
    let change = match opened.record {
        None => match C::create_record(cx, form, &mut *ex).await {
            Ok(record) => Change::Create(record),
            Err(WriteError::Refused(errors)) => {
                return Ok(refused(values, refusal(&child, "create_record", errors)?));
            }
            Err(WriteError::Failed(error)) => return Err(error),
        },
        Some(record) => {
            let record = *record
                .downcast::<C::Model>()
                .expect("`open::<C>` holds a `C::Model`");
            let before = record.clone();
            let posted = Posted::new(form, named_fields(&child, &named));
            match C::update_record(cx, record, posted, &mut *ex).await {
                Ok(after) => Change::Update { before, after },
                Err(WriteError::Refused(errors)) => {
                    return Ok(refused(values, refusal(&child, "update_record", errors)?));
                }
                Err(WriteError::Failed(error)) => return Err(error),
            }
        }
    };
    C::before_commit(cx, &change, &mut *ex).await?;
    crate::panel::write::end_changed_credentials(cx, &mut *ex, &change).await?;
    Ok(Written::Done(Box::new(move |cx| {
        Box::pin(crate::resource::run_after_commit::<C>(cx, change))
    })))
}

/// Renders `C`'s form as a section of its owner's form, each control posting under `{key}.`
/// and the foreign key renders none.
fn render<C: Resource>(cx: &Cx, shown: Shown) -> BoxView<'_> {
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
            view! { cx => <input type="hidden" name=(name) value=(version.to_string())> }
        });
        let values = mask.shown(&values.unwrap_or_else(|| child.form.defaults()));
        let source = Source::form(&values, &errors)
            .editing(editing)
            .masked(mask)
            .scoped(key.clone())
            .posting_under(format!("{key}."));
        let body = child.form.render(cx, source).await?;
        Ok(view! {
            cx =>
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
                (crate::schema::titled(cx, label, body))
            </div>
        }
        .boxed())
    })
}

/// The record an [`Opened`] of `C` holds.
fn child_record<C: Resource>(record: &(dyn Any + Send + Sync)) -> &C::Model {
    record
        .downcast_ref::<C::Model>()
        .expect("`open::<C>` holds a `C::Model`")
}
