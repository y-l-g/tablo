use toasty::Db;
use topcoat::context::CxTestBuilder;

use super::*;
use crate::{Tenancy, lens, test_support::User};

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn table() -> crate::resource::Table<User> {
        crate::resource::Table::new(crate::resource::TextColumn::new(lens!(User.name)))
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

    fn table() -> crate::resource::Table<User> {
        crate::resource::Table::new(crate::resource::TextColumn::new(lens!(User.name)))
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
    tenant_id: uuid::Uuid,
    name: String,
}

/// A resource scoped by its model's tenant column.
struct OwnedResource;

impl Resource for OwnedResource {
    type Model = Owned;
    type Form = crate::NoForm<Self::Model>;

    fn table() -> crate::resource::Table<Owned> {
        crate::resource::Table::new(crate::resource::TextColumn::new(lens!(Owned.name)))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<Owned>> {
        toasty::stmt::Query::<List<Owned>>::all().filter(Owned::fields().name().ne("Hidden"))
    }

    fn tenancy() -> Tenancy<Owned> {
        Tenancy::column(Owned::fields().tenant_id())
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
    for (tenant_id, name) in [(mine, "Mine"), (mine, "Hidden"), (theirs, "Theirs")] {
        toasty::create!(Owned { tenant_id, name })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let cx = CxTestBuilder::new()
        .app_context(db)
        .request_context(crate::Tenant(mine))
        .build();
    let mut db = crate::db::db(&cx);
    let rows = scoped_query::<OwnedResource>(&cx)
        .expect("the request has a tenant")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Mine");

    let tenantless = CxTestBuilder::new().build();
    assert!(scoped_query::<OwnedResource>(&tenantless).is_err());
}
