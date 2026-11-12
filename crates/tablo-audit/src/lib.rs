//! The audit log for Tablo panels: an [`AuditEntry`] for each record every write of a panel
//! names, stored inside the write's transaction, and the read-only [`AuditLog`] resource that
//! lists them.
//!
//! It is built on Tablo's public API alone: a [`CommitHook`](tablo_core::CommitHook) stores the
//! writes, an [`AuthHook`](tablo_core::AuthHook) stores impersonation, and [`AuditLog`] is an
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
mod log;
mod trail;

use std::time::Duration;

use tablo_core::{Panel, PanelHandle, ResourceDef};
use topcoat::l10n::locale;

pub use self::{
    entry::{AuditChange, AuditEntry, AuditEvent},
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
    /// transaction, and one for each start and stop of [impersonation](tablo_core::Impersonation),
    /// without listing them: an [`Area`](tablo_core::Area) group lists them with
    /// `group.resource_with::<AuditLog>(..)`. Calling it again changes nothing.
    ///
    /// Each entry names the [actor](tablo_core::auth::actor), the tenant they act for, the
    /// resource and record, and the fields the write changed, as a
    /// [`CommitHook`](tablo_core::CommitHook) reads them: a field marked `#[columns(skip)]`, such
    /// as a password hash, is never stored. A write whose entry fails to store rolls back with
    /// it; a stop of impersonation goes through unrecorded. Mounting refuses a `Db` that does not
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

/// Deletes the [`AuditEntry`] rows of `panel` older than `age`. Nothing else deletes them; a job
/// calls it on the schedule the app keeps its audit trail for.
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
