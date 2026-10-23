//! Layout blocks the showcase does not nest: a `Tabs` block in a tab of another.

use tablo::{
    Allow, Resource, ResourceDef, Schema, Tab, Table, Tabs, TextColumn, lens, testing::tabs,
};
use uuid::Uuid;

use crate::common::{TestClient, body_string, panel_router, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Contact {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    bio: String,
    street: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Contact)]
struct ContactForm {
    name: String,
    bio: String,
    street: String,
}

struct ContactResource;

impl Resource for ContactResource {
    type Model = Contact;
    type Form = ContactForm;

    fn declare() -> ResourceDef<Self> {
        let c = ContactForm::controls();
        ResourceDef::new()
            .slug("contacts")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Contact.name))))
            // The outer "Person" tab and the inner "Name" tab start at the same field.
            .form(Schema::new(Tabs::new([
                Tab::new("Person").schema(Tabs::new([
                    Tab::new("Name").schema(c.name),
                    Tab::new("Bio").schema(c.bio),
                ])),
                Tab::new("Address").schema(c.street),
            ])))
    }
}

/// Each tab of a nested block controls its own panel, apart from the outer tab holding it.
#[tokio::test]
async fn a_nested_block_controls_its_own_panels() {
    let db = test_db(toasty::models!(Contact)).await;
    let router = panel_router::<ContactResource>(db);
    let client = TestClient::new(&router);
    let html = body_string(client.get("/admin/contacts/-/create").await).await;
    let read: Vec<(Vec<String>, bool)> = tabs(&html)
        .into_iter()
        .map(|tab| (tab.keys, tab.active))
        .collect();
    let keys = |keys: &[&str]| keys.iter().map(|key| key.to_string()).collect::<Vec<_>>();
    assert_eq!(
        read,
        [
            (keys(&["name", "bio"]), true),
            (keys(&["street"]), false),
            (keys(&["name"]), true),
            (keys(&["bio"]), false),
        ]
    );
}
