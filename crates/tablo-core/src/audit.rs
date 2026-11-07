//! The audit log's rows: the [`AuditEntry`] each write of a panel with
//! [`Panel::audit_log`](crate::Panel::audit_log) stores, and the [`AuditChange`]s it names.

use jiff::Timestamp;
use uuid::Uuid;

use crate::TenantId;

/// Shipped record of one thing a panel user did: who, on which panel and for which tenant, to
/// which resource and record, and which fields it changed.
///
/// A panel with [`Panel::audit_log`](crate::Panel::audit_log) writes one per record of each
/// write, inside the write's transaction, and [`AuditEvent::record`](crate::AuditEvent::record)
/// writes the app's own. Nothing updates or deletes one but
/// [`PanelHandle::prune_audit_log`](crate::PanelHandle::prune_audit_log).
#[derive(Debug, Clone, toasty::Model)]
#[index(panel, created_at)]
pub struct AuditEntry {
    #[key]
    #[auto]
    pub id: Uuid,
    /// The prefix of the panel it happened on, e.g. `/admin`.
    pub panel: String,
    /// The tenant the user acted for; `None` outside every tenant.
    pub tenant: Option<TenantId>,
    /// [`PanelUser::user_id`](crate::PanelUser::user_id) of the user who did it; `None` for a
    /// request without one.
    pub actor_id: Option<String>,
    /// The user's [`display_name`](crate::PanelUser::display_name) when it happened, kept so the
    /// entry still names a user who is gone.
    pub actor: String,
    /// What happened: `create`, `update`, `delete`, `restore`, `force_delete`, `attach`,
    /// `detach`, `action:<name>`, `header_action:<name>`, or the app's own event.
    pub event: String,
    /// The slug of the resource it happened to, as its URLs spell it; `None` for an app event on
    /// no resource.
    pub resource: Option<String>,
    /// The record's key, as its URLs spell it; `None` for an event on no record.
    pub record: Option<String>,
    /// The record's detail page when it happened; `None` when the resource served none.
    pub url: Option<String>,
    /// The fields it changed, as JSON, so a field a later release adds to [`AuditChange`] needs
    /// no migration.
    #[column(type = text)]
    pub changes: toasty::Json<Vec<AuditChange>>,
    pub created_at: Timestamp,
}

// By hand, naming no tenant column: the log keeps every tenant's entries and its resource narrows
// them to the reader's own, so mounting it must neither mark the panel as acting for tenants nor
// ask it for a tenancy. Its resource declares its own table and detail page.
impl crate::ModelColumns for AuditEntry {
    const TENANT_COLUMNS: &'static [&'static str] = &[];
}

/// One field an [`AuditEntry`] changed: its name, its label in the writer's locale, and its
/// spelling before and after, `None` where it held nothing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditChange {
    /// The column's name, the model field it reads: `title`.
    pub field: String,
    /// The column's label as the writer read it: `Title`.
    pub label: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// The app's own event for the audit log of a panel with
/// [`Panel::audit_log`](crate::Panel::audit_log): a sign-in, an export, a payment, beside what the
/// panel's writes record.
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
    pub(crate) event: String,
    pub(crate) resource: Option<String>,
    pub(crate) record: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) changes: Vec<AuditChange>,
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

    /// Stores the event on `panel` for `tenant`, done by `actor`, its id and display name, through
    /// `ex`: what [`record`](Self::record) and auth's own events share.
    pub(crate) async fn write(
        self,
        ex: &mut dyn toasty::Executor,
        panel: String,
        tenant: Option<TenantId>,
        actor: Option<(String, String)>,
    ) -> toasty::Result<AuditEntry> {
        let (actor_id, actor) = actor.map_or((None, String::new()), |(id, name)| (Some(id), name));
        toasty::create!(AuditEntry {
            panel,
            tenant,
            actor_id,
            actor,
            event: self.event,
            resource: self.resource,
            record: self.record,
            url: self.url,
            changes: toasty::Json(self.changes),
            created_at: Timestamp::now(),
        })
        .exec(ex)
        .await
    }
}
