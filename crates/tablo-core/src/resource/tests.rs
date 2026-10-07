use toasty::Db;
use topcoat::context::CxTestBuilder;

use super::*;
use crate::{Tenancy, TenantId, lens, test_support::User};

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
            lens!(User.name),
        )))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<User>> {
        // Custom scoping example: only users named Ada
        toasty::stmt::Query::<List<User>>::all().filter(User::fields().name().eq("Ada"))
    }
}

struct BareResource;

impl Resource for BareResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
            lens!(User.name),
        )))
    }
}

#[tokio::test]
async fn query_seam_is_cloneable_via_db_helper() {
    // Proves the seam composes with the `db(cx)` helper without taking
    // ownership of the query.
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(User { name: "Ada" })
        .exec(&mut db)
        .await
        .unwrap();
    toasty::create!(User { name: "Bob" })
        .exec(&mut db)
        .await
        .unwrap();

    let cx = CxTestBuilder::new().app_context(db).build();
    let mut db = crate::db::db(&cx);
    let rows = UserResource::query(&cx).exec(&mut db).await.unwrap();
    // Custom query filters to Ada only
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");

    let rows_all = BareResource::query(&cx).exec(&mut db).await.unwrap();
    assert_eq!(rows_all.len(), 2);
}

/// A model with its own tenant column.
#[derive(Debug, Clone, toasty::Model)]
struct Owned {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    name: String,
}

/// A resource scoped by its model's tenant column.
struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .tenancy(Tenancy::column(Owned::fields().tenant_id()))
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Owned.name),
            )))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<Owned>> {
        toasty::stmt::Query::<List<Owned>>::all().filter(Owned::fields().name().ne("Hidden"))
    }
}

/// The tenancy filter is ANDed onto the resource's own query, and a request
/// with no tenant runs no query at all.
#[tokio::test]
async fn tenancy_is_anded_onto_the_base_query() {
    let mut db = Db::builder()
        .models(toasty::models!(Owned))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    for (tenant_id, name) in [
        (TenantId::from(mine), "Mine"),
        (TenantId::from(mine), "Hidden"),
        (TenantId::from(theirs), "Theirs"),
    ] {
        toasty::create!(Owned { tenant_id, name })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let tenantless = crate::test_support::panel_cx::<OwnedResource>(&db);
    let cx = tenantless.with(crate::Tenant(mine));
    let mut db = crate::db::db(&cx);
    let rows = scoped_query::<OwnedResource>(&cx)
        .expect("the request has a tenant")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Mine");

    assert!(scoped_query::<OwnedResource>(&tenantless).is_err());
}

/// A model whose tenant column accepts no tenant.
#[derive(Debug, Clone, toasty::Model)]
struct Assignable {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: Option<TenantId>,
    name: String,
}

/// A resource scoped by its nullable tenant column.
struct AssignableResource;

impl Resource for AssignableResource {
    type Model = Assignable;
    type Form = OptionTenantForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .tenancy(Tenancy::column(Assignable::fields().tenant_id()))
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Assignable.name),
            )))
    }
}

#[derive(crate::RecordForm)]
#[form(model = Assignable)]
struct OptionTenantForm {
    name: String,
}

/// A nullable tenant column is stamped `Some(request tenant)`, and scoped so
/// that neither another tenant's row nor a tenantless one is served.
#[tokio::test]
async fn a_nullable_tenant_column_is_stamped_and_scoped() {
    let mut db = Db::builder()
        .models(toasty::models!(Assignable))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    for (tenant_id, name) in [(Some(TenantId::from(theirs)), "Theirs"), (None, "Nobody")] {
        toasty::create!(Assignable { tenant_id, name })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let cx = crate::test_support::panel_cx::<AssignableResource>(&db).with(crate::Tenant(mine));
    let mut db = crate::db::db(&cx);
    let created = write_create::<AssignableResource>(
        &cx,
        OptionTenantForm {
            name: "Mine".to_string(),
        },
        &mut db,
    )
    .await
    .expect("the create runs");
    assert_eq!(created.tenant_id, Some(TenantId::from(mine)));

    let rows = scoped_query::<AssignableResource>(&cx)
        .expect("the request has a tenant")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only the request tenant's row is served");
    assert_eq!(rows[0].name, "Mine");
}

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
}

/// A resource with no record label: the title keeps the resource's label and the record key.
struct Unlabelled;

impl Resource for Unlabelled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
            lens!(Note.title),
        )))
    }
}

/// A resource that labels its records with the note's title.
struct Labelled;

impl Resource for Labelled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Note.title),
            )))
            .record_label(|_cx: &Cx, note: &Note| {
                (!note.title.is_empty()).then(|| note.title.clone())
            })
    }
}

fn note() -> Note {
    Note {
        id: uuid::Uuid::nil(),
        title: "A Title".to_string(),
    }
}

#[test]
fn a_resource_without_a_record_label_titles_the_record_with_its_key() {
    assert_eq!(
        crate::test_support::mounted::<Unlabelled>().record_title(
            &crate::test_support::cx(),
            &note(),
            "8f14e45f"
        ),
        "Note 8f14e45f"
    );
}

#[test]
fn a_declared_record_label_titles_the_record_it_labels() {
    let cx = crate::test_support::cx();
    let mounted = crate::test_support::mounted::<Labelled>();
    assert_eq!(mounted.record_title(&cx, &note(), "8f14e45f"), "A Title");
    let untitled = Note {
        title: String::new(),
        ..note()
    };
    assert_eq!(
        mounted.record_title(&cx, &untitled, "8f14e45f"),
        "Note 8f14e45f",
        "a record the label declines falls back to the key"
    );
}
