//! The Panel and routing chapter's snippets.

use tablo::{Page, auth::Authenticator, prelude::*};
use toasty::Db;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::{Font, fontsource::fontsource_font},
    router::{Router, RouterBuilderDiscoverExt, Slot, layout, page},
    tailwind,
    view::*,
};

use crate::resources::{OrderResource, PostResource, UserResource};

// ANCHOR: panel-portal-auth
pub struct Customers;

impl Authenticator for Customers {
    type User = Customer;

    async fn verify(&self, _cx: &Cx, _login: &str, _password: &str) -> Result<Option<Customer>> {
        todo!()
    }

    async fn find_by_id(&self, _cx: &Cx, _id: &str) -> Result<Option<Customer>> {
        todo!()
    }
}

pub struct Customer {
    pub id: String,
}

impl tablo::PanelUser for Customer {
    fn user_id(&self) -> String {
        self.id.clone()
    }

    fn display_name(&self) -> &str {
        &self.id
    }
}
// ANCHOR_END: panel-portal-auth

// ANCHOR: panel-admin-router-body
pub fn admin_router(db: Db) -> topcoat::Result<Router> {
    let router = Router::builder()
        .discover() // the app's own pages and routes, and Tablo's
        .app_context(db) // the toasty::Db every handler uses
        .panel(
            Panel::new("admin") // mounted at /admin
                .brand(Brand::new("Acme"))
                .home::<Dashboard>() // GET /admin
                .resource::<UserResource>() // /admin/users and its sub-routes
                .page::<ReportsPage>(), // GET /admin/reports
        )?
        .build();
    Ok(router)
}
// ANCHOR_END: panel-admin-router-body

// ANCHOR: panel-two-panels-body
pub fn two_panels(db: Db) -> topcoat::Result<Router> {
    let router = Router::builder()
        .discover()
        .app_context(db)
        .panel(
            Panel::new("admin")
                .resource::<UserResource>()
                .resource::<OrderResource>(),
        )?
        .panel(
            Panel::new("portal")
                .brand(Brand::new("Customer portal"))
                .auth(Auth::custom(Customers))
                .resource::<OrderResource>(),
        )?
        .build();
    Ok(router)
}
// ANCHOR_END: panel-two-panels-body

// ANCHOR: panel-urls-body
pub fn panel_urls(cx: &Cx) {
    let _ = tablo::url::resource::<PostResource>(cx); // Some("/admin/posts")
    let _ = tablo::url::page::<ReportsPage>(cx); // Some("/admin/reports")
    let _ = tablo::url::panel(cx); // Some("/admin")
}
// ANCHOR_END: panel-urls-body

// ANCHOR: panel-custom-layout
pub fn admin_layout<'a>(cx: &'a Cx, slot: Slot<'a>) -> BoxView<'a> {
    let page = view! { cx => <div class="acme-admin">(slot)</div> }.boxed();
    Panel::layout_shell(cx, page.into())
}

pub fn shell_panel() -> Panel {
    Panel::new("admin").layout(admin_layout)
}
// ANCHOR_END: panel-custom-layout

// ANCHOR: panel-reports-page
pub struct ReportsPage;

impl Page for ReportsPage {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! {
            cx =>
            tablo::ui::page(
                tablo::ui::page_header(tablo::ui::page_title("Reports"))
                tablo::ui::page_content(tablo::ui::card(tablo::ui::card_content("…")))
            )
        })
    }
}

pub fn reports_panel() -> Panel {
    Panel::new("admin").page::<ReportsPage>() // GET /admin/reports
}
// ANCHOR_END: panel-reports-page

// ANCHOR: panel-public-page
pub struct Dashboard;

impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "Dashboard" })
    }
}

pub struct MediaLibraryPage;

impl Page for MediaLibraryPage {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "Media library" })
    }
}
// ANCHOR_END: panel-public-page

// ANCHOR: panel-blog
// A layout wraps every route under its path: this one wraps /blog and /blog/{id}.
// A layout at "/" would wrap /admin too.
#[layout("/blog")]
pub async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::document(
        cx,
        "Blog",
        view! { <div class="mx-auto max-w-3xl px-6 py-10">(slot)</div> },
    )
    .await
}

#[page("/blog")]
pub async fn blog() -> Result<impl View> {
    Ok(view! { "Posts" })
}
// ANCHOR_END: panel-blog

// ANCHOR: panel-brand-body
pub fn branded_panel() -> Panel {
    Panel::new("admin")
        .brand(Brand::new("Acme").logo("/logo.svg"))
        .dark_mode(true)
        .login_hint("Demo: admin@example.com / password")
}
// ANCHOR_END: panel-brand-body

// ANCHOR: panel-geist
pub const GEIST: Font = fontsource_font!(GEIST, host: Asset);
// ANCHOR_END: panel-geist

// ANCHOR: panel-assets-body
pub fn asset_panel(db: Db, bundle: AssetBundle) -> Panel {
    let _router = Router::builder().discover().app_context(db).assets(bundle);
    Panel::new("admin").shell_assets(tailwind::stylesheet!(), GEIST)
}
// ANCHOR_END: panel-assets-body
