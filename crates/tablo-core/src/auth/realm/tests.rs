use super::*;

/// A realm at `prefix`.
fn realm(prefix: &str) -> Arc<Realm> {
    Arc::new(Realm {
        prefix: prefix.to_owned(),
        id: crate::PanelId::new(prefix),
        host: None,
        auth: crate::Auth::disabled().into_config(),
        login_hint: None,
        time_zone: jiff::tz::TimeZone::UTC,
        catalog: std::sync::Arc::new(
            crate::i18n::Catalog::new(std::slice::from_ref(&crate::i18n::ENGLISH), &[], &[])
                .expect("`en` builds"),
        ),
        notifications: false,
        serves_tenants: false,
        pages: SettingsPaths::under(prefix),
        auth_hooks: Vec::new(),
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

/// A request to `uri` carrying each of `hosts` as a `Host` header.
fn request(uri: &str, hosts: &[&str]) -> topcoat::context::Cx {
    let mut builder = http::Request::builder().uri(uri);
    for host in hosts {
        builder = builder.header(http::header::HOST, *host);
    }
    topcoat::context::Cx::default().with(builder.body(()).unwrap().into_parts().0)
}

/// The header and the URI's authority both name the host; the port, the case and a rooting dot
/// do not count.
#[test]
fn a_request_names_its_host_in_its_header_or_its_uri() {
    assert!(on_host(&request("/admin", &["admin.test"]), "admin.test"));
    assert!(on_host(
        &request("/admin", &["ADMIN.test.:8443"]),
        "admin.test"
    ));
    assert!(on_host(
        &request("https://admin.test/admin", &[]),
        "admin.test"
    ));
    assert!(on_host(&request("/admin", &["[::1]:3000"]), "[::1]"));
    assert!(!on_host(&request("/admin", &["app.test"]), "admin.test"));
    assert!(!on_host(&request("/admin", &["xadmin.test"]), "admin.test"));
}

/// A request that names no host, an unreadable one, or two that differ, is on no host: a
/// front end and the panel could otherwise each read a different one.
#[test]
fn a_request_naming_no_one_host_is_on_none() {
    assert!(!on_host(&request("/admin", &[]), "admin.test"));
    assert!(!on_host(
        &request("/admin", &["admin.test/x"]),
        "admin.test"
    ));
    assert!(!on_host(
        &request("/admin", &["admin.test", "app.test"]),
        "admin.test"
    ));
    assert!(!on_host(
        &request("https://app.test/admin", &["admin.test"]),
        "admin.test"
    ));
    assert!(!on_host(
        &request("https://admin.test/admin", &["app.test"]),
        "admin.test"
    ));
}

#[test]
fn a_declared_host_is_kept_lowercase_and_unrooted() {
    assert_eq!(
        declared_host("Admin.Example.com.").as_deref(),
        Some("admin.example.com")
    );
    assert_eq!(declared_host("admin.example.com:443"), None);
}
