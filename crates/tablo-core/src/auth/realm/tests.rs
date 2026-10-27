use super::*;

/// A realm at `prefix`.
fn realm(prefix: &str) -> Arc<Realm> {
    Arc::new(Realm {
        prefix: prefix.to_owned(),
        auth: crate::Auth::disabled().into_config(),
        login_hint: None,
        time_zone: jiff::tz::TimeZone::UTC,
        catalog: crate::i18n::Catalog::new(std::slice::from_ref(&crate::i18n::ENGLISH), &[])
            .expect("`en` builds"),
        notifications: false,
        serves_tenants: false,
    })
}

/// The request's realm names the prefix, whatever the path says; without one, the path's first
/// segment does.
#[test]
fn panel_prefix_prefers_the_realm_over_the_request_path() {
    use topcoat::context::Cx;

    let parts = || {
        http::Request::builder()
            .uri("/admin/users/42/edit")
            .body(())
            .unwrap()
            .into_parts()
            .0
    };
    let cx = Cx::default()
        .with(parts())
        .with(CurrentRealm(realm("/backoffice")));
    assert_eq!(panel_prefix(&cx), "/backoffice");

    let cx = Cx::default().with(parts());
    assert_eq!(panel_prefix(&cx), "/admin");
}
