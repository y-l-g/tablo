//! Auth mail: the mistakes mounting refuses in what a panel's auth mails.

use tablo::{
    Auth, AuthMail, AuthMailFault, DeclarationErrorKind, NoForm, Panel, PasswordAuth,
    PasswordReset, ReadOnly, Resource, ResourceDef, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, AuthToken},
    lens,
};
use toasty::Db;
use topcoat::{
    mail::{MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct MemoResource;

impl Resource for MemoResource {
    type Model = Memo;
    type Form = NoForm<Memo>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Memo.body))))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Memo, AdminUser, AuthSession, AuthToken)).await
}

fn sender() -> Mailbox {
    Mailbox::new("no-reply@example.com").unwrap()
}

/// Mounts `auth` on a router that can send mail when `with_mail`.
async fn mount(auth: Auth, with_mail: bool) -> topcoat::Result<Router> {
    let mut builder = Router::builder().discover().app_context(db().await);
    if with_mail {
        builder = builder.mail(
            MailConfig::builder()
                .transport(MemoryTransport::new())
                .build(),
        );
    }
    let panel = Panel::new("admin").auth(auth).resource::<MemoResource>();
    Ok(builder.panel(panel)?.build())
}

/// The faults mounting `auth` reports.
async fn faults(auth: Auth, with_mail: bool) -> Vec<DeclarationErrorKind> {
    refusal(mount(auth, with_mail).await)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// A feature that mails links needs a sender, a base URL the link can start with, and a router
/// that sends mail.
#[tokio::test]
async fn mounting_refuses_mail_the_panel_cannot_send() {
    let reset = || PasswordReset::new(PasswordAuth);

    assert_eq!(
        faults(Auth::password().password_reset(reset()), true).await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::Unset {
            feature: "password_reset"
        })]
    );

    let relative = AuthMail::new(sender(), "admin.example.com");
    assert_eq!(
        faults(
            Auth::password().mail(relative).password_reset(reset()),
            true
        )
        .await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::BaseUrl(
            "admin.example.com".to_string()
        ))]
    );

    let mail = AuthMail::new(sender(), "https://admin.example.com");
    assert_eq!(
        faults(Auth::password().mail(mail).password_reset(reset()), false).await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::MissingMail)]
    );
}

/// A sender with nothing to mail needs no mail configuration.
#[tokio::test]
async fn a_sender_without_a_feature_that_mails_mounts_anywhere() {
    let mail = AuthMail::new(sender(), "not a url");
    assert!(mount(Auth::password().mail(mail), false).await.is_ok());
}
