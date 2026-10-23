use super::*;

/// A realm at `prefix`.
fn realm(prefix: &str) -> Arc<Realm> {
    Arc::new(Realm {
        prefix: prefix.to_owned(),
        auth: crate::Auth::disabled(),
        login_hint: None,
        time_zone: jiff::tz::TimeZone::UTC,
        catalog: crate::i18n::Catalog::new(&["en".to_owned()], &[]).expect("`en` builds"),
        notifications: false,
    })
}

/// The request's realm names the prefix, whatever the path says; without one, the path's first
/// segment does.
#[test]
fn panel_prefix_prefers_the_realm_over_the_request_path() {
    use topcoat::context::CxTestBuilder;

    let parts = || {
        http::Request::builder()
            .uri("/admin/users/42/edit")
            .body(())
            .unwrap()
            .into_parts()
            .0
    };
    let cx = CxTestBuilder::new()
        .request_context(parts())
        .request_context(CurrentRealm(realm("/backoffice")))
        .build();
    assert_eq!(panel_prefix(&cx), "/backoffice");

    let cx = CxTestBuilder::new().request_context(parts()).build();
    assert_eq!(panel_prefix(&cx), "/admin");
}
