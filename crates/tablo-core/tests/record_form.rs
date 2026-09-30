//! Record forms, end to end: the derived parse and write, the edit path's
//! completion and naming, and the panel-build checks that keep a form's struct
//! and its `Schema` in agreement.

use std::collections::{HashMap, HashSet};

use http::StatusCode;
use tablo_core::{
    Field, FieldErrorKind, FieldErrors, NoForm, Panel, RecordForm, Repeater, Resource, Schema,
    Table, Tenant, TextColumn, write_create,
};
use toasty::Db;
use topcoat::context::{Cx, CxTestBuilder};
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, panel, panel_router, post_fields};

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
#[form(model = Item)]
struct ItemForm {
    title: String,
    notes: String,
    #[form(blank = 3)]
    priority: i64,
    #[form(blank = false)]
    done: bool,
}

fn item_schema() -> Schema {
    Schema::new((
        Field::text(Item::fields().title()),
        Field::text(Item::fields().notes()).optional(),
        Field::text(Item::fields().priority()).optional(),
        Field::choice(Item::fields().done())
            .options(vec!["true".to_string(), "false".to_string()])
            .optional(),
    ))
}

fn item_table(_cx: &Cx) -> Table<Item> {
    Table::new(
        |item: &Item| item.id.to_string(),
        TextColumn::r#for(Item::fields().title(), |item: &Item| item.title.clone()),
    )
}

struct ItemResource;

impl Resource for ItemResource {
    type Model = Item;
    type Form = ItemForm;

    fn form(_cx: &Cx) -> Schema {
        item_schema()
    }

    fn validate_record(_cx: &Cx, form: &ItemForm) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if form.priority > 10 {
            errors.add("priority", "Priority is at most 10");
        }
        errors
    }

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

    fn table(_cx: &Cx) -> Table<Item> {
        item_table(_cx)
    }

    fn view(_cx: &Cx) -> Schema {
        Schema::new(Field::text(Item::fields().title()))
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
    #[form(model = Item)]
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
    let router = panel_router::<ItemResource>(db.clone());
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
    let router = panel_router::<ItemResource>(db.clone());
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
    let router = panel_router::<ItemResource>(db.clone());
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
    let router = panel_router::<ItemResource>(db.clone());
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
    let router = panel_router::<ItemResource>(db.clone());
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

/// A `validate_record` error keyed to a repeater group's label renders in the
/// group's own slot, with a 200: the label is a key like any other.
#[tokio::test]
async fn a_repeater_label_keyed_rule_renders_in_the_group() {
    struct Tagged;

    impl Resource for Tagged {
        type Model = Item;
        type Form = ItemForm;

        /// Every control `ItemForm` binds, with the tagged ones inside the
        /// group the rule answers for.
        fn form(_cx: &Cx) -> Schema {
            Schema::new((
                Field::text(Item::fields().title()),
                Repeater::new("Tags").schema((
                    Field::text(Item::fields().notes()).optional(),
                    Field::text(Item::fields().priority()).optional(),
                    Field::choice(Item::fields().done())
                        .options(vec!["true".to_string(), "false".to_string()])
                        .optional(),
                )),
            ))
        }

        fn validate_record(_cx: &Cx, _form: &ItemForm) -> FieldErrors {
            let mut errors = FieldErrors::new();
            errors.add("Tags", "At least one tag");
            errors
        }

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view_any(_cx: &Cx) -> bool {
            true
        }

        fn can_view(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn can_update(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Tagged>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::OK, "the form re-renders");
    let html = body_string(response).await;
    assert!(
        html.contains("At least one tag"),
        "the group's label-keyed error must render in its slot, got {html}"
    );
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// The detail page of a form resource reads the form's projection, so the page
/// and the form agree about what a field holds without a `view_values`.
#[tokio::test]
async fn the_detail_page_reads_the_forms_projection() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<ItemResource>(db.clone());
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
#[form(model = Owned)]
struct OwnedForm {
    title: String,
}

struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;
    type Form = OwnedForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new(Field::text(Owned::fields().title()))
    }

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

    fn table(_cx: &Cx) -> Table<Owned> {
        Table::new(
            |row: &Owned| row.id.to_string(),
            TextColumn::r#for(Owned::fields().title(), |row: &Owned| row.title.clone()),
        )
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

/// The build error for a panel over `R`.
fn form_build_error<R: Resource>(db: Db) -> String {
    match panel(db).resource::<R>().build() {
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
            type Form = $form;

            fn slug() -> String {
                "items".to_string()
            }

            fn can_view_any(_cx: &Cx) -> bool {
                true
            }

            fn table(_cx: &Cx) -> Table<Item> {
                item_table(_cx)
            }

            fn form(_cx: &Cx) -> Schema {
                $schema
            }
        }
    };
}

#[derive(tablo_core::RecordForm)]
#[form(model = Item)]
struct TitleForm {
    title: String,
}

#[derive(tablo_core::RecordForm)]
#[form(model = Item)]
struct PriorityForm {
    priority: i64,
}

#[tokio::test]
async fn build_refuses_a_control_no_field_binds() {
    item_resource!(
        Unbound,
        TitleForm,
        Schema::new((
            Field::text(Item::fields().title()),
            Field::text(Item::fields().notes()),
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
        Schema::new(Field::text(Item::fields().title()))
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
        Schema::new(Field::text(Item::fields().priority()).optional())
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
        Schema::new(Repeater::new("Priorities").schema(Field::text(Item::fields().priority())))
    );
    let error = form_build_error::<Repeated>(item_db().await);
    assert!(error.contains("inside a `Repeater`"), "{error}");
}

#[tokio::test]
async fn build_refuses_a_gated_form_claiming_the_tenant_column() {
    #[derive(tablo_core::RecordForm)]
    #[form(model = Owned)]
    struct ClaimingForm {
        tenant_id: Uuid,
        title: String,
    }

    struct Claiming;

    impl Resource for Claiming {
        type Model = Owned;
        type Form = ClaimingForm;

        fn form(_cx: &Cx) -> Schema {
            Schema::new((
                Field::text(Owned::fields().tenant_id()),
                Field::text(Owned::fields().title()),
            ))
        }

        fn requires_tenant() -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Owned> {
            OwnedResource::table(_cx)
        }
    }

    let error = form_build_error::<Claiming>(memory_db(toasty::models!(Owned)).await);
    assert!(
        error.contains("claims its tenant column `tenant_id`"),
        "{error}"
    );
}

/// A `NoForm` resource over [`Item`] whose `can_create` and `form()` answer
/// the given values.
macro_rules! list_only_resource {
    ($name:ident, $create:expr, $schema:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;
            type Form = NoForm<Self::Model>;

            fn slug() -> String {
                "items".to_string()
            }

            fn can_create(_cx: &Cx) -> bool {
                $create
            }

            fn table(_cx: &Cx) -> Table<Item> {
                item_table(_cx)
            }

            fn form(_cx: &Cx) -> Schema {
                $schema
            }
        }
    };
}

#[tokio::test]
async fn build_refuses_a_list_only_resource_that_allows_create() {
    list_only_resource!(Creating, true, Schema::empty());
    let error = form_build_error::<Creating>(item_db().await);
    assert!(error.contains("allows create but has no form"), "{error}");
}

#[tokio::test]
async fn build_refuses_a_list_only_resource_that_declares_a_schema() {
    list_only_resource!(Schematic, false, item_schema());
    let error = form_build_error::<Schematic>(item_db().await);
    assert!(
        error.contains("declares a form schema") && error.contains("serves no form"),
        "{error}"
    );
}

#[tokio::test]
async fn build_names_a_missing_form_override() {
    item_resource!(Unoverridden, TitleForm, Schema::empty());
    let error = form_build_error::<Unoverridden>(item_db().await);
    assert!(error.contains("does not override `form()`"), "{error}");
}

#[tokio::test]
async fn a_list_only_resource_serves_no_form_route() {
    list_only_resource!(Listed, false, Schema::empty());
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Listed>(db.clone());
    let edit = format!("/admin/items/{}/edit", item.id);
    // `create` falls to the GET-only detail route, so its POST is a 405.
    for (response, status) in [
        (
            get(&router, "/admin/items/create").await,
            StatusCode::NOT_FOUND,
        ),
        (
            get(&router, "/admin/items/options?field=title").await,
            StatusCode::NOT_FOUND,
        ),
        (get(&router, &edit).await, StatusCode::NOT_FOUND),
        (
            post_fields(&router, "/admin/items/create", &[("title", "New")]).await,
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            post_fields(&router, &edit, &[("title", "New")]).await,
            StatusCode::NOT_FOUND,
        ),
    ] {
        assert_eq!(response.status(), status);
    }
    assert_eq!(reload(&db, item.id).await.title, "Stored");
}

/// A `NoForm` resource renders its detail page from `view_values` alone.
#[tokio::test]
async fn a_list_only_detail_page_reads_view_values() {
    struct Viewed;

    impl Resource for Viewed {
        type Model = Item;
        type Form = NoForm<Self::Model>;

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }

        fn view(_cx: &Cx) -> Schema {
            Schema::new(Field::text(Item::fields().title()))
        }

        fn view_values(_cx: &Cx, record: &Item) -> HashMap<String, String> {
            HashMap::from([("title".to_string(), format!("{} (view)", record.title))])
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Viewed>(db);
    let response = get(&router, &format!("/admin/items/{}", item.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Stored (view)"), "{html}");
}

#[tokio::test]
async fn a_form_resource_serves_create_and_edit() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router: topcoat::router::Router = Panel::new("admin")
        .app_context(db)
        .auth(tablo_core::Auth::disabled())
        .resource::<ItemResource>()
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

/// A resource over [`Item`] that allows create through `F`, whose form writes
/// only `title`.
macro_rules! title_only_resource {
    ($name:ident, $columns:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;
            type Form = TitleForm;

            const CREATE_COLUMNS: &'static [&'static str] = $columns;

            fn slug() -> String {
                "items".to_string()
            }

            fn can_create(_cx: &Cx) -> bool {
                true
            }

            fn table(_cx: &Cx) -> Table<Item> {
                item_table(_cx)
            }

            fn form(_cx: &Cx) -> Schema {
                Schema::new(Field::text(Item::fields().title()))
            }
        }
    };
}

#[tokio::test]
async fn build_refuses_a_create_that_leaves_a_required_column_unset() {
    title_only_resource!(Partial, &[]);
    let error = form_build_error::<Partial>(item_db().await);
    assert!(
        error.contains("non-nullable column `notes`") && error.contains("CREATE_COLUMNS"),
        "{error}"
    );
}

#[tokio::test]
async fn create_columns_names_what_an_override_sets() {
    title_only_resource!(Covered, &["notes", "priority", "done"]);
    panel(item_db().await)
        .resource::<Covered>()
        .build()
        .expect("the override's own columns are declared");

    title_only_resource!(Misnamed, &["notes", "priority", "done", "nope"]);
    let error = form_build_error::<Misnamed>(item_db().await);
    assert!(error.contains("`nope` in `CREATE_COLUMNS`"), "{error}");
}

/// A value the control lets through but the field's type refuses renders
/// inline through the real handler, and nothing is written.
#[tokio::test]
async fn a_value_the_form_type_refuses_renders_inline() {
    struct Loose;

    impl Resource for Loose {
        type Model = Item;
        type Form = PriorityForm;

        fn form(_cx: &Cx) -> Schema {
            // A static-options select checks membership, not the column's type.
            Schema::new(
                Field::choice(Item::fields().priority())
                    .options(vec!["1".to_string(), "lots".to_string()]),
            )
        }

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn can_update(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Loose>(db.clone());
    let response = post_fields(
        &router,
        &format!("/admin/items/{}/edit", item.id),
        &[("priority", "lots")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "the form re-renders");
    let html = body_string(response).await;
    assert!(
        html.contains("`lots` is not a valid whole number"),
        "{html}"
    );
    assert_eq!(reload(&db, item.id).await.priority, 7);
}

/// A `validate_record` error on a field the form binds to no key cannot
/// render, so the submit fails closed instead of writing past it.
#[tokio::test]
async fn an_unkeyable_record_rule_fails_closed() {
    /// [`PriorityForm`] with no keys: nothing can render its errors.
    struct Keyless(PriorityForm);

    impl RecordForm for Keyless {
        type Model = Item;
        type Field = PriorityFormField;

        fn fields(_cx: &Cx) -> Vec<tablo_core::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn hydrate(cx: &Cx, record: &Item) -> HashMap<String, String> {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(
            _cx: &Cx,
            _values: &HashMap<String, String>,
        ) -> Result<Self, Vec<tablo_core::FieldError>> {
            // The form binds no key, so the submission holds nothing to read.
            Ok(Keyless(PriorityForm { priority: 1 }))
        }

        fn into_create(self) -> <Item as toasty::schema::Model>::Create {
            self.0.into_create()
        }

        fn into_update<'a>(
            self,
            record: &'a mut Item,
            named: &HashSet<PriorityFormField>,
        ) -> Option<<Item as toasty::schema::Model>::Update<'a>> {
            self.0.into_update(record, named)
        }

        fn exec_update<'a>(
            update: <Item as toasty::schema::Model>::Update<'a>,
            ex: &'a mut dyn toasty::Executor,
        ) -> impl std::future::Future<Output = toasty::Result<()>> + Send + 'a {
            PriorityForm::exec_update(update, ex)
        }
    }

    struct Refusing;

    impl Resource for Refusing {
        type Model = Item;
        type Form = Keyless;

        fn form(_cx: &Cx) -> Schema {
            Schema::empty()
        }

        fn validate_record(_cx: &Cx, _form: &Keyless) -> FieldErrors {
            let mut errors = FieldErrors::new();
            errors.add("priority", "never");
            errors
        }

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn can_update(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Refusing>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// A form whose own parse refuses a key the schema renders nowhere fails
/// closed too: the message could never reach the page.
#[tokio::test]
async fn an_unkeyable_parse_failure_fails_closed() {
    /// [`PriorityForm`] with no keys, refusing on a key nothing renders.
    struct Unkeyable(PriorityForm);

    impl RecordForm for Unkeyable {
        type Model = Item;
        type Field = PriorityFormField;

        fn fields(_cx: &Cx) -> Vec<tablo_core::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn hydrate(cx: &Cx, record: &Item) -> HashMap<String, String> {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(
            _cx: &Cx,
            _values: &HashMap<String, String>,
        ) -> Result<Self, Vec<tablo_core::FieldError>> {
            Err(vec![tablo_core::FieldError::invalid(
                "never",
                "never renders",
            )])
        }

        fn into_create(self) -> <Item as toasty::schema::Model>::Create {
            self.0.into_create()
        }

        fn into_update<'a>(
            self,
            record: &'a mut Item,
            named: &HashSet<PriorityFormField>,
        ) -> Option<<Item as toasty::schema::Model>::Update<'a>> {
            self.0.into_update(record, named)
        }

        fn exec_update<'a>(
            update: <Item as toasty::schema::Model>::Update<'a>,
            ex: &'a mut dyn toasty::Executor,
        ) -> impl std::future::Future<Output = toasty::Result<()>> + Send + 'a {
            PriorityForm::exec_update(update, ex)
        }
    }

    struct Parseless;

    impl Resource for Parseless {
        type Model = Item;
        type Form = Unkeyable;

        fn form(_cx: &Cx) -> Schema {
            Schema::empty()
        }

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn can_update(_cx: &Cx, _record: &Item) -> bool {
            true
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Parseless>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// A list-only resource serves no create route, so its list links to none —
/// even when a request-scoped `can_create` allows create and the build check,
/// which runs with no request, could not see it.
#[tokio::test]
async fn a_list_only_resource_never_links_to_create() {
    struct TenantCreates;

    impl Resource for TenantCreates {
        type Model = Item;
        type Form = tablo_core::NoForm<Self::Model>;

        fn slug() -> String {
            "items".to_string()
        }

        fn can_view_any(_cx: &Cx) -> bool {
            true
        }

        fn can_create(cx: &Cx) -> bool {
            tablo_core::tenant_id(cx).is_some()
        }

        fn table(_cx: &Cx) -> Table<Item> {
            item_table(_cx)
        }
    }

    let router = panel(item_db().await)
        .resource::<TenantCreates>()
        .build()
        .expect("create is denied without a request");
    let request = http::Request::builder()
        .uri("/admin/items")
        .extension(Tenant(Uuid::new_v4()))
        .body(topcoat::router::Body::empty())
        .unwrap();
    let response = router.handle(request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(!html.contains("/admin/items/create"), "{html}");
}
