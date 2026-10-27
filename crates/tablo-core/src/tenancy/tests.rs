use topcoat::context::Cx;

use super::*;

#[test]
fn tenant_extension_wins_over_scoped_value_and_defaults_to_none() {
    let id = TenantId::from(uuid::Uuid::new_v4());
    let other = TenantId::from(uuid::Uuid::new_v4());

    // A scoped value app code put on the `Cx`.
    let cx = Cx::default().with(Tenant(id));
    assert_eq!(tenant_id(&cx), Some(id));

    // A server-set request extension overrides it deliberately.
    let cx = Cx::default()
        .with({
            let mut parts = http::Request::builder()
                .uri("/")
                .body(())
                .unwrap()
                .into_parts()
                .0;
            parts.extensions.insert(Tenant(id));
            parts
        })
        .with(Tenant(other));
    assert_eq!(tenant_id(&cx), Some(id));

    // Nothing set → None (callers must reject tenantless access).
    let cx = Cx::default();
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
