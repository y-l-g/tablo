use toasty::Db;
use topcoat::context::CxTestBuilder;

use super::*;
use crate::test_support::User;

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn table() -> crate::resource::Table<User> {
        crate::resource::Table::new(
            |r: &User| r.id.to_string(),
            crate::resource::TextColumn::r#for(User::fields().name(), |r: &User| r.name.clone()),
        )
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
        crate::resource::Table::new(
            |r: &User| r.id.to_string(),
            crate::resource::TextColumn::r#for(User::fields().name(), |r: &User| r.name.clone()),
        )
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

/// A gated resource over a model the framework cannot scope from: declared
/// as requiring a tenant, no `tenant_id` column to derive the filter from.
struct Misdeclared;

impl Resource for Misdeclared {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn table() -> crate::resource::Table<User> {
        crate::resource::Table::new(
            |r: &User| r.id.to_string(),
            crate::resource::TextColumn::r#for(User::fields().name(), |r: &User| r.name.clone()),
        )
    }

    fn requires_tenant() -> bool {
        true
    }
}

/// the failure mode is an error naming the resource, not a
/// fallback to the unscoped query. `User` has no `tenant_id`, and
/// `scoped_query` must refuse to answer rather than serve every row.
#[test]
fn gated_resource_without_a_tenant_column_fails_closed() {
    let cx = CxTestBuilder::new()
        .request_context(crate::Tenant(uuid::Uuid::new_v4()))
        .build();
    let error = scoped_query::<Misdeclared>(&cx).expect_err("must not run unscoped");
    let message = error.to_string();
    assert!(
        message.contains("misdeclareds"),
        "the error must name the resource: {message}"
    );
    assert!(
        message.contains("tenant_id") && message.contains("tenant_scope"),
        "the error must name both ways to scope it: {message}"
    );
}

/// A gated resource whose tenancy is not a column on its own model declares
/// the predicate itself — the shape the showcase's comments need,
/// where the tenant lives on the parent post. `name` stands in for the
/// relation path here: the point is that the hook is consulted and ANDed.
struct DeclaredScope;

impl Resource for DeclaredScope {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn table() -> crate::resource::Table<User> {
        crate::resource::Table::new(
            |r: &User| r.id.to_string(),
            crate::resource::TextColumn::r#for(User::fields().name(), |r: &User| r.name.clone()),
        )
    }

    fn requires_tenant() -> bool {
        true
    }

    fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
        Some(User::fields().name().eq(tenant.to_string()))
    }
}

#[tokio::test]
async fn declared_tenant_scope_is_anded_onto_the_base_query() {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    for name in [mine.to_string(), theirs.to_string()] {
        toasty::create!(User { name }).exec(&mut db).await.unwrap();
    }
    let cx = CxTestBuilder::new()
        .app_context(db)
        .request_context(crate::Tenant(mine))
        .build();
    let mut db = crate::db::db(&cx);
    let rows = scoped_query::<DeclaredScope>(&cx)
        .expect("a declared scope is not a misdeclaration")
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, mine.to_string());

    // And the gate still runs first: no tenant, no query.
    let tenantless = CxTestBuilder::new().build();
    assert!(scoped_query::<DeclaredScope>(&tenantless).is_err());
}
