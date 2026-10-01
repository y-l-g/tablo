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

/// A user holding the memberships it carries.
struct Member(Vec<Membership>);

impl crate::auth::PanelUser for Member {
    fn user_id(&self) -> String {
        "member".to_string()
    }

    fn display_name(&self) -> &str {
        "Member"
    }

    fn tenants(&self) -> &[Membership] {
        &self.0
    }
}

/// A request on `/admin` signed in as a member of `tenants`, whose session
/// selected `selected`, with an optional `Tenant` override on the `Cx`.
fn member_cx(
    tenants: &[uuid::Uuid],
    selected: Option<uuid::Uuid>,
    overridden: Option<uuid::Uuid>,
) -> Cx {
    let panel = std::sync::Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        crate::Auth::password(),
    ));
    let memberships = tenants
        .iter()
        .enumerate()
        .map(|(index, tenant)| Membership::new(*tenant, format!("Tenant {index}")))
        .collect();
    let mut builder = CxTestBuilder::new()
        .request_context(crate::panel::state::CurrentPanel(std::sync::Arc::clone(
            &panel,
        )))
        .request_context(crate::auth::SignedIn {
            user: std::sync::Arc::new(Member(memberships)),
            panel,
            tenant: selected,
        });
    if let Some(tenant) = overridden {
        builder = builder.request_context(Tenant(tenant));
    }
    builder.build()
}

#[test]
fn the_request_acts_for_the_selected_membership_else_the_first() {
    let (a, b, stranger) = (
        uuid::Uuid::from_u128(1),
        uuid::Uuid::from_u128(2),
        uuid::Uuid::from_u128(3),
    );
    assert_eq!(tenant_id(&member_cx(&[a, b], None, None)), Some(a));
    assert_eq!(tenant_id(&member_cx(&[a, b], Some(b), None)), Some(b));
    let cx = member_cx(&[a, b], Some(b), None);
    assert_eq!(membership(&cx).map(|m| m.name.as_str()), Some("Tenant 1"));
    // A selection the user is no longer a member of falls back to the first.
    assert_eq!(
        tenant_id(&member_cx(&[a, b], Some(stranger), None)),
        Some(a)
    );
    // No membership, no tenant.
    assert_eq!(tenant_id(&member_cx(&[], Some(a), None)), None);
    // An override wins, and names no membership unless the user holds it.
    let cx = member_cx(&[a, b], None, Some(stranger));
    assert_eq!(tenant_id(&cx), Some(stranger));
    assert_eq!(membership(&cx), None);
    let cx = member_cx(&[a, b], None, Some(b));
    assert_eq!(membership(&cx).map(|m| m.tenant), Some(b));
}

#[test]
fn tenant_header_is_never_trusted() {
    // Spoofing another tenant's
    // UUID in `x-tenant-id` must not resolve a tenant.
    let id = uuid::Uuid::new_v4();
    assert_eq!(tenant_id(&cx_with_header(&id.to_string())), None);
}

/// A model with its own tenant column.
#[derive(Debug, Clone, toasty::Model)]
struct Scoped {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
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

/// The filter is only real if it reaches SQL: the lens and the UUID
/// comparison must narrow a live query.
#[tokio::test]
async fn tenancy_column_filter_scopes_a_live_query() {
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

    let filter = Tenancy::column(Scoped::fields().tenant_id())
        .filter(mine)
        .expect("a column tenancy filters");
    let rows = toasty::stmt::Query::<toasty::stmt::List<Scoped>>::all()
        .filter(filter)
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Mine");
}
