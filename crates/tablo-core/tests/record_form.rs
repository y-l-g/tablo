//! Record forms, end to end: the derived parse and write, the edit path's
//! completion and naming, and the panel-build checks that keep a form's struct
//! and its `Schema` in agreement.

use std::collections::{HashMap, HashSet};

use http::StatusCode;
use tablo_core::{
    FieldErrorKind, FieldErrors, FormResource, Panel, RecordForm, Repeater, Resource, Schema,
    Select, Table, Tenant, TextColumn, TextInput, write_create,
};
use toasty::Db;
use topcoat::context::{Cx, CxTestBuilder};
use uuid::Uuid;

use crate::common::{body_string, form_router, get, memory_db, panel, post_fields};

#[derive(Debug, Clone, toasty::Model)]
struct Item {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    notes: String,
    priority: i64,
    done: bool,
}

/// Every column, with the blank answers the optional controls need.
#[derive(tablo_core::RecordForm)]
#[record_form(model = Item)]
struct ItemForm {
    title: String,
    notes: String,
    #[record_form(blank = 3)]
    priority: i64,
    #[record_form(blank = false)]
    done: bool,
}

fn item_schema() -> Schema {
    Schema::new((
        TextInput::r#for(Item::fields().title()),
        TextInput::r#for(Item::fields().notes()).optional(),
        TextInput::typed::<Item, i64>(Item::fields().priority()).optional(),
        Select::r#for(Item::fields().done())
            .options(vec!["true".to_string(), "false".to_string()])
            .optional(),
    ))
}

fn item_table(cx: &Cx) -> Table<Item> {
    Table::r#for(cx)
        .key(|item: &Item| item.id.to_string())
        .columns(TextColumn::r#for(Item::fields().title(), |item: &Item| {
            item.title.clone()
        }))
}

struct ItemResource;

impl Resource for ItemResource {
    type Model = Item;

    fn slug() -> String {
        "items".to_string()
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &Item) -> bool {
        true
    }

    fn can_create(_cx: &Cx) -> bool {
        true
    }

    fn can_update(_cx: &Cx, _record: &Item) -> bool {
        true
    }

    fn table(cx: &Cx) -> Table<Item> {
        item_table(cx)
    }

    fn view(_cx: &Cx) -> Schema {
        Schema::new(TextInput::r#for(Item::fields().title()))
    }
}

impl FormResource for ItemResource {
    type Form = ItemForm;

    fn form(_cx: &Cx) -> Schema {
        item_schema()
    }

    fn validate_record(_cx: &Cx, form: &ItemForm) -> FieldErrors<ItemForm> {
        let mut errors = FieldErrors::new();
        if form.priority > 10 {
            errors.add(ItemFormField::Priority, "Priority is at most 10");
        }
        errors
    }
}

async fn item_db() -> Db {
    memory_db(toasty::models!(Item)).await
}

async fn seed_item(db: &Db) -> Item {
    let mut db = db.clone();
    toasty::create!(Item {
        title: "Stored".to_string(),
        notes: "keep me".to_string(),
        priority: 7,
        done: true,
    })
    .exec(&mut db)
    .await
    .expect("seed item")
}

async fn reload(db: &Db, id: Uuid) -> Item {
    let mut db = db.clone();
    Item::get_by_id(&mut db, &id)
        .await
        .expect("the item exists")
}

fn cx_for(db: &Db) -> Cx {
    CxTestBuilder::new().app_context(db.clone()).build()
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The upstream fact `into_update`'s `Option` rests on: toasty refuses an
/// update with no assignment. The assert runs on the connection's worker task,
/// so the caller sees the dropped reply channel.
#[tokio::test]
#[should_panic(expected = "RecvError")]
async fn toasty_refuses_an_update_with_no_assignment() {
    let db = item_db().await;
    let mut record = seed_item(&db).await;
    let mut db = db.clone();
    let _ = record.update().exec(&mut db).await;
}

/// `into_update` borrows the record for the builder's life, `exec` consumes
/// the builder, and the record then holds the written row.
#[tokio::test]
async fn the_derived_update_reloads_the_record_it_wrote() {
    let db = item_db().await;
    let mut record = seed_item(&db).await;
    let form = ItemForm {
        title: "Written".to_string(),
        notes: String::new(),
        priority: 1,
        done: false,
    };
    let named = HashSet::from([ItemFormField::Title]);
    let update = form
        .into_update(&mut record, &named)
        .expect("a named field builds an update");
    let mut handle = db.clone();
    update.exec(&mut handle).await.expect("the update runs");
    assert_eq!(record.title, "Written", "the record holds the written row");
    let stored = reload(&db, record.id).await;
    assert_eq!(stored.title, "Written");
    assert_eq!(stored.notes, "keep me", "unnamed fields are not assigned");
    assert_eq!(stored.priority, 7);
}

#[tokio::test]
async fn into_update_is_none_when_no_field_is_named() {
    let db = item_db().await;
    let mut record = seed_item(&db).await;
    let form = ItemForm {
        title: "x".to_string(),
        notes: String::new(),
        priority: 1,
        done: false,
    };
    assert!(form.into_update(&mut record, &HashSet::new()).is_none());
}

#[tokio::test]
async fn blank_keys_take_each_fields_blank_answer() {
    let db = item_db().await;
    let cx = cx_for(&db);
    let form = ItemForm::parse(
        &cx,
        &map(&[("title", " "), ("notes", ""), ("priority", "")]),
    )
    .expect("every field answers blank");
    assert_eq!(form.title, "", "a `String` answers the empty string");
    assert_eq!(form.priority, 3, "a declared blank answers itself");
    assert!(!form.done, "an absent key reads as blank");
}

#[tokio::test]
async fn a_blank_with_no_answer_and_a_bad_value_are_refused_by_key() {
    #[derive(tablo_core::RecordForm)]
    #[record_form(model = Item)]
    struct StrictForm {
        priority: i64,
        done: bool,
    }

    let db = item_db().await;
    let cx = cx_for(&db);
    let Err(errors) = StrictForm::parse(&cx, &map(&[("priority", ""), ("done", "maybe")])) else {
        panic!("both fields fail");
    };
    assert_eq!(
        errors.len(),
        2,
        "every failing key reports once: {errors:?}"
    );
    assert_eq!(errors[0].key, "priority");
    assert_eq!(errors[0].kind, FieldErrorKind::Required);
    assert_eq!(errors[1].key, "done");
    assert_eq!(errors[1].kind, FieldErrorKind::Invalid);
    assert_eq!(errors[1].message, "`maybe` is not a valid yes/no value");
}

#[tokio::test]
async fn hydrate_is_the_parse_s_inverse() {
    let db = item_db().await;
    let cx = cx_for(&db);
    let record = seed_item(&db).await;
    let values = ItemForm::hydrate(&cx, &record);
    assert_eq!(values["title"], "Stored");
    assert_eq!(values["notes"], "keep me");
    assert_eq!(values["priority"], "7");
    assert_eq!(values["done"], "true");
    let form = ItemForm::parse(&cx, &values).expect("a hydrated record parses");
    assert_eq!(form.priority, 7);
    assert!(form.done);
}

#[tokio::test]
async fn a_create_writes_the_parsed_form() {
    let db = item_db().await;
    let router = form_router::<ItemResource>(db.clone());
    let response = post_fields(&router, "/admin/items/create", &[("title", "New")]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let mut handle = db.clone();
    let items = Item::all().exec(&mut handle).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "New");
    assert_eq!(items[0].notes, "");
    assert_eq!(
        items[0].priority, 3,
        "an absent optional key takes its blank answer"
    );
    assert!(!items[0].done);
}

/// An edit that posts one key changes one field: every other key is completed
/// from the stored record, and an omitted required key keeps its value rather
/// than failing validation.
#[tokio::test]
async fn an_edit_writes_only_the_fields_it_names() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = form_router::<ItemResource>(db.clone());
    let response = post_fields(
        &router,
        &format!("/admin/items/{}/edit", item.id),
        &[("priority", "9")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let stored = reload(&db, item.id).await;
    assert_eq!(stored.priority, 9);
    assert_eq!(
        stored.title, "Stored",
        "an omitted required key keeps its value"
    );
    assert_eq!(stored.notes, "keep me");
    assert!(stored.done);
}

#[tokio::test]
async fn an_emptied_control_stores_its_blank_answer() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = form_router::<ItemResource>(db.clone());
    let response = post_fields(
        &router,
        &format!("/admin/items/{}/edit", item.id),
        &[("notes", ""), ("priority", ""), ("done", "")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let stored = reload(&db, item.id).await;
    assert_eq!(stored.notes, "", "an emptied `String` clears");
    assert_eq!(stored.priority, 3);
    assert!(!stored.done);
    assert_eq!(stored.title, "Stored");
}

/// A submission naming no form field runs no statement: toasty would refuse
/// the empty update, and nothing changed.
#[tokio::test]
async fn an_edit_naming_no_field_writes_nothing_and_redirects() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = form_router::<ItemResource>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let stored = reload(&db, item.id).await;
    assert_eq!(stored.title, "Stored");
    assert_eq!(stored.priority, 7);
}

/// A schema error and a `validate_record` error in one submission render
/// together, and nothing is written.
#[tokio::test]
async fn schema_and_record_errors_render_in_one_round() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = form_router::<ItemResource>(db.clone());
    let response = post_fields(
        &router,
        &format!("/admin/items/{}/edit", item.id),
        &[("title", ""), ("priority", "11")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "the form re-renders");
    let html = body_string(response).await;
    assert!(html.contains("Title is required"), "{html}");
    assert!(html.contains("Priority is at most 10"), "{html}");
    let stored = reload(&db, item.id).await;
    assert_eq!(stored.priority, 7, "a refused submission writes nothing");
}

/// The detail page of a form resource reads the form's projection, so the page
/// and the form agree about what a field holds without a `view_values`.
#[tokio::test]
async fn the_detail_page_reads_the_forms_projection() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = form_router::<ItemResource>(db.clone());
    let response = get(&router, &format!("/admin/items/{}", item.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Stored"), "{html}");
}

#[derive(Debug, Clone, toasty::Model)]
struct Owned {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    tenant_id: Uuid,
    title: String,
}

#[derive(tablo_core::RecordForm)]
#[record_form(model = Owned)]
struct OwnedForm {
    title: String,
}

struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;

    fn slug() -> String {
        "owned".to_string()
    }

    fn requires_tenant() -> bool {
        true
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_create(_cx: &Cx) -> bool {
        true
    }

    fn table(cx: &Cx) -> Table<Owned> {
        Table::r#for(cx)
            .key(|row: &Owned| row.id.to_string())
            .columns(TextColumn::r#for(Owned::fields().title(), |row: &Owned| {
                row.title.clone()
            }))
    }
}

impl FormResource for OwnedResource {
    type Form = OwnedForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new(TextInput::r#for(Owned::fields().title()))
    }
}

#[tokio::test]
async fn the_derived_create_stamps_the_request_tenant() {
    let db = memory_db(toasty::models!(Owned)).await;
    let tenant = Uuid::new_v4();
    let cx = CxTestBuilder::new()
        .app_context(db.clone())
        .request_context(Tenant(tenant))
        .build();
    let mut handle = db.clone();
    let created = write_create::<OwnedResource>(
        &cx,
        OwnedForm {
            title: "Mine".to_string(),
        },
        &mut handle,
    )
    .await
    .expect("the create runs");
    assert_eq!(created.tenant_id, tenant, "the request tenant is stamped");
}

/// The build error for `R` registered with `Panel::form_resource`.
fn form_build_error<R: FormResource>(db: Db) -> String {
    match panel(db).form_resource::<R>().build() {
        Ok(_) => panic!("{} must not build", std::any::type_name::<R>()),
        Err(error) => error.to_string(),
    }
}

/// A resource over [`Item`] whose form is `F` and whose schema is `schema`.
macro_rules! item_resource {
    ($name:ident, $form:ty, $schema:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;

            fn slug() -> String {
                "items".to_string()
            }

            fn can_view_any(_cx: &Cx) -> bool {
                true
            }

            fn table(cx: &Cx) -> Table<Item> {
                item_table(cx)
            }
        }

        impl FormResource for $name {
            type Form = $form;

            fn form(_cx: &Cx) -> Schema {
                $schema
            }
        }
    };
}

#[derive(tablo_core::RecordForm)]
#[record_form(model = Item)]
struct TitleForm {
    title: String,
}

#[derive(tablo_core::RecordForm)]
#[record_form(model = Item)]
struct PriorityForm {
    priority: i64,
}

#[tokio::test]
async fn build_refuses_a_control_no_field_binds() {
    item_resource!(
        Unbound,
        TitleForm,
        Schema::new((
            TextInput::r#for(Item::fields().title()),
            TextInput::r#for(Item::fields().notes()),
        ))
    );
    let error = form_build_error::<Unbound>(item_db().await);
    assert!(
        error.contains("form control `notes`") && error.contains("never written"),
        "{error}"
    );
}

#[tokio::test]
async fn build_refuses_a_field_no_control_declares() {
    item_resource!(
        Unclaimed,
        ItemForm,
        Schema::new(TextInput::r#for(Item::fields().title()))
    );
    let error = form_build_error::<Unclaimed>(item_db().await);
    assert!(
        error.contains("binds key `notes`") && error.contains("no control"),
        "{error}"
    );
}

#[tokio::test]
async fn build_refuses_an_optional_control_with_no_blank_answer() {
    item_resource!(
        Unanswered,
        PriorityForm,
        Schema::new(TextInput::typed::<Item, i64>(Item::fields().priority()).optional())
    );
    let error = form_build_error::<Unanswered>(item_db().await);
    assert!(
        error.contains("`priority` is optional") && error.contains("no blank answer"),
        "{error}"
    );
}

#[tokio::test]
async fn build_refuses_a_repeater_control_with_no_blank_answer() {
    item_resource!(
        Repeated,
        PriorityForm,
        Schema::new(
            Repeater::new("Priorities")
                .schema(TextInput::typed::<Item, i64>(Item::fields().priority()))
        )
    );
    let error = form_build_error::<Repeated>(item_db().await);
    assert!(error.contains("inside a `Repeater`"), "{error}");
}

#[tokio::test]
async fn build_refuses_a_gated_form_claiming_the_tenant_column() {
    #[derive(tablo_core::RecordForm)]
    #[record_form(model = Owned)]
    struct ClaimingForm {
        tenant_id: Uuid,
        title: String,
    }

    struct Claiming;

    impl Resource for Claiming {
        type Model = Owned;

        fn requires_tenant() -> bool {
            true
        }

        fn table(cx: &Cx) -> Table<Owned> {
            OwnedResource::table(cx)
        }
    }

    impl FormResource for Claiming {
        type Form = ClaimingForm;

        fn form(_cx: &Cx) -> Schema {
            Schema::new((
                TextInput::typed::<Owned, Uuid>(Owned::fields().tenant_id()),
                TextInput::r#for(Owned::fields().title()),
            ))
        }
    }

    let error = form_build_error::<Claiming>(memory_db(toasty::models!(Owned)).await);
    assert!(
        error.contains("claims its tenant column `tenant_id`"),
        "{error}"
    );
}

#[tokio::test]
async fn a_list_only_registration_refuses_create_or_edit() {
    let db = item_db().await;
    let Err(error) = panel(db).resource::<ItemResource>().build() else {
        panic!("a resource that allows create needs a form registration");
    };
    let error = error.to_string();
    assert!(
        error.contains("declares create") && error.contains("Panel::form_resource"),
        "{error}"
    );
}

#[tokio::test]
async fn a_form_resource_serves_create_and_edit() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router: topcoat::router::Router = Panel::new("admin")
        .app_context(db)
        .auth(tablo_core::Auth::disabled())
        .form_resource::<ItemResource>()
        .build()
        .expect("panel builds");
    let create = get(&router, "/admin/items/create").await;
    assert_eq!(create.status(), StatusCode::OK);
    // Every rendered control posts its key, which is what lets an unposted key
    // mean "keep".
    let html = body_string(create).await;
    for name in ["title", "notes", "priority", "done"] {
        assert!(
            html.contains(&format!("name=\"{name}\"")),
            "`{name}` posts: {html}"
        );
    }
    assert_eq!(
        get(&router, &format!("/admin/items/{}/edit", item.id))
            .await
            .status(),
        StatusCode::OK
    );
}
