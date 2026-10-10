//! Record forms, end to end.

use std::collections::{HashMap, HashSet};

use http::StatusCode;
use tablo::{
    Ability, ComputedColumn, DeclarationErrorKind, Detail, FieldErrorKind, FieldErrors, NoForm,
    Panel, RecordForm, Resource, ResourceDef, Schema, Table, Tenancy, Tenant, TenantId, TextColumn,
    lens, write_create,
};
use toasty::Db;
use topcoat::context::{Cx, CxTestBuilder};
use uuid::Uuid;

use crate::framework::common::{
    body_string, field_error, get, input_value, memory_db, mount, panel, panel_cx, panel_router,
    post_fields, refusal,
};

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

/// Every column; `title` alone has no blank answer.
#[derive(tablo::RecordForm)]
#[form(model = Item)]
struct ItemForm {
    title: String,
    #[form(optional)]
    notes: String,
    #[form(blank = 3)]
    priority: i64,
    #[form(blank = false)]
    done: bool,
}

fn item_schema() -> Schema<ItemForm> {
    let c = ItemForm::controls();
    Schema::new((
        c.title,
        c.notes,
        c.priority,
        c.done.choice().options(["true", "false"]),
    ))
}

fn item_table() -> Table<Item> {
    Table::new(TextColumn::new(lens!(Item.title)))
}

struct ItemResource;

impl Resource for ItemResource {
    type Model = Item;
    type Form = ItemForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            })
            .table(item_table())
            .form(item_schema())
            .detail(Detail::new(TextColumn::new(lens!(Item.title))))
    }

    fn validate_record(_cx: &Cx, form: &ItemForm) -> FieldErrors<ItemFormField> {
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

/// The derived update reloads the record it wrote.
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
        &map(&[("title", "Kept"), ("notes", " "), ("priority", "")]),
    )
    .expect("every emptied field answers blank");
    assert_eq!(
        form.notes, "",
        "an optional `String` answers the empty string"
    );
    assert_eq!(form.priority, 3, "a declared blank answers itself");
    assert!(!form.done, "an absent `bool` reads as `false`");
    let Err(errors) = ItemForm::parse(&cx, &map(&[("title", " ")])) else {
        panic!("a `String` with no blank answer is required");
    };
    assert_eq!(errors[0].key, "title");
    assert_eq!(errors[0].kind, FieldErrorKind::Required);
}

#[tokio::test]
async fn a_blank_with_no_answer_and_a_bad_value_are_refused_by_key() {
    #[derive(tablo::RecordForm)]
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
    assert_eq!(
        errors[1].kind,
        FieldErrorKind::Invalid("`maybe` is not a valid yes/no value".to_string())
    );
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

/// An edit that posts one key changes one field.
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

/// A submission naming no form field runs no statement.
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

/// Every refused key renders in one round, and `validate_record` runs once the form parses.
#[tokio::test]
async fn parse_and_record_errors_render_inline() {
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<ItemResource>(db.clone());
    let edit = format!("/admin/items/{}/edit", item.id);
    let response = post_fields(&router, &edit, &[("title", ""), ("priority", "lots")]).await;
    assert_eq!(response.status(), StatusCode::OK, "the form re-renders");
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "title").as_deref(),
        Some("Title is required"),
        "{html}"
    );
    assert_eq!(
        field_error(&html, "priority").as_deref(),
        Some("`lots` is not a valid whole number"),
        "{html}"
    );

    let response = post_fields(&router, &edit, &[("priority", "11")]).await;
    assert_eq!(response.status(), StatusCode::OK, "the form re-renders");
    let html = body_string(response).await;
    assert_eq!(
        field_error(&html, "priority").as_deref(),
        Some("Priority is at most 10"),
        "{html}"
    );
    let stored = reload(&db, item.id).await;
    assert_eq!(stored.priority, 7, "a refused submission writes nothing");
}

/// The detail page of a form resource reads the form's projection.
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
    tenant_id: TenantId,
    title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Owned)]
struct OwnedForm {
    title: String,
}

struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;
    type Form = OwnedForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("owned")
            .policy(|_cx: &Cx, ability: Ability<'_, Owned>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            })
            .tenancy(Tenancy::column(Owned::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Owned.title))))
    }
}

#[tokio::test]
async fn the_derived_create_stamps_the_request_tenant() {
    let db = memory_db(toasty::models!(Owned)).await;
    let tenant = Uuid::new_v4();
    let cx = panel_cx::<OwnedResource>(&db).with(Tenant(tenant));
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
    assert_eq!(
        created.tenant_id.get(),
        tenant,
        "the request tenant is stamped"
    );
}

/// What a panel over `R` refuses to mount with.
fn refused<R: Resource>(db: Db) -> Vec<DeclarationErrorKind> {
    refusal(mount(db, panel().resource::<R>()))
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// A resource over [`Item`] that allows create through [`ItemForm`] placed by `schema`.
macro_rules! item_resource {
    ($name:ident, $schema:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;
            type Form = ItemForm;

            fn declare() -> ResourceDef<Self> {
                ResourceDef::new()
                    .slug("items")
                    .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                        matches!(ability, Ability::Create)
                    })
                    .table(item_table())
                    .form($schema)
            }
        }
    };
}

#[derive(tablo::RecordForm)]
#[form(model = Item)]
struct TitleForm {
    title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Item)]
struct PriorityForm {
    priority: i64,
}

/// A field the form does not place follows the ones it does, in declaration order, and a form
/// that places nothing renders them all.
#[tokio::test]
async fn a_field_the_form_does_not_place_follows_the_ones_it_does() {
    item_resource!(Partly, Schema::new(ItemForm::controls().priority));
    item_resource!(Unplaced, Schema::default());

    /// Where each control posting one of `names` sits on the create page.
    async fn positions<R: Resource>(names: &[&str]) -> Vec<usize> {
        let router = panel_router::<R>(item_db().await);
        let html = body_string(get(&router, "/admin/items/create").await).await;
        names
            .iter()
            .map(|name| {
                html.find(&format!("name=\"{name}\""))
                    .unwrap_or_else(|| panic!("no control posts {name}: {html}"))
            })
            .collect()
    }

    let placed = positions::<Partly>(&["priority", "title", "notes", "done"]).await;
    assert!(placed.is_sorted(), "{placed:?}");
    let unplaced = positions::<Unplaced>(&["title", "notes", "priority", "done"]).await;
    assert!(unplaced.is_sorted(), "{unplaced:?}");
}

#[tokio::test]
async fn build_refuses_a_gated_form_claiming_the_tenant_column() {
    #[derive(tablo::RecordForm)]
    #[form(model = Owned)]
    struct ClaimingForm {
        tenant_id: TenantId,
        title: String,
    }

    struct Claiming;

    impl Resource for Claiming {
        type Model = Owned;
        type Form = ClaimingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .tenancy(Tenancy::column(Owned::fields().tenant_id()))
                .table(Table::new(TextColumn::new(lens!(Owned.title))))
        }
    }

    let errors = refused::<Claiming>(memory_db(toasty::models!(Owned)).await);
    assert!(
        errors.contains(&DeclarationErrorKind::FormClaimsTenantColumn {
            field: "tenant_id".to_string(),
            column: "tenant_id".to_string(),
        }),
        "{errors:?}"
    );
}

/// A `NoForm` resource over [`Item`].
macro_rules! list_only_resource {
    ($name:ident, $create:expr) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;
            type Form = NoForm<Self::Model>;

            fn declare() -> ResourceDef<Self> {
                ResourceDef::new()
                    .slug("items")
                    .policy(|_cx: &Cx, ability: Ability<'_, Item>| match ability {
                        Ability::Create => $create,
                        _ => false,
                    })
                    .table(item_table())
            }
        }
    };
}

/// A hand-written record form whose `control` renders nothing leaves its fields with no control.
#[tokio::test]
async fn build_refuses_a_field_record_form_control_does_not_render() {
    struct Bare(TitleForm);

    impl RecordForm for Bare {
        type Model = Item;
        type Field = TitleFormField;

        fn fields(resolver: &tablo::FieldResolver) -> Vec<tablo::FormField<TitleFormField>> {
            TitleForm::fields(resolver)
        }

        fn control(_field: TitleFormField) -> Schema<Self> {
            Schema::default()
        }

        fn hydrate(cx: &Cx, record: &Item) -> HashMap<String, String> {
            TitleForm::hydrate(cx, record)
        }

        fn parse(
            cx: &Cx,
            values: &HashMap<String, String>,
        ) -> Result<Self, Vec<tablo::FieldError>> {
            TitleForm::parse(cx, values).map(Bare)
        }

        fn into_create(self) -> <Item as toasty::schema::Model>::Create {
            self.0.into_create()
        }

        fn into_update<'a>(
            self,
            record: &'a mut Item,
            named: &HashSet<TitleFormField>,
        ) -> Option<<Item as toasty::schema::Model>::Update<'a>> {
            self.0.into_update(record, named)
        }

        fn exec_update<'a>(
            update: <Item as toasty::schema::Model>::Update<'a>,
            ex: &'a mut dyn toasty::Executor,
        ) -> impl std::future::Future<Output = toasty::Result<()>> + Send + 'a {
            TitleForm::exec_update(update, ex)
        }
    }

    struct Uncontrolled;

    impl Resource for Uncontrolled {
        type Model = Item;
        type Form = Bare;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|_cx: &Cx, ability: Ability<'_, Item>| matches!(ability, Ability::ViewAny))
                .table(item_table())
        }
    }

    assert_eq!(
        refused::<Uncontrolled>(item_db().await),
        [DeclarationErrorKind::MissingControl {
            field: "title".to_string(),
            key: "title".to_string(),
        }]
    );
}

/// A choice with neither options nor a relationship renders an empty `<select>`, and its
/// validation, with no option to check against, would accept any value: mounting refuses it.
#[tokio::test]
async fn build_refuses_a_choice_that_offers_nothing() {
    item_resource!(Empty, {
        let c = ItemForm::controls();
        Schema::new(c.notes.choice())
    });
    assert_eq!(
        refused::<Empty>(item_db().await),
        [DeclarationErrorKind::EmptyChoice {
            field: "notes".to_string(),
        }]
    );
}

#[tokio::test]
async fn build_refuses_a_list_only_resource_that_allows_create() {
    list_only_resource!(Creating, true);
    assert_eq!(
        refused::<Creating>(item_db().await),
        [DeclarationErrorKind::CreateWithoutForm]
    );
}

#[tokio::test]
async fn a_list_only_resource_serves_no_form_route() {
    list_only_resource!(Listed, false);
    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Listed>(db.clone());
    let edit = format!("/admin/items/{}/edit", item.id);
    // `create` falls to the GET-only detail route.
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

/// A `NoForm` resource declares its detail page, whose columns read the record.
#[tokio::test]
async fn a_list_only_resource_declares_its_detail_page() {
    struct Viewed;

    impl Resource for Viewed {
        type Model = Item;
        type Form = NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|_cx: &Cx, ability: Ability<'_, Item>| matches!(ability, Ability::View(_)))
                .table(item_table())
                .detail(Detail::new(ComputedColumn::new("Title", |item: &Item| {
                    format!("{} (view)", item.title)
                })))
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
    let router: topcoat::router::Router = mount(
        db,
        Panel::new("admin")
            .auth(tablo::Auth::disabled())
            .resource::<ItemResource>(),
    )
    .expect("panel builds");
    let create = get(&router, "/admin/items/create").await;
    assert_eq!(create.status(), StatusCode::OK);
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

/// A resource over [`Item`] that allows create through `F`.
macro_rules! title_only_resource {
    ($name:ident, [$($column:ident),*]) => {
        struct $name;

        impl Resource for $name {
            type Model = Item;
            type Form = TitleForm;

            fn declare() -> ResourceDef<Self> {
                ResourceDef::new()
                    .slug("items")
                    .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                        matches!(ability, Ability::Create)
                    })
                    .table(item_table())
                    $(.create_column(Item::fields().$column()))*
            }
        }
    };
}

#[tokio::test]
async fn build_refuses_a_create_that_leaves_a_required_column_unset() {
    title_only_resource!(Partial, []);
    let errors = refused::<Partial>(item_db().await);
    assert!(
        errors.contains(&DeclarationErrorKind::UnwrittenColumn {
            column: "notes".to_string()
        }),
        "{errors:?}"
    );
}

#[tokio::test]
async fn create_columns_names_what_an_override_sets() {
    title_only_resource!(Covered, [notes, priority, done]);
    mount(item_db().await, panel().resource::<Covered>())
        .expect("the override's own columns are declared");
}

/// A value the control lets through but the field's type refuses renders inline.
#[tokio::test]
async fn a_value_the_form_type_refuses_renders_inline() {
    #[derive(tablo::RecordForm)]
    #[form(model = Item)]
    struct LooseForm {
        priority: i64,
    }

    struct Loose;

    impl Resource for Loose {
        type Model = Item;
        type Form = LooseForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                    matches!(ability, Ability::View(_) | Ability::Update(_))
                })
                .table(item_table())
                .form(
                    // A static-options select checks membership, not the column's type.
                    Schema::new(
                        LooseForm::controls()
                            .priority
                            .choice()
                            .options(["1", "lots"]),
                    ),
                )
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
    assert_eq!(
        field_error(&html, "priority").as_deref(),
        Some("`lots` is not a valid whole number"),
        "{html}"
    );
    assert_eq!(reload(&db, item.id).await.priority, 7);
}

/// An unkeyable record rule fails closed.
#[tokio::test]
async fn an_unkeyable_record_rule_fails_closed() {
    /// [`PriorityForm`] with no keys: nothing can render its errors.
    struct Keyless(PriorityForm);

    impl RecordForm for Keyless {
        type Model = Item;
        type Field = PriorityFormField;

        fn fields(_: &tablo::FieldResolver) -> Vec<tablo::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn control(_field: PriorityFormField) -> Schema<Self> {
            Schema::default()
        }

        fn hydrate(cx: &Cx, record: &Item) -> HashMap<String, String> {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(
            _cx: &Cx,
            _values: &HashMap<String, String>,
        ) -> Result<Self, Vec<tablo::FieldError>> {
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                    matches!(ability, Ability::View(_) | Ability::Update(_))
                })
                .table(item_table())
        }

        fn validate_record(_cx: &Cx, _form: &Keyless) -> FieldErrors<PriorityFormField> {
            let mut errors = FieldErrors::new();
            errors.add(PriorityFormField::Priority, "never");
            errors
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Refusing>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// An unkeyable parse failure fails closed.
#[tokio::test]
async fn an_unkeyable_parse_failure_fails_closed() {
    /// [`PriorityForm`] with no keys, refusing on a key nothing renders.
    struct Unkeyable(PriorityForm);

    impl RecordForm for Unkeyable {
        type Model = Item;
        type Field = PriorityFormField;

        fn fields(_: &tablo::FieldResolver) -> Vec<tablo::FormField<PriorityFormField>> {
            Vec::new()
        }

        fn control(_field: PriorityFormField) -> Schema<Self> {
            Schema::default()
        }

        fn hydrate(cx: &Cx, record: &Item) -> HashMap<String, String> {
            PriorityForm::hydrate(cx, record)
        }

        fn parse(
            _cx: &Cx,
            _values: &HashMap<String, String>,
        ) -> Result<Self, Vec<tablo::FieldError>> {
            Err(vec![tablo::FieldError::invalid("never", "never renders")])
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|_cx: &Cx, ability: Ability<'_, Item>| {
                    matches!(ability, Ability::View(_) | Ability::Update(_))
                })
                .table(item_table())
        }
    }

    let db = item_db().await;
    let item = seed_item(&db).await;
    let router = panel_router::<Parseless>(db.clone());
    let response = post_fields(&router, &format!("/admin/items/{}/edit", item.id), &[]).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(reload(&db, item.id).await.priority, 7, "nothing is written");
}

/// A list-only resource never links to create.
#[tokio::test]
async fn a_list_only_resource_never_links_to_create() {
    struct TenantCreates;

    impl Resource for TenantCreates {
        type Model = Item;
        type Form = tablo::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("items")
                .policy(|cx: &Cx, ability: Ability<'_, Item>| match ability {
                    Ability::ViewAny => true,
                    Ability::Create => tablo::tenant_id(cx).is_some(),
                    _ => false,
                })
                .table(item_table())
        }
    }

    let router = mount(item_db().await, panel().resource::<TenantCreates>())
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

/// The derived default form renders and writes.
#[tokio::test]
async fn the_derived_default_form_renders_and_writes() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, tablo::Options)]
    enum WidgetRole {
        Admin,
        Member,
    }

    assert_eq!(WidgetRole::Admin.value(), "admin");

    #[derive(Debug, Clone, toasty::Model)]
    struct Widget {
        #[key]
        #[auto]
        id: Uuid,
        name: String,
        role: String,
        active: bool,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Widget)]
    struct WidgetForm {
        name: String,
        #[form(options = WidgetRole, blank = WidgetRole::Member.value())]
        role: String,
        active: bool,
    }

    struct WidgetResource;

    impl Resource for WidgetResource {
        type Model = Widget;
        type Form = WidgetForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("widgets")
                .policy(|_cx: &Cx, ability: Ability<'_, Widget>| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                    )
                })
                .table(Table::new(TextColumn::new(lens!(Widget.name))))
        }
    }

    let db = memory_db(toasty::models!(Widget)).await;
    let router = panel_router::<WidgetResource>(db.clone());
    let create = get(&router, "/admin/widgets/create").await;
    assert_eq!(create.status(), StatusCode::OK);
    let html = body_string(create).await;
    for name in ["name", "role", "active"] {
        assert!(
            html.contains(&format!("name=\"{name}\"")),
            "`{name}` posts: {html}"
        );
    }
    assert!(
        html.find("name=\"name\"").unwrap() < html.find("name=\"role\"").unwrap()
            && html.find("name=\"role\"").unwrap() < html.find("name=\"active\"").unwrap(),
        "the default schema keeps declaration order: {html}"
    );
    assert!(
        html.contains("value=\"admin\"") && html.contains("value=\"member\""),
        "the choice carries the Options list: {html}"
    );

    let response = post_fields(
        &router,
        "/admin/widgets/create",
        &[("name", "New"), ("role", "admin")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let mut handle = db.clone();
    let rows = Widget::all().exec(&mut handle).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "New");
    assert_eq!(rows[0].role, "admin");
    assert!(!rows[0].active, "an absent toggle reads as false");

    let edit = get(&router, &format!("/admin/widgets/{}/edit", rows[0].id)).await;
    assert_eq!(edit.status(), StatusCode::OK);
    let html = body_string(edit).await;
    assert_eq!(
        input_value(&html, "name").as_deref(),
        Some("New"),
        "the edit form hydrates the stored value: {html}"
    );
    let response = post_fields(
        &router,
        &format!("/admin/widgets/{}/edit", rows[0].id),
        &[("active", "true")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let mut handle = db.clone();
    let stored = Widget::get_by_id(&mut handle, &rows[0].id)
        .await
        .expect("the widget exists");
    assert!(stored.active);
    assert_eq!(stored.name, "New", "an unposted key keeps its value");
    assert_eq!(stored.role, "admin", "an unposted key keeps its value");
}

#[derive(Debug, Clone, toasty::Model)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    subject: String,
    status: String,
    urgent: bool,
    estimate: i64,
}

#[derive(tablo::Options)]
enum TicketStatus {
    Open,
    #[option(label = "Waiting on customer")]
    Waiting,
}

#[derive(tablo::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    subject: String,
    #[form(options = TicketStatus)]
    status: String,
    urgent: bool,
    estimate: i64,
}

/// A resource naming only its model, its form and its policy.
struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(tablo::ReadOnly)
    }
}

/// The same resource with its detail page turned off.
struct UnviewedTicketResource;

impl Resource for UnviewedTicketResource {
    type Model = Ticket;
    type Form = TicketForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .detail(Detail::empty())
    }
}

async fn ticket_db() -> (Db, Ticket) {
    let mut db = memory_db(toasty::models!(Ticket)).await;
    let ticket = toasty::create!(Ticket {
        subject: "Printer jam".to_string(),
        status: TicketStatus::Waiting.value().to_string(),
        urgent: true,
        estimate: 3,
    })
    .exec(&mut db)
    .await
    .expect("seed ticket");
    toasty::create!(Ticket {
        subject: "New laptop".to_string(),
        status: TicketStatus::Open.value().to_string(),
        urgent: false,
        estimate: 8,
    })
    .exec(&mut db)
    .await
    .expect("seed ticket");
    (db, ticket)
}

/// The record form derives the table: a sortable column per text field, searchable over a
/// `String`, an options field by its label, and a toggle as yes or no.
#[tokio::test]
async fn the_record_form_derives_the_table() {
    let (db, _) = ticket_db().await;
    let router = panel_router::<TicketResource>(db);

    let html = body_string(get(&router, "/admin/tickets?sort=estimate&dir=desc").await).await;
    for header in ["Subject", "Status", "Urgent", "Estimate"] {
        assert!(html.contains(header), "a {header} column: {html}");
    }
    assert!(
        html.contains("Waiting on customer") && !html.contains(">waiting<"),
        "an options field shows its label: {html}"
    );
    assert!(html.contains("Yes") && html.contains("No"), "{html}");
    assert!(
        html.find("New laptop").unwrap() < html.find("Printer jam").unwrap(),
        "sorted by the estimate, descending: {html}"
    );

    let searched = body_string(get(&router, "/admin/tickets?q=printer").await).await;
    assert!(
        searched.contains("Printer jam") && !searched.contains("New laptop"),
        "searched by the subject: {searched}"
    );
}

/// The record form derives the detail page: each field the table lists, read-only; an empty
/// view turns it off.
#[tokio::test]
async fn the_record_form_derives_the_detail_page() {
    let (db, ticket) = ticket_db().await;
    let viewed = panel_router::<TicketResource>(db.clone());
    let detail = get(&viewed, &format!("/admin/tickets/{}", ticket.id)).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = body_string(detail).await;
    for (label, value) in [
        ("Subject", "Printer jam"),
        ("Status", "Waiting on customer"),
        ("Urgent", "Yes"),
        ("Estimate", ">3<"),
    ] {
        assert!(
            entry(&detail, label).contains(value),
            "{label} shows {value}: {detail}"
        );
    }
    assert_eq!(
        input_value(&detail, "subject"),
        None,
        "the detail page renders no control for the field: {detail}"
    );

    let unviewed = panel_router::<UnviewedTicketResource>(db);
    let detail = get(&unviewed, &format!("/admin/unviewed-tickets/{}", ticket.id)).await;
    assert_eq!(detail.status(), StatusCode::NOT_FOUND);
}

/// The detail page's entry labelled `label`: its markup up to the next entry.
fn entry<'h>(html: &'h str, label: &str) -> &'h str {
    let at = html
        .find(&format!(">{label}<"))
        .unwrap_or_else(|| panic!("no {label} entry: {html}"));
    let rest = &html[at..];
    &rest[..rest.find("data-slot=\"field\"").unwrap_or(rest.len())]
}

/// A post's stage, stored as an embedded enum.
#[derive(Debug, Clone, toasty::Embed, tablo::EmbeddedForm)]
enum Stage {
    #[column(variant = 1)]
    Draft,
    #[column(variant = 2)]
    Out { note: String },
}

#[derive(Debug, Clone, toasty::Model)]
struct Attachment {
    #[key]
    #[auto]
    id: Uuid,
    path: String,
    owner_id: Uuid,
    stage: Stage,
}

#[derive(tablo::RecordForm)]
#[form(model = Attachment)]
struct AttachmentForm {
    #[form(file)]
    path: String,
    // Any source serves: the test reads the key the detail page shows.
    #[form(relationship = AttachmentResource)]
    owner_id: Uuid,
    #[form(embed)]
    stage: Stage,
}

/// An uploader for a panel whose test never uploads: a file field mounts only with one.
struct KeepName;

impl tablo::Uploader for KeepName {
    async fn store(&self, filename: &str, _bytes: &[u8]) -> Result<String, String> {
        Ok(filename.to_string())
    }
}

/// A resource whose detail page the record form derives, over a file, a relationship and an
/// embedded enum.
struct AttachmentResource;

impl Resource for AttachmentResource {
    type Model = Attachment;
    type Form = AttachmentForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(Table::new((
                TextColumn::new(lens!(Attachment.path)),
                tablo::EmbeddedColumn::new(lens!(Attachment.stage)),
            )))
    }
}

/// The derived detail page links a file, shows a relationship's key and an embedded enum's stored
/// variant; the table's embedded column exports the same reading.
#[tokio::test]
async fn the_derived_detail_page_shows_files_keys_and_embedded_values() {
    let mut db = memory_db(toasty::models!(Attachment)).await;
    let owner = Uuid::new_v4();
    let attachment = toasty::create!(Attachment {
        path: "/uploads/a.pdf".to_string(),
        owner_id: owner,
        stage: Stage::Out {
            note: "shipped".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .expect("seed attachment");
    let router = mount(
        db,
        panel().uploads(KeepName).resource::<AttachmentResource>(),
    )
    .expect("panel builds");

    let detail =
        body_string(get(&router, &format!("/admin/attachments/{}", attachment.id)).await).await;
    assert!(
        entry(&detail, "Path").contains("href=\"/uploads/a.pdf\""),
        "a file links its path: {detail}"
    );
    assert!(
        entry(&detail, "Owner id").contains(&owner.to_string()),
        "a relationship shows its key: {detail}"
    );
    assert!(entry(&detail, "Stage").contains(">Out<"), "{detail}");
    assert!(entry(&detail, "Note").contains("shipped"), "{detail}");

    let csv = body_string(get(&router, "/admin/attachments/export").await).await;
    assert!(
        csv.contains("/uploads/a.pdf,\"Stage: Out, Note: shipped\"\n"),
        "the embedded column exports its leaves: {csv}"
    );
}

/// Whether the control posting `name` renders `required`.
fn renders_required(html: &str, name: &str) -> bool {
    let needle = format!("name=\"{name}\"");
    html.split('<')
        .filter_map(|tag| tag.split_once('>').map(|(attrs, _)| attrs))
        .find(|attrs| attrs.contains(&needle))
        .unwrap_or_else(|| panic!("no control posts {name}: {html}"))
        .split_whitespace()
        .any(|attr| attr == "required" || attr.starts_with("required="))
}

/// The record form decides which controls render required, not the controls `item_schema`
/// places.
#[tokio::test]
async fn the_record_form_decides_which_controls_are_required() {
    let router = panel_router::<ItemResource>(item_db().await);
    let html = body_string(get(&router, "/admin/items/create").await).await;
    assert!(renders_required(&html, "title"), "no blank answer: {html}");
    assert!(!renders_required(&html, "notes"), "`optional`: {html}");
    assert!(
        !renders_required(&html, "priority"),
        "a declared blank: {html}"
    );
}

#[tokio::test]
async fn build_refuses_a_unique_field_an_empty_submission_answers() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Handle {
        #[key]
        #[auto]
        id: Uuid,
        #[unique]
        name: String,
    }

    #[derive(tablo::RecordForm)]
    #[form(model = Handle)]
    struct HandleForm {
        #[form(optional)]
        name: String,
    }

    struct HandleResource;

    impl Resource for HandleResource {
        type Model = Handle;
        type Form = HandleForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(Table::new(TextColumn::new(lens!(Handle.name))))
        }
    }

    let errors = refused::<HandleResource>(memory_db(toasty::models!(Handle)).await);
    assert_eq!(
        errors,
        [DeclarationErrorKind::OptionalUnique {
            field: "name".to_string()
        }]
    );
}
