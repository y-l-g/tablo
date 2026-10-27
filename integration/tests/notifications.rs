//! Stored notifications and the bell: who sees what, what its procedures may change, what the
//! mount refuses, and the live updates over Topcoat's runtime socket.

use std::{collections::BTreeSet, time::Duration};

use futures_util::{SinkExt, StreamExt};
use tablo::{
    Auth, DeclarationErrorKind, Notification, Panel, PanelHandle, PanelUser, RouterBuilderPanelExt,
    auth::{AuthSession, mint_session, revoke_sessions_for_user},
    notification::StoredNotification,
    testing::{Bell, SESSION_COOKIE, TestClient, bell, body_string, refusal},
};
use toasty::Db;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt};

use crate::common::{
    members::{Member, MemberAuth, NoteResource, Seat, SignedMember, member},
    test_db,
};

const MARK: &str = "/_topcoat/runtime/procedures/tablo-notifications/mark";
const REMOVE: &str = "/_topcoat/runtime/procedures/tablo-notifications/remove";
const READ_ALL: &str = "/_topcoat/runtime/procedures/tablo-notifications/read-all";
const CLEAR: &str = "/_topcoat/runtime/procedures/tablo-notifications/clear";
const BELL: &str = "/_topcoat/runtime/shards/tablo-notifications";

async fn db() -> Db {
    test_db(toasty::models!(
        Member,
        Seat,
        tablo::notification::StoredNotification,
        crate::common::members::Note,
        AuthSession
    ))
    .await
}

fn panel() -> Panel {
    Panel::new("admin")
        .auth(Auth::custom(MemberAuth))
        .resource::<NoteResource>()
}

/// The router serving the notes panel with its bell, and the panel's handle.
fn mount(db: Db, panel: Panel) -> (Router, PanelHandle) {
    let builder = Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)
        .expect("the panel mounts");
    let admin = builder.panel_handle("admin").expect("mounted above");
    (builder.build(), admin)
}

/// What `client` sees in the bell over the notes list.
async fn bell_of(client: &TestClient<'_>) -> Bell {
    let response = client.get("/admin/notes").await;
    assert_eq!(response.status(), 200);
    bell(&body_string(response).await).expect("the panel shows a bell")
}

fn titles(bell: &Bell) -> BTreeSet<String> {
    bell.notifications
        .iter()
        .map(|notification| notification.title.clone())
        .collect()
}

async fn send(admin: &PanelHandle, to: &SignedMember, notification: Notification) -> String {
    admin
        .notifications()
        .send(&to.user_id(), notification)
        .await
        .expect("the notification is stored")
        .id
        .to_string()
}

/// Each user's bell lists what was sent to them, and nothing sent to anyone else.
#[tokio::test]
async fn a_bell_lists_only_its_users_notifications() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    let bob = member(&db, "Bob", &["Acme"]).await;
    send(
        &admin,
        &ada,
        Notification::success("Export ready").action("Download", "/exports/1.csv"),
    )
    .await;
    send(&admin, &bob, Notification::info("For Bob")).await;

    let ada_bell = bell_of(&TestClient::new(&router).sign_in(&db, "admin", &ada).await).await;
    assert_eq!(ada_bell.unread, 1);
    assert_eq!(
        titles(&ada_bell),
        BTreeSet::from(["Export ready".to_owned()])
    );
    assert_eq!(
        ada_bell.notifications[0].actions,
        [("Download".to_owned(), "/exports/1.csv".to_owned())]
    );
    let bob_bell = bell_of(&TestClient::new(&router).sign_in(&db, "admin", &bob).await).await;
    assert_eq!(titles(&bob_bell), BTreeSet::from(["For Bob".to_owned()]));
}

/// The bell lists what was sent for the tenant the user acts for, and what was sent for none.
#[tokio::test]
async fn a_bell_lists_its_tenants_notifications_and_the_tenantless() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme", "Globex"]).await;
    let (acme, globex) = (ada.tenants[0].tenant, ada.tenants[1].tenant);
    let notifications = admin.notifications();
    let mut ids = Vec::new();
    for (tenant, title) in [
        (Some(acme), "Acme report"),
        (Some(globex), "Globex report"),
        (None, "Everywhere"),
    ] {
        let stored = notifications
            .clone()
            .tenant(tenant)
            .send(&ada.user_id(), Notification::info(title))
            .await
            .unwrap();
        ids.push(stored.id.to_string());
    }
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    // Acting for Acme, the Globex row's id names nothing to mark or remove.
    let globex_id = &ids[1];
    for (path, args) in [
        (MARK, format!(r#"["{globex_id}", true]"#)),
        (REMOVE, format!(r#"["{globex_id}"]"#)),
    ] {
        let response = client.tenant(acme).call_procedure(path, &args).await;
        assert_eq!(response.status(), 200, "{path}");
    }
    assert_eq!(bell_of(&client.tenant(globex)).await.unread, 2);

    let in_acme = bell_of(&client.tenant(acme)).await;
    assert_eq!(
        titles(&in_acme),
        BTreeSet::from(["Acme report".to_owned(), "Everywhere".to_owned()])
    );
    assert_eq!(in_acme.unread, 2);
    let in_globex = bell_of(&client.tenant(globex)).await;
    assert_eq!(
        titles(&in_globex),
        BTreeSet::from(["Everywhere".to_owned(), "Globex report".to_owned()])
    );

    // Clearing in one tenant leaves the other tenant's notifications.
    let cleared = client.tenant(acme).call_procedure(CLEAR, "[]").await;
    assert_eq!(cleared.status(), 200);
    assert_eq!(
        titles(&bell_of(&client.tenant(globex)).await),
        BTreeSet::from(["Globex report".to_owned()])
    );
}

/// Each panel's bell lists what was sent on it, and its procedures change nothing on another
/// panel the same user signs in to.
#[tokio::test]
async fn a_bell_lists_only_its_panels_notifications() {
    let db = db().await;
    let builder = Router::builder()
        .discover()
        .app_context(db.clone())
        .panel(panel().notifications())
        .expect("the admin panel mounts")
        .panel(
            Panel::new("portal")
                .auth(Auth::custom(MemberAuth))
                .resource::<NoteResource>()
                .notifications(),
        )
        .expect("the portal panel mounts");
    let admin = builder.panel_handle("admin").expect("mounted above");
    let portal = builder.panel_handle("portal").expect("mounted above");
    let router = builder.build();
    let ada = member(&db, "Ada", &["Acme"]).await;
    send(&admin, &ada, Notification::info("On admin")).await;
    let portals = send(&portal, &ada, Notification::info("On portal")).await;
    let on_admin = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let on_portal = TestClient::new(&router).sign_in(&db, "portal", &ada).await;

    assert_eq!(
        titles(&bell_of(&on_admin).await),
        BTreeSet::from(["On admin".to_owned()])
    );
    for (path, args) in [
        (MARK, format!(r#"["{portals}", true]"#)),
        (REMOVE, format!(r#"["{portals}"]"#)),
        (READ_ALL, "[]".to_owned()),
        (CLEAR, "[]".to_owned()),
    ] {
        let response = on_admin.call_procedure(path, &args).await;
        assert_eq!(response.status(), 200, "{path}");
    }
    let response = on_portal.get("/portal/notes").await;
    let portal_bell = bell(&body_string(response).await).expect("the portal shows a bell");
    assert_eq!(
        portal_bell.unread, 1,
        "the admin bell changed nothing on the portal"
    );
    assert_eq!(
        titles(&portal_bell),
        BTreeSet::from(["On portal".to_owned()])
    );
}

/// The bell's procedures mark and remove the user's own notifications, and change nothing of
/// anyone else's, whatever id they are sent.
#[tokio::test]
async fn the_bell_changes_only_its_users_notifications() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    let bob = member(&db, "Bob", &["Acme"]).await;
    let first = send(&admin, &ada, Notification::info("First")).await;
    let second = send(&admin, &ada, Notification::info("Second")).await;
    let bobs = send(&admin, &bob, Notification::info("Bob's")).await;
    let ada_client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let bob_client = TestClient::new(&router).sign_in(&db, "admin", &bob).await;

    for (path, args) in [
        (MARK, format!(r#"["{bobs}", true]"#)),
        (REMOVE, format!(r#"["{bobs}"]"#)),
        (READ_ALL, "[]".to_owned()),
    ] {
        let response = ada_client.call_procedure(path, &args).await;
        assert_eq!(response.status(), 200, "{path}");
    }
    let bob_bell = bell_of(&bob_client).await;
    assert_eq!(bob_bell.unread, 1, "another user's id changes nothing");
    assert_eq!(titles(&bob_bell), BTreeSet::from(["Bob's".to_owned()]));

    // Ada's read-all above marked both of hers read; one back to unread, then removed.
    let ada_bell = bell_of(&ada_client).await;
    assert_eq!(ada_bell.unread, 0);
    ada_client
        .call_procedure(MARK, &format!(r#"["{first}", false]"#))
        .await;
    let ada_bell = bell_of(&ada_client).await;
    assert_eq!(ada_bell.unread, 1);
    let unread: Vec<_> = ada_bell
        .notifications
        .iter()
        .filter(|notification| !notification.read)
        .map(|notification| notification.id.clone())
        .collect();
    assert_eq!(unread, std::slice::from_ref(&first));
    ada_client
        .call_procedure(REMOVE, &format!(r#"["{second}"]"#))
        .await;
    assert_eq!(
        titles(&bell_of(&ada_client).await),
        BTreeSet::from(["First".to_owned()])
    );

    let cleared = ada_client.call_procedure(CLEAR, "[]").await;
    assert_eq!(cleared.status(), 200);
    assert!(bell_of(&ada_client).await.notifications.is_empty());
    assert_eq!(titles(&bell_of(&bob_client).await).len(), 1);

    // An id that is no id names no notification.
    let malformed = ada_client
        .call_procedure(MARK, r#"["not-an-id", true]"#)
        .await;
    assert_eq!(malformed.status(), 404);
}

/// Without a session the bell and its procedures answer 401, never a redirect.
#[tokio::test]
async fn the_bell_refuses_anonymous_requests() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    let id = send(&admin, &ada, Notification::info("Private")).await;
    let anonymous = TestClient::new(&router);
    for (path, args) in [
        (MARK, format!(r#"["{id}", true]"#)),
        (REMOVE, format!(r#"["{id}"]"#)),
        (READ_ALL, "[]".to_owned()),
        (CLEAR, "[]".to_owned()),
    ] {
        let response = anonymous.call_procedure(path, &args).await;
        assert_eq!(response.status(), 401, "{path}");
    }
    let shard = http::Request::builder()
        .method(http::Method::POST)
        .uri(BELL)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"args":[],"signals":{}}"#))
        .unwrap();
    assert_eq!(router.handle(shard).await.status(), 401);
    let rows = StoredNotification::all()
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert!(rows[0].read_at.is_none(), "nothing changed");
}

/// A panel without `notifications()` shows no bell, and its procedures change nothing.
#[tokio::test]
async fn a_panel_without_notifications_shows_no_bell() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel());
    let ada = member(&db, "Ada", &["Acme"]).await;
    send(&admin, &ada, Notification::info("Unlisted")).await;
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let page = body_string(client.get("/admin/notes").await).await;
    assert!(bell(&page).is_none());
    assert_eq!(client.call_procedure(CLEAR, "[]").await.status(), 404);
    let rows = StoredNotification::all()
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

/// The mount refuses a bell nobody signs in to, and one whose model the `Db` lacks.
#[tokio::test]
async fn the_mount_refuses_a_bell_it_cannot_serve() {
    let db = db().await;
    let without_auth = Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("admin").auth(Auth::disabled()).notifications());
    assert!(
        refusal(without_auth)
            .iter()
            .any(|error| error.kind == DeclarationErrorKind::NotificationsWithoutAuth)
    );

    let bare = test_db(toasty::models!(Member, Seat, AuthSession)).await;
    let without_model = Router::builder().discover().app_context(bare).panel(
        Panel::new("admin")
            .auth(Auth::custom(MemberAuth))
            .notifications(),
    );
    assert!(refusal(without_model).iter().any(|error| matches!(
        &error.kind,
        DeclarationErrorKind::MissingAuthModels { models } if models.contains(&"StoredNotification")
    )));
}

/// A link to any other scheme than `http` or `https` renders no link.
#[tokio::test]
async fn a_notification_links_only_to_web_urls() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    send(
        &admin,
        &ada,
        Notification::warning("Links")
            .action("Run", "javascript:alert(1)")
            .action("Data", "data:text/html,<script>alert(1)</script>")
            .action("Docs", "https://example.com/docs")
            .action("Order", "/admin/notes"),
    )
    .await;
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let bell = bell_of(&client).await;
    assert_eq!(
        bell.notifications[0].actions,
        [
            ("Docs".to_owned(), "https://example.com/docs".to_owned()),
            ("Order".to_owned(), "/admin/notes".to_owned()),
        ]
    );
}

/// A bell open over the runtime socket shows a notification sent after the page loaded, and
/// stops once the session is revoked: nothing sent after that reaches the tab.
#[tokio::test]
async fn an_open_bell_updates_until_its_session_ends() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    let token = mint_session(&mut db.clone(), "admin", &ada).await.unwrap();
    let OpenBell {
        mut socket,
        stop,
        server,
    } = open_bell(router, &token).await;
    // The connected render's first emission: nothing to show yet.
    assert!(frames_until(&mut socket, "No notifications").await);

    send(&admin, &ada, Notification::success("Export ready")).await;
    assert!(
        frames_until(&mut socket, "Export ready").await,
        "the open bell shows what was sent"
    );

    let cx = admin.enter(&admin.context());
    revoke_sessions_for_user(&cx, &ada.user_id()).await.unwrap();
    send(&admin, &ada, Notification::error("After revoke")).await;
    assert!(
        !frames_until(&mut socket, "After revoke").await,
        "a revoked session's bell shows nothing new"
    );

    stop.send(()).ok();
    drop(socket);
    let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
}

/// An open bell toasts a notification that arrives, and not an older one marked unread again.
#[tokio::test]
async fn an_open_bell_toasts_only_what_arrives() {
    let db = db().await;
    let (router, admin) = mount(db.clone(), panel().notifications());
    let ada = member(&db, "Ada", &["Acme"]).await;
    let old = send(&admin, &ada, Notification::info("Old")).await;
    let token = mint_session(&mut db.clone(), "admin", &ada).await.unwrap();
    let client = TestClient::new(&router).cookie(SESSION_COOKIE, &token);
    client
        .call_procedure(MARK, &format!(r#"["{old}", true]"#))
        .await;
    let OpenBell {
        mut socket,
        stop,
        server,
    } = open_bell(router.clone(), &token).await;
    assert!(frames_until(&mut socket, "Old").await);

    client
        .call_procedure(MARK, &format!(r#"["{old}", false]"#))
        .await;
    let badge = frame_with(&mut socket, r#"data-unread="1""#)
        .await
        .expect("the badge counts the unread one");
    assert!(
        !badge.contains("tablo-arrival-"),
        "an old notification marked unread is no arrival"
    );

    let fresh = send(&admin, &ada, Notification::success("Fresh")).await;
    assert!(
        frames_until(&mut socket, &format!("tablo-arrival-{fresh}")).await,
        "what arrives toasts"
    );

    stop.send(()).ok();
    drop(socket);
    let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A served router with one tab's runtime socket rendering the bell.
struct OpenBell {
    socket: Socket,
    stop: tokio::sync::oneshot::Sender<()>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

/// Serves `router` and starts the bell's run over the runtime socket, as a browser does once the
/// page loaded, signed in with the session `token`.
async fn open_bell(router: Router, token: &str) -> OpenBell {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(topcoat::serve_until(listener, router, async {
        stopped.await.ok();
    }));

    let mut request = format!("ws://{addr}/admin/notes")
        .into_client_request()
        .unwrap();
    let headers = request.headers_mut();
    headers.insert(
        http::header::SEC_WEBSOCKET_PROTOCOL,
        http::HeaderValue::from_static("topcoat-runtime"),
    );
    headers.insert(
        http::header::COOKIE,
        format!("{SESSION_COOKIE}={token}").parse().unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let run = serde_json::json!({
        "run": 1,
        "method": "POST",
        "path": BELL,
        "headers": { "content-type": "application/json" },
        "body": r#"{"args":[],"signals":{}}"#,
    });
    socket.send(Message::text(run.to_string())).await.unwrap();
    OpenBell {
        socket,
        stop,
        server,
    }
}

/// Reads the socket until a frame carries `needle`, for up to two seconds.
async fn frames_until(socket: &mut Socket, needle: &str) -> bool {
    frame_with(socket, needle).await.is_some()
}

/// The first frame whose HTML carries `needle`, read for up to two seconds.
async fn frame_with(socket: &mut Socket, needle: &str) -> Option<String> {
    let read = async {
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Text(text) = message {
                // A frame is JSON: the quotes of its HTML arrive escaped.
                let html = text.replace("\\\"", "\"");
                if html.contains(needle) {
                    return Some(html);
                }
            }
        }
        None
    };
    tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .ok()
        .flatten()
}
