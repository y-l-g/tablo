use topcoat::context::CxTestBuilder;

use super::*;

#[test]
fn tenant_extension_wins_over_scoped_value_and_defaults_to_none() {
    let id = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();

    // A scoped value app code put on the `Cx`.
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

/// A model with its own tenant column.
#[derive(Debug, Clone, toasty::Model)]
struct Scoped {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    name: String,
}

#[test]
fn tenancy_none_is_unscoped_and_filters_nothing() {
    let tenancy = Tenancy::<Scoped>::none();
    assert!(!tenancy.is_scoped());
    assert!(tenancy.filter(uuid::Uuid::new_v4()).is_none());
    assert!(tenancy.column_field().is_none());
}

#[test]
fn tenancy_column_binds_the_lens_field() {
    let tenancy = Tenancy::column(Scoped::fields().tenant_id());
    assert!(tenancy.is_scoped());
    let field = tenancy
        .column_field()
        .expect("a column tenancy names a column")
        .expect("the lens is one field of the model");
    // `id` is index 0, `tenant_id` index 1.
    assert_eq!(field.index, 1);
    assert_eq!(field.name, "tenant_id");
}
