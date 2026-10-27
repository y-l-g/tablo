//! A page's widget grid: who sees it, and how its widgets are named.

use tablo::{
    Auth, DeclarationErrorKind, Page, PageDef, Panel, RouterBuilderPanelExt, Widget, WidgetDef,
    auth::{AdminUser, AuthSession},
    testing::widgets,
};
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, RouteFn, RouteFuture, Router, RouterBuilderDiscoverExt, response::AsyncIntoResponse,
    },
    view::{View, ViewExt, view},
};

use crate::common::{body_string, get, mount, refusal, test_db};

/// A widget whose body would be the panel's to guard.
struct Secret;

impl Widget for Secret {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <p>"secret"</p> })
    }
}

struct Dashboard;

impl Page for Dashboard {
    fn declare() -> PageDef {
        PageDef::new().widget::<Secret>()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <main>(tablo::widgets::<Self>(cx))</main> })
    }
}

/// A public route that enters the panel and renders its dashboard's grid.
fn public_grid(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let entered = topcoat::context::context::<tablo::PanelHandle>(cx).enter(cx);
        view! { entered => <main>(tablo::widgets::<Dashboard>(&entered))</main> }
            .boxed()
            .async_into_response(cx)
            .await
    })
}

#[tokio::test]
async fn a_route_outside_the_prefix_renders_no_widget_to_a_visitor_the_panel_would_sign_in() {
    let builder = Router::builder()
        .discover()
        .with(test_db(toasty::models!(AdminUser, AuthSession)).await)
        .route(RouteFn::new(http::Method::GET, "/public", public_grid))
        .panel(
            Panel::new("admin")
                .auth(Auth::password())
                .page::<Dashboard>(),
        )
        .expect("the panel mounts");
    let admin = builder.panel_handle("admin").expect("mounted above");
    let router = builder.with(admin).build();

    let response = get(&router, "/public").await;
    assert_eq!(response.status(), 200);
    assert_eq!(widgets(&body_string(response).await), Vec::<String>::new());
}

/// Counts what `T` names: a widget generic over what it reads.
struct Count<T>(std::marker::PhantomData<T>);

struct Books;
struct Authors;

impl<T: Send + Sync + 'static> Widget for Count<T> {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <p>"count"</p> })
    }
}

/// A page mounting [`Count`] twice, named once per mount when `named`.
struct Counts<const NAMED: bool>;

impl<const NAMED: bool> Page for Counts<NAMED> {
    fn declare() -> PageDef {
        if NAMED {
            PageDef::new()
                .widget::<Count<Books>>()
                .widget::<NamedAuthors>()
        } else {
            PageDef::new()
                .widget::<Count<Books>>()
                .widget::<Count<Authors>>()
        }
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <main>(tablo::widgets::<Self>(cx))</main> })
    }
}

/// `Count<Authors>` under a name of its own.
struct NamedAuthors;

impl Widget for NamedAuthors {
    fn declare() -> WidgetDef {
        WidgetDef::new().name("authors")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Count::<Authors>::render(cx).await
    }
}

#[tokio::test]
async fn two_widgets_sharing_a_name_fail_the_mount() {
    let db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .page::<Counts<false>>(),
    ));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::DuplicateWidget {
            name: "count".to_string(),
        }
    );
}

#[tokio::test]
async fn a_widget_is_named_after_its_type_without_generic_arguments() {
    let db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let router = mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .page_with::<Counts<true>>(|def| def.slug("counts")),
    )
    .expect("the panel mounts");
    let page = body_string(get(&router, "/admin/counts").await).await;
    assert_eq!(widgets(&page), ["count", "authors"]);
}
