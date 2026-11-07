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
