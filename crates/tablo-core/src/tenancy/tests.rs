use topcoat::context::CxTestBuilder;

use super::*;

fn cx_with_header(value: &str) -> Cx {
    let mut parts = http::Request::builder()
        .uri("/")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    parts.headers.insert(
        "x-tenant-id",
        value.parse().expect("header value must parse"),
    );
    CxTestBuilder::new().request_context(parts).build()
}

#[test]
fn tenant_extension_wins_over_scoped_value_and_defaults_to_none() {
    let id = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();

    // The auth layer's scoped value is the production source.
    let cx = CxTestBuilder::new().request_context(Tenant(id)).build();
    assert_eq!(tenant_id(&cx), Some(id));

    // A server-set request extension overrides it deliberately.
    let cx = CxTestBuilder::new()
        .request_context({
            let mut parts = http::Request::builder()
                .uri("/")
                .body(())
                .unwrap()
                .into_parts()
                .0;
            parts.extensions.insert(Tenant(id));
            parts
        })
        .request_context(Tenant(other))
        .build();
    assert_eq!(tenant_id(&cx), Some(id));

    // Nothing set → None (callers must reject tenantless access).
    let cx = CxTestBuilder::new().build();
    assert_eq!(tenant_id(&cx), None);
}

#[test]
fn tenant_header_is_never_trusted() {
    // GH #131: the header fallback is gone; spoofing another tenant's
    // UUID in `x-tenant-id` must not resolve a tenant.
    let id = uuid::Uuid::new_v4();
    assert_eq!(tenant_id(&cx_with_header(&id.to_string())), None);
}

/// A model with the conventional column, one without any, and one whose
/// same-named column is the wrong type.
#[derive(Debug, Clone, toasty::Model)]
struct Scoped {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Unscoped {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct WronglyTyped {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: String,
}

#[test]
fn tenant_column_is_discovered_by_name_and_uuid_type() {
    // `id` is index 0, `tenant_id` index 1, `name` index 2.
    assert_eq!(tenant_field_index::<Scoped>(), Some(1));
    // No column at all, and a `tenant_id` that is not a UUID: both are
    // "cannot scope", never "scope by something else".
    assert_eq!(tenant_field_index::<Unscoped>(), None);
    assert_eq!(tenant_field_index::<WronglyTyped>(), None);
}

/// The derived filter is only real if it reaches SQL: the discovered index
/// and the UUID comparison must narrow a live query.
#[tokio::test]
async fn derived_tenant_filter_scopes_a_live_query() {
    let mut db = toasty::Db::builder()
        .models(toasty::models!(Scoped))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    toasty::create!(Scoped {
        tenant_id: mine,
        name: "Mine"
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Scoped {
        tenant_id: theirs,
        name: "Theirs"
    })
    .exec(&mut db)
    .await
    .unwrap();

    let filter = derived_tenant_filter::<Scoped>(mine).expect("Scoped declares tenant_id");
    let rows = toasty::stmt::Query::<toasty::stmt::List<Scoped>>::all()
        .filter(filter)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Mine");

    // No discoverable column → no filter → the caller must fail rather than
    // run the query.
    assert!(derived_tenant_filter::<Unscoped>(mine).is_none());
}
