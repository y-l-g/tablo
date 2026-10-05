//! Fixtures shared by the panel's `#[cfg(test)]` modules.
//!
//! Every module's resource declares the same two-column model and the same
//! table over it, and mounts the same panel; one copy lives here.

use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, RouterBuilderDiscoverExt},
};

use crate::{
    Ability, Panel, Policy, RouterBuilderPanelExt, lens,
    resource::{Resource, Table, TextColumn},
    schema::{Field, Schema},
};

/// The two-column model a panel test's resource renders.
#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct Dummy {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

/// [`Dummy`]'s canonical table: one name column.
pub(crate) fn dummy_table() -> Table<Dummy> {
    Table::<Dummy>::new(TextColumn::new(lens!(Dummy.name)))
}

/// A panel at `/admin` with one resource and the auth gate off.
pub(crate) fn panel_for<R: Resource>() -> Panel {
    Panel::new("admin")
        .resource::<R>()
        .auth(crate::Auth::disabled())
}

/// A panel at `prefix` guarded by `auth`, with no resources, pages or shell
/// settings: the request's panel a handler test puts on its `Cx`.
pub(crate) fn panel_state(prefix: &str, auth: crate::Auth) -> super::state::PanelState {
    super::state::PanelState {
        prefix: prefix.to_string(),
        nav_items: Vec::new(),
        brand: None,
        dark_mode: false,
        shell_assets: None,
        search: std::collections::HashMap::new(),
        relations: std::collections::HashMap::new(),
        root_redirect: None,
        auth,
        login_hint: None,
        uploads: None,
        served_paths: Vec::new(),
        urls: std::collections::HashMap::new(),
    }
}

/// `state` as the request's panel, for a `CxTestBuilder::request_context`.
pub(crate) fn current_panel(state: super::state::PanelState) -> super::state::CurrentPanel {
    super::state::CurrentPanel(std::sync::Arc::new(state))
}

/// `panel` mounted on a router that holds no `Db`: what a panel refuses
/// before it needs one.
pub(crate) fn mount_without_db(panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder().discover().panel(panel)?.build())
}

/// `panel` mounted on a router holding `db`, the way an app mounts one.
pub(crate) fn mount(db: Db, panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)?
        .build())
}

/// The body of `response`, for an inline-error assertion.
pub(crate) async fn response_html(response: http::Response<Body>) -> String {
    String::from_utf8_lossy(
        &http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .to_string()
}

/// Seed `rows` [`Dummy`] rows in a single batched insert.
///
/// The export-cap tests seed more rows than a per-row `toasty::create!`
/// loop can afford, so `create_many` accumulates the inserts into one
/// statement. `name` maps a row index to its label.
pub(crate) async fn seed_dummies(db: &mut Db, rows: usize, name: impl Fn(usize) -> String) {
    let mut create = Dummy::create_many();
    for i in 0..rows {
        create = create.item(Dummy::create().name(name(i)));
    }
    create.exec(&mut *db).await.unwrap();
}

/// The typed unique field the unique-probe tests share. The column
/// is a `Uuid`, not a whole number: SQLite's INTEGER affinity coerces `01`
/// to `1`, so a whole-number column lets a text probe pass.
#[derive(Debug, toasty::Model, Clone)]
pub(crate) struct Tagged {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    #[unique]
    pub(crate) token: uuid::Uuid,
}

pub(crate) struct TaggedResource;

impl crate::resource::Resource for TaggedResource {
    type Model = Tagged;
    type Form = TaggedForm;

    fn form() -> Schema {
        Schema::new((
            Field::text(Tagged::fields().name()),
            Field::text(Tagged::fields().token()).unique(),
        ))
    }

    fn slug() -> String {
        "tagged".to_string()
    }

    fn policy() -> impl Policy<Tagged> {
        |_cx: &Cx, ability: Ability<'_, Tagged>| {
            matches!(
                ability,
                Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
            )
        }
    }

    fn table() -> crate::resource::Table<Tagged> {
        crate::resource::Table::new(crate::resource::TextColumn::new(lens!(Tagged.name)))
    }
}

/// [`Tagged`]'s record form: both columns, written through the derived write.
#[derive(crate::RecordForm)]
#[form(model = Tagged)]
pub(crate) struct TaggedForm {
    pub(crate) name: String,
    pub(crate) token: uuid::Uuid,
}
