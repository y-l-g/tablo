//! An action's button on one record: its text follows the record's state in the record's row and
//! on its record page, while the bulk bar keeps the action's `label`.
//!
//! One resource declares a toggle whose `label_for` reads the account's `active` flag.

use tablo::{Action, Allow, NoForm, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{action_label, body_string, get, mount, panel, test_db};

#[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
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

    fn label() -> tablo::Text {
        "Toggle".into()
    }

    fn label_for(_cx: &Cx, account: &Account) -> tablo::Text {
        if account.active {
            "Deactivate".into()
        } else {
            "Activate".into()
        }
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
