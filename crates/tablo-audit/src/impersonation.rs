//! [`ImpersonationEvent`]: what an [`AuditEntry`] records when someone starts or stops acting as a
//! user, read back typed.

use jiff::Timestamp;
use toasty::stmt::Expr;

use crate::AuditEntry;

/// The event of an entry recording the start of an impersonation.
pub(crate) const IMPERSONATE: &str = "impersonate";
/// The event of an entry recording its stop.
pub(crate) const STOP_IMPERSONATING: &str = "stop_impersonating";
/// The change naming the account an auth event was done to.
pub(crate) const TARGET_FIELD: &str = "user";
/// The change naming when an impersonation ends at the latest.
pub(crate) const UNTIL_FIELD: &str = "until";

/// A start or a stop of acting as a user, as [`AuditEntry::impersonation`] reads it. The entry's
/// [`actor`](AuditEntry::actor) is who acted; this names whom they acted as.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImpersonationEvent {
    /// The actor started acting as the user.
    Started {
        /// The user's [`PanelUser::user_id`](tablo_core::PanelUser::user_id).
        user_id: String,
        /// The user's display name then; `None` when the account was not loaded.
        user_name: Option<String>,
        /// When the session ends at the latest: a start no stop follows ended then.
        until: Option<Timestamp>,
    },
    /// The actor stopped acting as the user.
    Stopped {
        /// The user's [`PanelUser::user_id`](tablo_core::PanelUser::user_id).
        user_id: String,
        /// The user's display name then; `None` when the account was not loaded, as when the
        /// session ran out.
        user_name: Option<String>,
    },
}

impl AuditEntry {
    /// The impersonation this entry records; `None` for any other entry.
    ///
    /// ```rust
    /// # use tablo_audit::{AuditEntry, ImpersonationEvent};
    /// fn whom(entry: &AuditEntry) -> Option<String> {
    ///     match entry.impersonation()? {
    ///         ImpersonationEvent::Started { user_name, .. }
    ///         | ImpersonationEvent::Stopped { user_name, .. } => user_name,
    ///         _ => None,
    ///     }
    /// }
    /// ```
    pub fn impersonation(&self) -> Option<ImpersonationEvent> {
        let started = match self.event.as_str() {
            IMPERSONATE => true,
            STOP_IMPERSONATING => false,
            _ => return None,
        };
        let user_id = self.record.clone()?;
        let after = |field: &str| {
            self.changes
                .0
                .iter()
                .find(|change| change.field == field)
                .and_then(|change| change.after.clone())
        };
        // The trail writes the id where it has no name.
        let user_name = after(TARGET_FIELD).filter(|name| *name != user_id);
        Some(if started {
            ImpersonationEvent::Started {
                user_id,
                user_name,
                until: after(UNTIL_FIELD).and_then(|until| until.parse().ok()),
            }
        } else {
            ImpersonationEvent::Stopped { user_id, user_name }
        })
    }
}

/// The entries that record an impersonation, started or stopped, for a resource's
/// [`query`](tablo_core::ResourceDef::query) or a filter:
///
/// ```rust
/// # use tablo_audit::AuditEntry;
/// let starts_and_stops = AuditEntry::filter(tablo_audit::impersonations());
/// # let _ = starts_and_stops;
/// ```
pub fn impersonations() -> Expr<bool> {
    AuditEntry::fields()
        .event()
        .in_list([IMPERSONATE.to_owned(), STOP_IMPERSONATING.to_owned()])
}
