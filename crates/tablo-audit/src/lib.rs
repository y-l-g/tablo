//! The audit log for Tablo panels: an [`AuditEntry`] for each record every write of a panel
//! names, stored inside the write's transaction, and the read-only [`AuditLog`] resource that
//! lists them.
//!
//! It is built on Tablo's public API alone: a [`CommitHook`](tablo_core::CommitHook) stores the
//! writes, an [`AuthHook`](tablo_core::AuthHook) stores what auth does, and [`AuditLog`] is an
//! ordinary [`Resource`](tablo_core::Resource). [`PanelAuditExt`] registers them on a
//! [`Panel`]:
//!
//! ```rust
//! # use tablo_core::{Panel, when};
//! # use topcoat::context::Cx;
//! # fn is_admin(cx: &Cx) -> bool { false }
//! use tablo_audit::PanelAuditExt;
//!
//! Panel::new("admin").audit_log(|log| log.policy(when(is_admin)));
//! ```
//!
//! The `Db` registers [`AuditEntry`]: list it in the app's
//! [`models!`](tablo_core::models!), as `tablo::models!(Post, tablo::audit::AuditEntry)`.

mod entry;
mod impersonation;
mod log;
mod trail;

use std::time::Duration;

use tablo_core::{Panel, PanelHandle, ResourceDef, TenantId};
use topcoat::{context::Cx, l10n::locale};

pub use self::{
    entry::{AuditChange, AuditEntry, AuditEvent},
    impersonation::{ImpersonationEvent, impersonations},
    log::AuditLog,
};

/// The catalogs the log's words read from, under Tablo's and the app's.
const CATALOGS: &[(topcoat::l10n::Locale, &str)] = &[
    (locale!("en"), include_str!("../locales/en.ftl")),
    (locale!("fr"), include_str!("../locales/fr.ftl")),
];

/// The audit log's registrations on a [`Panel`].
pub trait PanelAuditExt {
    /// Keeps an [`AuditEntry`] for each record every write of the panel names, inside the write's
    /// transaction, as [`audit_trail`](Self::audit_trail) does, and lists them through the
    /// [`AuditLog`] resource at `{prefix}/audit-log`, with `customize` adjusting its def.
    ///
    /// The log is denied to everyone until `customize` gives it a policy, and that policy allows
    /// reads only. A reader sees the entries of the resources whose list they may open, whatever
    /// row scope each resource's own query keeps, so the log's policy should be no wider than the
    /// narrowest of them.
    #[must_use]
    fn audit_log(
        self,
        customize: impl FnOnce(ResourceDef<AuditLog>) -> ResourceDef<AuditLog> + Send + 'static,
    ) -> Self;

    /// Keeps an [`AuditEntry`] for each record every write of the panel names, inside the write's
    /// transaction, and one for each [`AuthEvent`](tablo_core::AuthEvent) of the panel, a sign-in
    /// or a change of credentials among them, without listing them: an [`Area`](tablo_core::Area)
    /// group lists them with `group.resource_with::<AuditLog>(..)`. Calling it again changes
    /// nothing.
    ///
    /// A CSV export stores an entry too, with the number of rows it hands over, before the first
    /// byte leaves; an export whose entry fails to store is refused.
    ///
    /// Each entry names the [actor](tablo_core::auth::actor), the tenant they act for, the
    /// resource and record, and the fields the write changed, as a
    /// [`CommitHook`](tablo_core::CommitHook) reads them: a field marked `#[columns(skip)]`, such
    /// as a password hash, is never stored. A write whose entry fails to store rolls back with
    /// it, and so does a sign-in or a change of credentials; a refused sign-in, a sign-out and a
    /// stop of impersonation go through unrecorded. A refused sign-in stores the login as it
    /// was typed. Mounting refuses a `Db` that does not
    /// register [`AuditEntry`], and an [`AuditLog`] on a panel without the trail.
    #[must_use]
    fn audit_trail(self) -> Self;
}

impl PanelAuditExt for Panel {
    fn audit_log(
        self,
        customize: impl FnOnce(ResourceDef<AuditLog>) -> ResourceDef<AuditLog> + Send + 'static,
    ) -> Self {
        self.audit_trail().resource_with::<AuditLog>(customize)
    }

    fn audit_trail(self) -> Self {
        CATALOGS.iter().fold(
            self.before_commit::<trail::Trail>()
                .auth_hook::<trail::Trail>()
                .require_model::<AuditEntry>(),
            |panel, (locale, catalog)| panel.default_translations(locale.clone(), *catalog),
        )
    }
}

/// Deletes the [`AuditEntry`] rows of `panel` older than `age`. Nothing else deletes them but
/// [`forget_tenant`]; a job calls it on the schedule the app keeps its audit trail for.
///
/// # Errors
///
/// The driver's failure when the rows cannot be deleted.
pub async fn prune(panel: &PanelHandle, age: Duration) -> topcoat::Result<()> {
    let cutoff = jiff::Timestamp::now().checked_sub(jiff::SignedDuration::try_from(age)?)?;
    let fields = AuditEntry::fields();
    AuditEntry::filter(
        fields
            .panel()
            .eq(panel.id().to_string())
            .and(fields.created_at().lt(cutoff)),
    )
    .delete()
    .exec(&mut tablo_core::db::db(&panel.context())?)
    .await?;
    Ok(())
}

/// Keeps what the user `user_id` did, but no longer who did it, for an account being erased: each
/// entry they acted in names a deleted user, the `tablo-audit-deleted-user` message, in their
/// place, and each auth entry done to their account, a sign-in or an impersonation of them, no
/// longer holds their name. The entries' events, records and fields stay, as does the account's
/// id, which names no one once the account is gone. A refused sign-in names no account and keeps
/// the login it typed until [`prune`] deletes it. Run it through the transaction that deletes
/// the account.
///
/// # Errors
///
/// The driver's failure when the rows cannot be read or written.
pub async fn forget_actor(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
) -> topcoat::Result<()> {
    let deleted = tablo_core::t!(cx, "tablo-audit-deleted-user");
    let fields = AuditEntry::fields();
    AuditEntry::filter(fields.actor_id().eq(Some(user_id.to_owned())))
        .update()
        .actor_id(None::<String>)
        .actor(deleted.clone())
        .exec(&mut *ex)
        .await?;
    let done_to = AuditEntry::filter(
        fields
            .record()
            .eq(Some(user_id.to_owned()))
            .and(fields.resource().is_none()),
    )
    .exec(&mut *ex)
    .await?;
    for mut entry in done_to {
        let mut changes = entry.changes.0.clone();
        let mut scrubbed = false;
        for change in &mut changes {
            if change.field == impersonation::TARGET_FIELD {
                change.after = Some(deleted.clone());
                scrubbed = true;
            }
        }
        if scrubbed {
            entry
                .update()
                .changes(toasty::Json(changes))
                .exec(&mut *ex)
                .await?;
        }
    }
    Ok(())
}

/// Deletes the entries filed under `tenant`, for a tenant being erased, but those recording an
/// [impersonation](impersonations): a start is filed under the actor's tenant, and stays as the
/// record of who reached which account until [`prune`] deletes it. Entries of panels that act for
/// no tenant name none, and stay.
///
/// # Errors
///
/// The driver's failure when the rows cannot be deleted.
pub async fn forget_tenant(ex: &mut dyn toasty::Executor, tenant: TenantId) -> topcoat::Result<()> {
    AuditEntry::filter(
        AuditEntry::fields()
            .tenant()
            .eq(Some(tenant))
            .and(impersonations().not()),
    )
    .delete()
    .exec(ex)
    .await?;
    Ok(())
}
