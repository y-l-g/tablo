//! [`Panel::audit_log`]: every write of a panel kept as [`AuditEntry`] rows inside the write's
//! transaction, and the read-only [`AuditLog`] resource that lists them.

use std::sync::Arc;

use jiff::Timestamp;
use toasty::Executor;
use topcoat::{Result, context::Cx};

use super::{Panel, PanelChange, state::current};
use crate::{
    Ability, ChangedRecord, NoForm, Policy, Resource, ResourceDef, Tenancy, Text,
    audit::{AuditChange, AuditEntry},
    resource::Change,
};

mod columns;

/// The read-only resource listing the [`AuditEntry`] rows of its panel, newest first, at
/// `{prefix}/audit-log`. [`Panel::audit_log`] registers it.
///
/// It lists what happened on its own panel only, for the tenant the reader acts for, and to the
/// resources the reader may list: an entry about a resource whose list the reader may not open
/// stays out of it, and a field [`field_access`](crate::ResourceDef::field_access) hides from the
/// reader reads "Hidden".
pub struct AuditLog;

impl Resource for AuditLog {
    type Model = AuditEntry;
    type Form = NoForm<AuditEntry>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("audit-log")
            .label(Text::message("tablo-audit-entry"))
            .plural_label(Text::message("tablo-audit-log"))
            // Every entry carries its tenant; `scope` keeps the reader's own.
            .tenancy(Tenancy::none())
            .table(columns::table())
            .detail(columns::detail())
            .query(scope)
    }

    // The log keeps what happened: even registered by hand, past [`Panel::audit_log`]'s policy,
    // its pages delete nothing.
    async fn delete_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> std::result::Result<(), crate::WriteError> {
        Err(immutable())
    }

    async fn restore_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> std::result::Result<(), crate::WriteError> {
        Err(immutable())
    }

    async fn force_delete_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> std::result::Result<(), crate::WriteError> {
        Err(immutable())
    }
}

/// The refusal of every write through the log's pages.
fn immutable() -> crate::WriteError {
    crate::WriteError::declined(Text::message("tablo-audit-immutable"))
}

/// The entries `cx` may read: its panel's, for the tenant it acts for, about the resources it
/// may list or about none.
fn scope(
    cx: &Cx,
    query: toasty::stmt::Query<toasty::stmt::List<AuditEntry>>,
) -> toasty::stmt::Query<toasty::stmt::List<AuditEntry>> {
    let Some(panel) = current(cx) else {
        return query.filter(crate::toasty_compat::never());
    };
    let fields = AuditEntry::fields();
    let tenant = match crate::tenant_id(cx) {
        Some(tenant) => fields.tenant().eq(Some(tenant)),
        None => fields.tenant().is_none(),
    };
    let listable = listable_slugs(cx);
    let resource = if listable.is_empty() {
        fields.resource().is_none()
    } else {
        fields
            .resource()
            .is_none()
            .or(fields.resource().in_list(listable))
    };
    query.filter(
        fields
            .panel()
            .eq(panel.realm.prefix.clone())
            .and(tenant)
            .and(resource),
    )
}

/// The slugs of the panel's resources `cx` may list. A slug two resources share, in two areas,
/// counts only when `cx` may list both: an entry names the slug alone.
fn listable_slugs(cx: &Cx) -> Vec<Option<String>> {
    let Some(panel) = current(cx) else {
        return Vec::new();
    };
    let mut slugs: Vec<Option<String>> = Vec::new();
    for child in panel.children.values() {
        let shared = || {
            panel
                .children
                .values()
                .filter(|other| other.slug == child.slug)
        };
        let slug = Some(child.slug.clone());
        if !slugs.contains(&slug) && shared().all(|other| (other.listable)(cx)) {
            slugs.push(slug);
        }
    }
    slugs
}

/// Allows the reads its inner policy allows, and nothing else: the log is never written through
/// its pages, whatever policy the app gives it.
struct Immutable(Arc<dyn Policy<AuditEntry>>);

impl Policy<AuditEntry> for Immutable {
    fn allows(&self, cx: &Cx, ability: Ability<'_, AuditEntry>) -> bool {
        ability.is_read() && self.0.allows(cx, ability)
    }
}

impl Panel {
    /// Keeps an [`AuditEntry`] for each record every write of the panel names, inside the write's
    /// transaction, and lists them at `{prefix}/audit-log` through the [`AuditLog`] resource.
    ///
    /// Each entry names the signed-in user, the tenant they act for, the resource and record,
    /// and the fields the write changed, as a [`CommitHook`](crate::CommitHook) reads them: a
    /// field marked `#[columns(skip)]`, such as a password hash, is never stored. A write whose
    /// entry fails to store rolls back with it.
    ///
    /// A reader sees the entries of the resources whose list they may open, whatever row scope
    /// each resource's own query keeps, so the log's policy should be no wider than the narrowest
    /// of them. A delete keeps every field the record held until
    /// [`prune_audit_log`](crate::PanelHandle::prune_audit_log) removes the entry.
    ///
    /// The log is denied to everyone until `customize` gives it a policy, and that policy allows
    /// reads only:
    ///
    /// ```rust
    /// # use tablo_core::{AuditLog, Panel, PanelUser, auth, when};
    /// # use topcoat::context::Cx;
    /// # fn is_admin(cx: &Cx) -> bool { false }
    /// Panel::new("admin").audit_log(|log| log.policy(when(is_admin)));
    /// ```
    ///
    /// The database registers [`AuditEntry`]: [`models!`](crate::models!) does.
    #[must_use]
    pub fn audit_log(
        mut self,
        customize: impl FnOnce(ResourceDef<AuditLog>) -> ResourceDef<AuditLog> + Send + 'static,
    ) -> Self {
        self.commit_hooks
            .push(|cx, change, ex| Box::pin(store(cx, change, ex)));
        self.resource_with::<AuditLog>(move |def| {
            let mut def = customize(def);
            def.policy = Arc::new(Immutable(def.policy));
            def
        })
    }
}

/// The app's own event for the audit log of a panel with [`Panel::audit_log`]: a sign-in, an
/// export, a payment, beside what the panel's writes record.
///
/// ```rust,no_run
/// # use tablo_core::AuditEvent;
/// # use topcoat::context::Cx;
/// # async fn exported(cx: &Cx, ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
/// AuditEvent::new("export")
///     .resource("orders")
///     .record(cx, ex)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AuditEvent {
    event: String,
    resource: Option<String>,
    record: Option<String>,
    url: Option<String>,
    changes: Vec<AuditChange>,
}

impl AuditEvent {
    /// An event named `event`, on no resource: the log shows the name as it is.
    pub fn new(event: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            resource: None,
            record: None,
            url: None,
            changes: Vec::new(),
        }
    }

    /// The slug of the resource it happened to: only readers who may list that resource see it.
    #[must_use]
    pub fn resource(mut self, slug: impl Into<String>) -> Self {
        self.resource = Some(slug.into());
        self
    }

    /// The key of the record it happened to, and its page when it has one.
    #[must_use]
    pub fn key(mut self, key: impl Into<String>, url: Option<String>) -> Self {
        self.record = Some(key.into());
        self.url = url;
        self
    }

    /// A field it changed, from `before` to `after`.
    #[must_use]
    pub fn change(
        mut self,
        field: impl Into<String>,
        label: impl Into<String>,
        before: Option<String>,
        after: Option<String>,
    ) -> Self {
        self.changes.push(AuditChange {
            field: field.into(),
            label: label.into(),
            before,
            after,
        });
        self
    }

    /// Stores the event for the request's panel, user and tenant through `ex`: the write's
    /// transaction inside a [`CommitHook`](crate::CommitHook) or
    /// [`Resource::before_commit`], or the panel's [`Db`](toasty::Db) outside one.
    ///
    /// # Errors
    ///
    /// A declaration error outside every panel; an opaque infrastructure failure when the row
    /// cannot be written, the driver's text going to the log.
    pub async fn record(self, cx: &Cx, ex: &mut dyn Executor) -> Result<AuditEntry> {
        let Some(realm) = crate::auth::current_realm(cx) else {
            return Err(crate::error::declaration(
                "AuditEvent::record needs a request under a panel".to_owned(),
            ));
        };
        let prefix = realm.prefix.clone();
        insert(cx, ex, prefix, self).await
    }
}

/// The mount's refusal of a panel whose registrations, by the URLs they serve at, include the
/// [`AuditLog`], over a `db` that does not register [`AuditEntry`].
pub(super) fn declaration_error(
    db: &toasty::Db,
    urls: &std::collections::HashMap<std::any::TypeId, String>,
) -> Option<crate::DeclarationError> {
    let kept = urls.contains_key(&std::any::TypeId::of::<AuditLog>());
    (kept && !crate::toasty_compat::model::AppSchema::of_db(db).registers("AuditEntry"))
        .then(|| crate::DeclarationError::panel(crate::DeclarationErrorKind::MissingAuditModel))
}

/// Store an entry for each record `change` names.
async fn store(cx: &Cx, change: &PanelChange, ex: &mut dyn Executor) -> Result<()> {
    let Some(panel) = current(cx) else {
        return Ok(());
    };
    let prefix = panel.realm.prefix.clone();
    for event in events(cx, change) {
        insert(cx, &mut *ex, prefix.clone(), event).await?;
    }
    Ok(())
}

/// The events `change` records: one per record it names, or one on no record for a header
/// action.
fn events(cx: &Cx, change: &PanelChange) -> Vec<AuditEvent> {
    let on = |name: &str, record: &ChangedRecord| {
        let event = AuditEvent::new(name)
            .resource(change.slug())
            .key(record.key(), record.url().map(str::to_owned));
        record.fields().iter().fold(event, |event, field| {
            event.change(
                field.name(),
                field.label().localize(cx),
                field.before().map(str::to_owned),
                field.after().map(str::to_owned),
            )
        })
    };
    let each = |name: &str, records: &[ChangedRecord]| {
        records.iter().map(|record| on(name, record)).collect()
    };
    match change.change() {
        Change::Create(record) => vec![on("create", record)],
        // A save that changed no field the log reads writes no entry.
        Change::Update { after, .. } if after.fields().is_empty() => Vec::new(),
        Change::Update { after, .. } => vec![on("update", after)],
        Change::Delete(records) => each("delete", records),
        Change::Restore(records) => each("restore", records),
        Change::ForceDelete(records) => each("force_delete", records),
        Change::Action { name, records } => each(&format!("action:{name}"), records),
        Change::HeaderAction { name } => {
            vec![AuditEvent::new(format!("header_action:{name}")).resource(change.slug())]
        }
        Change::Attach {
            owner,
            relation,
            key,
        } => vec![on("attach", owner).change(relation, relation, None, Some(key.clone()))],
        Change::Detach {
            owner,
            relation,
            keys,
        } => vec![on("detach", owner).change(relation, relation, Some(keys.join(", ")), None)],
    }
}

/// Write `event` as an entry of the panel at `prefix`, by the request's user and tenant.
async fn insert(
    cx: &Cx,
    ex: &mut dyn Executor,
    prefix: String,
    event: AuditEvent,
) -> Result<AuditEntry> {
    let user = crate::auth::signed(cx).map(|signed| signed.user.clone());
    toasty::create!(AuditEntry {
        panel: prefix,
        tenant: crate::tenant_id(cx),
        actor_id: user.as_ref().map(|user| user.user_id()),
        actor: user
            .as_ref()
            .map(|user| user.display_name().to_owned())
            .unwrap_or_default(),
        event: event.event,
        resource: event.resource,
        record: event.record,
        url: event.url,
        changes: toasty::Json(event.changes),
        created_at: Timestamp::now(),
    })
    .exec(ex)
    .await
    .map_err(|error| crate::error::driver_failure(error, "The audit log is unavailable right now."))
}
