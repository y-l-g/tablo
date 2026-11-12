//! An action's button on one record: its text follows the record's state in the record's row and
//! on its record page, while the bulk bar keeps the action's `label`.
//!
//! One resource declares a toggle whose `label_for` reads the account's `active` flag.

use tablo::{Action, ActionDef, Allow, NoForm, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{
    action_label, action_tooltip, body_string, disabled_actions, get, mount, panel, test_db,
};

#[derive(Debug, toasty::Model, tablo::PanelModel, Clone)]
struct Account {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    active: bool,
}

/// Deactivates an active account and activates an inactive one.
struct Toggle;

impl Action<AccountResource> for Toggle {
    type Input = ();
    const NAME: &'static str = "toggle";

    fn declare() -> ActionDef<AccountResource> {
        ActionDef::new()
            .label("Toggle")
            .label_for(|_cx, account: &Account| {
                if account.active {
                    "Deactivate".into()
                } else {
                    "Activate".into()
                }
            })
    }

    async fn run(
        _cx: &Cx,
        _: &[Account],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<std::convert::Infallible>> {
        Ok(tablo::Outcome::done())
    }
}

/// Sends an account its welcome mail, which only an active account may receive. Its row's button
/// shows the hint below, or the reason it is disabled on an inactive account.
struct Notify;

impl Action<AccountResource> for Notify {
    type Input = ();
    const NAME: &'static str = "notify";

    fn declare() -> ActionDef<AccountResource> {
        ActionDef::new()
            .label("Notify")
            .tooltip("Sends the welcome mail")
            .can_run(|_cx, account: &Account| account.active)
            .disabled_reason(|_cx, _account| Some("Reactivate the account first".into()))
    }

    async fn run(
        _cx: &Cx,
        _: &[Account],
        _: (),
        _: &mut dyn toasty::Executor,
    ) -> topcoat::Result<tablo::Outcome, tablo::WriteError<std::convert::Infallible>> {
        Ok(tablo::Outcome::done())
    }
}

struct AccountResource;

impl Resource for AccountResource {
    type Model = Account;
    type Form = NoForm<Account>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("accounts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Account.name))))
            .action::<Toggle>()
            .action::<Notify>()
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Account)).await
}

async fn seed(db: &Db, name: &str, active: bool) -> Account {
    let mut db = db.clone();
    toasty::create!(Account {
        name: name.to_string(),
        active,
    })
    .exec(&mut db)
    .await
    .expect("seed account")
}

fn router(db: &Db) -> Router {
    mount(db.clone(), panel().resource::<AccountResource>()).expect("panel builds")
}

fn toggle_url(account: &Account) -> String {
    format!("/admin/accounts/{}/-/actions/toggle", account.id)
}

fn notify_url(account: &Account) -> String {
    format!("/admin/accounts/{}/-/actions/notify", account.id)
}

#[tokio::test]
async fn a_row_names_the_action_for_its_record() {
    let db = db().await;
    let ada = seed(&db, "Ada", true).await;
    let bea = seed(&db, "Bea", false).await;
    let router = router(&db);

    let html = body_string(get(&router, "/admin/accounts").await).await;
    assert_eq!(
        action_label(&html, &toggle_url(&ada)).as_deref(),
        Some("Deactivate"),
        "an active account's row offers to deactivate it: {html}"
    );
    assert_eq!(
        action_label(&html, &toggle_url(&bea)).as_deref(),
        Some("Activate"),
        "an inactive account's row offers to activate it: {html}"
    );
    assert_eq!(
        action_label(&html, "/admin/accounts/-/actions/toggle").as_deref(),
        Some("Toggle"),
        "the bulk bar keeps the action's label: {html}"
    );
}

#[tokio::test]
async fn a_record_page_names_the_action_for_its_record() {
    let db = db().await;
    let ada = seed(&db, "Ada", true).await;
    let bea = seed(&db, "Bea", false).await;
    let router = router(&db);

    let html = body_string(get(&router, &format!("/admin/accounts/{}", ada.id)).await).await;
    assert_eq!(
        action_label(&html, &toggle_url(&ada)).as_deref(),
        Some("Deactivate"),
        "Ada's page offers to deactivate her: {html}"
    );

    let html = body_string(get(&router, &format!("/admin/accounts/{}", bea.id)).await).await;
    assert_eq!(
        action_label(&html, &toggle_url(&bea)).as_deref(),
        Some("Activate"),
        "Bea's page offers to activate her: {html}"
    );
}

#[tokio::test]
async fn a_row_hovers_its_action_hint_and_a_disabled_button_its_reason() {
    let db = db().await;
    let ada = seed(&db, "Ada", true).await;
    seed(&db, "Bea", false).await;
    let router = router(&db);

    let html = body_string(get(&router, "/admin/accounts").await).await;
    assert_eq!(
        action_tooltip(&html, &notify_url(&ada)).as_deref(),
        Some("Sends the welcome mail"),
        "an active account's Notify hovers its hint: {html}"
    );
    assert_eq!(
        action_tooltip(&html, &toggle_url(&ada)),
        None,
        "an action with no hint shows no hover text: {html}"
    );
    assert!(
        disabled_actions(&html).iter().any(|disabled| {
            disabled.name == "notify" && disabled.reason == "Reactivate the account first"
        }),
        "an inactive account's Notify shows disabled with its reason, not its hint: {html}"
    );
}
