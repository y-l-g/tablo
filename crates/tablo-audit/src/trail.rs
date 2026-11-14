//! [`Trail`]: the hooks that store an [`AuditEntry`](crate::AuditEntry) for each write and each
//! [`AuthEvent`]: a sign-in, a sign-out, a change of credentials, an impersonation.

use tablo_core::{
    AuthEvent, AuthEventKind, AuthHook, Change, ChangedRecord, CommitHook, PanelChange,
};
use toasty::Executor;
use topcoat::{Result, context::Cx};

use crate::AuditEvent;

/// The trail [`audit_trail`](crate::PanelAuditExt::audit_trail) registers.
pub(crate) struct Trail;

impl CommitHook for Trail {
    async fn before_commit(cx: &Cx, change: &PanelChange, ex: &mut dyn Executor) -> Result<()> {
        // Every panel the hook is registered on serves the request it runs in.
        let Some(panel) = tablo_core::url::panel(cx) else {
            return Ok(());
        };
        let actor = tablo_core::auth::actor(cx)
            .map(|actor| (actor.user_id(), actor.display_name().to_owned()));
        let tenant = tablo_core::tenant_id(cx);
        for event in events(cx, change) {
            event
                .write(&mut *ex, panel.clone(), tenant, actor.clone())
                .await?;
        }
        Ok(())
    }
}

impl AuthHook for Trail {
    async fn on_event(cx: &Cx, event: &AuthEvent<'_>, ex: &mut dyn Executor) -> Result<()> {
        let name = match event.kind() {
            AuthEventKind::Impersonate { .. } => "impersonate",
            AuthEventKind::StopImpersonating => "stop_impersonating",
            AuthEventKind::SignIn => "sign_in",
            AuthEventKind::SignInFailed { .. } => "sign_in_failed",
            AuthEventKind::SecondFactorFailed => "second_factor_failed",
            AuthEventKind::SignOut => "sign_out",
            AuthEventKind::CredentialsChanged => "credentials_changed",
            AuthEventKind::PasswordReset => "password_reset",
            AuthEventKind::EmailChanged => "email_changed",
            AuthEventKind::TwoFactorEnabled => "two_factor_enabled",
            AuthEventKind::TwoFactorDisabled => "two_factor_disabled",
            AuthEventKind::RecoveryCodesReplaced => "recovery_codes_replaced",
            // An event the trail does not know records nothing rather than a guess.
            _ => return Ok(()),
        };
        let mut entry = AuditEvent::new(name);
        if let Some(target) = event.target_id() {
            entry = entry.key(target, None).change(
                "user",
                tablo_core::t!(cx, "tablo-audit-target"),
                None,
                Some(event.target_name().unwrap_or(target).to_owned()),
            );
        }
        match event.kind() {
            // A session that runs out unseen records no stop, so a start names when it ends at
            // the latest.
            AuthEventKind::Impersonate { until } => {
                entry = entry.change(
                    "until",
                    tablo_core::t!(cx, "tablo-audit-until"),
                    None,
                    Some(until.to_string()),
                );
            }
            AuthEventKind::SignInFailed { login } => {
                entry = entry.change(
                    "login",
                    tablo_core::t!(cx, "tablo-audit-login"),
                    None,
                    Some(login.to_owned()),
                );
            }
            _ => {}
        }
        let actor = event
            .actor()
            .map(|actor| (actor.user_id(), actor.display_name().to_owned()));
        entry
            .write(ex, event.panel().to_string(), event.tenant(), actor)
            .await?;
        Ok(())
    }
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
