//! [`AuditLog`]: the read-only resource that lists the [`AuditEntry`] rows of its panel.

use tablo_core::{
    NoForm, ReadOnly, Resource, ResourceDef, Tenancy, Text, WriteError, mounted_resources,
};
use toasty::Executor;
use topcoat::context::Cx;

use crate::AuditEntry;

mod columns;

/// The read-only resource listing the [`AuditEntry`] rows of its panel, newest first, at
/// `{prefix}/audit-log`, or under the area that places it.
/// [`audit_log`](crate::PanelAuditExt::audit_log) registers it.
///
/// It lists what happened on its own panel only, for the tenant the reader acts for, and to the
/// resources the reader may list: an entry about a resource whose list the reader may not open
/// stays out of it, and a field [`field_access`](tablo_core::ResourceDef::field_access) hides
/// from the reader reads "Hidden". Whatever policy an app gives it, it allows reads only.
pub struct AuditLog;

impl Resource for AuditLog {
    type Model = AuditEntry;
    type Form = NoForm<AuditEntry>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("audit-log")
            .label(Text::message("tablo-audit-entry"))
            .plural_label(Text::message("tablo-audit-log"))
            // The log keeps what happened: no policy the app sets writes through its pages.
            .restrict(ReadOnly)
            // Every entry carries its tenant; `scope` keeps the reader's own.
            .tenancy(Tenancy::none())
            .table(columns::table())
            .detail(columns::detail())
            .query_fn(scope)
    }

    // Past `ReadOnly`, which every policy the def holds passes through, a delete refuses too.
    async fn delete_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> Result<(), WriteError> {
        Err(immutable())
    }

    async fn restore_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> Result<(), WriteError> {
        Err(immutable())
    }

    async fn force_delete_record(
        _cx: &Cx,
        _record: &AuditEntry,
        _ex: &mut dyn Executor,
    ) -> Result<(), WriteError> {
        Err(immutable())
    }
}

/// The refusal of every write through the log's pages.
fn immutable() -> WriteError {
    WriteError::declined(Text::message("tablo-audit-immutable"))
}

/// The entries `cx` may read: its panel's, for the tenant it acts for, about the resources it
/// may list or about none.
fn scope(cx: &Cx) -> toasty::stmt::Expr<bool> {
    let Some(panel) = tablo_core::url::panel(cx) else {
        return toasty::stmt::IntoExpr::into_expr(false);
    };
    let fields = AuditEntry::fields();
    let tenant = match tablo_core::tenant_id(cx) {
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
    fields.panel().eq(panel).and(tenant).and(resource)
}

/// The slugs of the panel's resources `cx` may list. A slug two resources share, in two areas,
/// counts only when `cx` may list both: an entry names the slug alone.
fn listable_slugs(cx: &Cx) -> Vec<Option<String>> {
    let mounted = mounted_resources(cx);
    let mut slugs: Vec<Option<String>> = Vec::new();
    for resource in &mounted {
        let slug = Some(resource.slug().to_owned());
        let shared = mounted
            .iter()
            .filter(|other| other.slug() == resource.slug());
        if !slugs.contains(&slug) && shared.clone().all(|other| other.can_list(cx)) {
            slugs.push(slug);
        }
    }
    slugs
}
