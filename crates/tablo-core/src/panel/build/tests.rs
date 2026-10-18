use super::*;
use crate::{
    Ability, ResourceDef, Site,
    panel::test_support::{
        Dummy, current_panel, dummy_table, mount, panel_for, panel_state, refusal,
    },
    test_support::tableless_db,
};

/// A served directory's path is a route pattern ending in a
/// catch-all, and only that; everything else is a build error.
#[test]
fn serve_dir_accepts_only_a_catch_all_pattern() {
    assert!(is_directory_pattern("/uploads/{*file}"));
    assert!(is_directory_pattern("/{*file}"));
    assert!(is_directory_pattern("/up loads/{*file}"));
    assert!(!is_directory_pattern("/uploads"));
    assert!(!is_directory_pattern("/uploads/"));
    assert!(!is_directory_pattern("/uploads/{file}"));
    assert!(!is_directory_pattern("/uploads/{*}"));
    assert!(!is_directory_pattern("/{*file}/more"));
    assert!(!is_directory_pattern("/uploads/{*file"));
    assert!(!is_directory_pattern("/uploads//{*file}"));
    assert!(!is_directory_pattern("/uploads/{*fi-le}"));
}

/// A detail column whose path binds no single field.
struct Unbindable;

impl crate::table::Column<Dummy> for Unbindable {
    fn name(&self) -> &str {
        "unbindable"
    }

    fn label(&self) -> &str {
        "Unbindable"
    }

    fn text(&self, _cx: &Cx, _row: &Dummy) -> String {
        String::new()
    }

    fn misdeclared(&self) -> Option<DeclarationErrorKind> {
        Some(DeclarationErrorKind::TraversalLens { steps: 2 })
    }
}

/// A detail column whose path binds no single field refuses the mount, which only a column
/// implementing `misdeclared` itself can cause.
#[tokio::test]
async fn a_detail_column_binding_no_field_refuses_the_mount() {
    struct UnbindableView;
    impl crate::resource::Resource for UnbindableView {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .table(dummy_table())
                .detail(crate::Detail::empty().column(Unbindable))
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    assert_eq!(
        refusal(mount(db, panel_for::<UnbindableView>())),
        [DeclarationError::of::<UnbindableView>(
            Site::Detail,
            DeclarationErrorKind::TraversalLens { steps: 2 },
        )]
    );
}

/// Re-checks auth before reading the root target.
#[tokio::test]
async fn panel_root_redirect_rechecks_auth_before_the_root_target() {
    use topcoat::{context::CxTestBuilder, router::response::IntoResponse};

    // Enforced auth, no resolved user: the handler itself redirects to
    // login — and never reaches the root target read.
    let (parts, ()) = http::Request::builder()
        .uri("/admin")
        .body(())
        .unwrap()
        .into_parts();
    let mut gated = panel_state("/admin", crate::Auth::password());
    gated.root_redirect = Some("/admin/users".to_string());
    let gated = current_panel(gated);
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(gated.clone())
        .build();
    let err = match panel_root_redirect(&cx, Body::empty()).await {
        Ok(_) => panic!("unauthenticated root must not read the root target"),
        Err(err) => err,
    };
    let location = err
        .into_response(&cx)
        .expect("gate redirect renders")
        .headers()
        .get(http::header::LOCATION)
        .expect("login redirect carries a location")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.starts_with("/admin/login"),
        "unauthenticated root must redirect to login, got {location}"
    );

    // A resolved user passes the re-check and lands on the first resource.
    let user = crate::auth::SignedIn {
        user: Arc::new(crate::auth::AdminUser {
            id: uuid::Uuid::nil(),
            email: "ada@example.com".to_string(),
            password_hash: String::new(),
            display_name: "Ada".to_string(),
            active: true,
            created_at: jiff::Timestamp::UNIX_EPOCH,
        }),
        panel: Arc::clone(&gated.0),
        tenant: None,
    };
    let (parts, ()) = http::Request::builder()
        .uri("/admin")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(gated)
        .request_context(user)
        .build();
    let err = match panel_root_redirect(&cx, Body::empty()).await {
        Ok(_) => panic!("the redirect is an Err response"),
        Err(err) => err,
    };
    let location = err
        .into_response(&cx)
        .expect("root redirect renders")
        .headers()
        .get(http::header::LOCATION)
        .expect("root redirect carries a location")
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(location, "/admin/users");
}

/// A resource no panel in the context mounts has no def: its policy allows nothing and its
/// scoped query is refused, whether the context holds another panel or none at all.
#[tokio::test]
async fn an_unmounted_resource_is_refused_not_rebuilt() {
    use crate::resource::Resource;

    struct UnmountedResource;
    impl Resource for UnmountedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().policy(crate::Allow).table(dummy_table())
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    let bare = topcoat::context::CxTestBuilder::new()
        .app_context(db.clone())
        .build();
    let panel = Panel::new("admin")
        .handle(&db)
        .expect("panel builds")
        .context();
    for cx in [bare, panel] {
        assert!(!crate::can::<UnmountedResource>(&cx, Ability::ViewAny));
        let refused = crate::scoped_query::<UnmountedResource>(&cx).expect_err("not mounted");
        assert!(refused.to_string().contains("not mounted"), "{refused}");
    }
}

/// A mounted panel's handle builds each context from the def the mount declared: the resource
/// answers as mounted, and `declare` does not run again.
#[tokio::test]
async fn a_panel_handle_answers_from_the_mounted_def() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::resource::Resource;

    static DECLARE_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct HandledResource;
    impl Resource for HandledResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            DECLARE_CALLS.fetch_add(1, Ordering::SeqCst);
            ResourceDef::new()
                .policy(crate::ReadOnly)
                .table(dummy_table())
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    let builder = topcoat::router::Router::builder()
        .app_context(db)
        .panel(panel_for::<HandledResource>())
        .expect("panel builds");
    assert!(builder.panel_handle("portal").is_none(), "no panel there");
    let handle = builder
        .panel_handle("/admin/")
        .expect("the panel at /admin");
    for cx in [handle.context(), handle.clone().context()] {
        assert!(crate::can::<HandledResource>(&cx, Ability::ViewAny));
        assert!(!crate::can::<HandledResource>(&cx, Ability::Create));
    }
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);
}

/// [`Panel::handle`] refuses a misdeclared resource with the errors a router mount reports.
#[tokio::test]
async fn handle_refuses_a_misdeclared_resource() {
    use crate::resource::Resource;

    struct BadView;
    impl Resource for BadView {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .table(dummy_table())
                .detail(crate::Detail::empty().column(Unbindable))
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    assert_eq!(
        refusal(panel_for::<BadView>().handle(&db)),
        refusal(mount(db, panel_for::<BadView>()))
    );
}
