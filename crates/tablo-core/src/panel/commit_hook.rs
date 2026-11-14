//! [`Panel::before_commit`]: the hooks every write of a panel runs inside its transaction, whichever
//! resource wrote it.

use std::{any::TypeId, future::Future};

use toasty::Executor;
use topcoat::{Result, context::Cx};

use super::{Panel, state::current};
use crate::{
    BoxFuture, PanelModel, Text,
    resource::{Change, Resource, require_mounted},
};

/// Work every write of a panel does inside its transaction, whichever of the panel's resources
/// wrote it: an audit trail no resource can leave out. [`Panel::before_commit`] registers it.
///
/// It runs on the writes [`Resource::before_commit`] runs on, and on a CSV export, right after
/// that hook, with the
/// same transaction and the same rules: run every statement through `ex`, and an `Err` rolls the
/// whole write back and fails the request with the write's failure toast.
///
/// ```rust
/// # use tablo_core::{Change, CommitHook, PanelChange};
/// # use topcoat::{Result, context::Cx};
/// # #[derive(Debug, toasty::Model)]
/// # struct AuditEntry { #[key] #[auto] id: uuid::Uuid, resource: String, record: String }
/// struct Audit;
///
/// impl CommitHook for Audit {
///     async fn before_commit(
///         cx: &Cx,
///         change: &PanelChange,
///         ex: &mut dyn toasty::Executor,
///     ) -> Result<()> {
///         for record in change.change().records() {
///             toasty::create!(AuditEntry {
///                 resource: change.label().localize(cx),
///                 record: record.key().to_string(),
///             })
///             .exec(&mut *ex)
///             .await?;
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait CommitHook: 'static {
    /// Work inside the write's transaction, after the resource's own
    /// [`Resource::before_commit`].
    fn before_commit(
        cx: &Cx,
        change: &PanelChange,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// A [`Change`] as a [`CommitHook`] receives it: the resource that wrote it, and each record it
/// names by its key, its URL and the fields the write changed.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelChange {
    slug: String,
    label: Text,
    change: Change<ChangedRecord>,
}

impl PanelChange {
    /// The slug of the resource that wrote, as its URLs spell it: `posts`.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// The resource's label: `Post`, or the message its catalog spells it with.
    pub fn label(&self) -> &Text {
        &self.label
    }

    /// What the write changed, each record named by its key, URL and fields.
    pub fn change(&self) -> &Change<ChangedRecord> {
        &self.change
    }
}

/// A record a [`PanelChange`] names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedRecord {
    key: String,
    url: Option<String>,
    fields: Vec<ChangedField>,
}

impl ChangedRecord {
    /// The record's key, as its URLs spell it: `42`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The record's detail page, as [`url::record`](crate::url::record) spells it; `None` when
    /// the resource serves none.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// The fields the write changed on this record, each with its value before and after it.
    ///
    /// They are the fields the model's [`PanelModel::table`] lists, each spelled as that column
    /// spells it in an export, in the request's locale, then its foreign keys and
    /// [`TenantId`](crate::TenantId) fields, which the table leaves out, spelled as the database
    /// stores them: a field marked `#[columns(skip)]`, such as a password hash, never appears. A
    /// create names each field it filled, an update's `after` record each field whose spelling
    /// changed, and a delete's or a force delete's records each field they held. The other
    /// records name none: a restore, an action, an attach and a detach write what the model's
    /// columns do not say, and an update's `before` record leaves the change to its `after`.
    pub fn fields(&self) -> &[ChangedField] {
        &self.fields
    }
}

/// One field a write changed on a [`ChangedRecord`]: its name, its label, and its spelling before
/// and after the write, `None` where it held nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedField {
    name: String,
    label: Text,
    before: Option<String>,
    after: Option<String>,
}

impl ChangedField {
    /// The column's name, the model field it reads: `title`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The column's label: `Title`, or the message its catalog spells it with.
    pub fn label(&self) -> &Text {
        &self.label
    }

    /// The field's spelling before the write; `None` for a create, or a field that held nothing.
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }

    /// The field's spelling after the write; `None` for a delete, or a field left empty.
    pub fn after(&self) -> Option<&str> {
        self.after.as_deref()
    }
}

/// The fields `before` and `after` spell differently, by the columns of `M`'s default table and
/// its [`references`](PanelModel::references); a record that is absent spells every field empty.
fn changed_fields<M: PanelModel>(
    cx: &Cx,
    before: Option<&M>,
    after: Option<&M>,
) -> Vec<ChangedField> {
    let table = M::table();
    let (before_record, after_record) = (before, after);
    let spell = |record: Option<&M>| record.map(|record| table.spell(cx, record));
    let (before, after) = (spell(before), spell(after));
    let columns = after.as_ref().or(before.as_ref()).map_or(0, Vec::len);
    let held = |spelled: &Option<Vec<(String, Text, String)>>, index: usize| {
        spelled
            .as_ref()
            .map(|spelled| spelled[index].2.clone())
            .filter(|text| !text.is_empty())
    };
    let mut changed: Vec<ChangedField> = (0..columns)
        .filter_map(|index| {
            let (was, is) = (held(&before, index), held(&after, index));
            let (name, label, _) = after.as_ref().or(before.as_ref())?[index].clone();
            (was != is).then_some(ChangedField {
                name,
                label,
                before: was,
                after: is,
            })
        })
        .collect();
    let references = |record: Option<&M>| record.map(PanelModel::references);
    let (was, is) = (references(before_record), references(after_record));
    let names = is.as_ref().or(was.as_ref()).cloned().unwrap_or_default();
    for (index, (name, _)) in names.into_iter().enumerate() {
        let held = |spelled: &Option<Vec<(&str, Option<String>)>>| {
            spelled
                .as_ref()
                .and_then(|spelled| spelled[index].1.clone())
        };
        let (before, after) = (held(&was), held(&is));
        if before != after && changed.iter().all(|field| field.name != name) {
            changed.push(ChangedField {
                name: name.to_owned(),
                label: Text::field(name),
                before,
                after,
            });
        }
    }
    changed
}

/// `change` with each record named by its key and URL, and the fields the write changed on it.
fn changed_records<R: Resource>(
    cx: &Cx,
    change: &Change<R::Model>,
    key: impl Fn(&R::Model) -> String,
) -> Change<ChangedRecord> {
    let named = |record: &R::Model, fields| ChangedRecord {
        key: key(record),
        url: crate::url::record::<R>(cx, record),
        fields,
    };
    let held = |records: &[R::Model]| {
        records
            .iter()
            .map(|record| named(record, changed_fields(cx, Some(record), None)))
            .collect()
    };
    match change {
        Change::Create(record) => Change::Create(named(
            record,
            changed_fields::<R::Model>(cx, None, Some(record)),
        )),
        Change::Update { before, after } => Change::Update {
            before: named(before, Vec::new()),
            after: named(after, changed_fields(cx, Some(before), Some(after))),
        },
        Change::Delete(records) => Change::Delete(held(records)),
        Change::ForceDelete(records) => Change::ForceDelete(held(records)),
        other => other.map(|record| named(record, Vec::new())),
    }
}

/// A registered [`CommitHook`], its type erased.
pub(crate) type CommitHookFn =
    for<'a> fn(&'a Cx, &'a PanelChange, &'a mut dyn Executor) -> BoxFuture<'a, Result<()>>;

fn erased<'a, H: CommitHook>(
    cx: &'a Cx,
    change: &'a PanelChange,
    ex: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(H::before_commit(cx, change, ex))
}

impl Panel {
    /// Runs the [`CommitHook`] `H` inside the transaction of every write of the panel, after the
    /// writing resource's own [`Resource::before_commit`]. Several hooks run in the order they
    /// are registered; registering `H` again changes nothing, so it never runs twice.
    pub fn before_commit<H: CommitHook>(mut self) -> Self {
        let id = TypeId::of::<H>();
        if !self.commit_hooks.iter().any(|(hook, _)| *hook == id) {
            self.commit_hooks.push((id, erased::<H>));
        }
        self
    }
}

impl<R: Resource> crate::ResourceDef<R> {
    /// Refuses to mount the resource on a panel that does not run the [`CommitHook`] `H`
    /// ([`MissingCommitHook`](crate::DeclarationErrorKind::MissingCommitHook)): a list of what
    /// `H` stores would stay empty, however it is placed.
    #[must_use]
    pub fn requires_commit_hook<H: CommitHook>(mut self) -> Self {
        let hook = (TypeId::of::<H>(), std::any::type_name::<H>());
        if !self.required_hooks.contains(&hook) {
            self.required_hooks.push(hook);
        }
        self
    }
}

/// A fault for each hook a registered resource requires that `hooks` leaves out.
pub(super) fn missing_commit_hooks(
    resources: &[super::register::Registered],
    hooks: &[(TypeId, CommitHookFn)],
) -> Vec<crate::DeclarationError> {
    resources
        .iter()
        .flat_map(|resource| {
            resource
                .required_hooks
                .iter()
                .map(move |hook| (resource, hook))
        })
        .filter(|(_, (id, _))| !hooks.iter().any(|(hook, _)| hook == id))
        .map(|(resource, (_, hook))| crate::DeclarationError {
            resource: Some(resource.name),
            site: crate::Site::Registration,
            kind: crate::DeclarationErrorKind::MissingCommitHook { hook },
        })
        .collect()
}

/// Run `R`'s [`Resource::before_commit`] on `change`, then each of the panel's [`CommitHook`]s,
/// inside the write's transaction `ex`.
pub(crate) async fn before_commit<R: Resource>(
    cx: &Cx,
    change: &Change<R::Model>,
    ex: &mut dyn Executor,
) -> Result<()> {
    R::before_commit(cx, change, &mut *ex).await?;
    let hooks = current(cx).map_or(&[][..], |panel| panel.commit_hooks.as_slice());
    if hooks.is_empty() {
        return Ok(());
    }
    let mounted = require_mounted::<R>(cx)?;
    let change = PanelChange {
        slug: mounted.slug.clone(),
        label: mounted.label.clone(),
        change: changed_records::<R>(cx, change, |record| mounted.table.key_of(record)),
    };
    run(cx, hooks, &change, ex).await
}

/// Run each of `hooks` on `change`, stopping at the first error.
fn run<'a>(
    cx: &'a Cx,
    hooks: &'a [CommitHookFn],
    change: &'a PanelChange,
    ex: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(async move {
        for hook in hooks {
            hook(cx, change, &mut *ex).await?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
