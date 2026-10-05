//! Fixtures shared by the panel's `#[cfg(test)]` modules.
//!
//! The models panel tests declare, the canonical name table, and the panel
//! mounting helpers; one copy lives here.

use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, RouterBuilderDiscoverExt},
};

use crate::{
    Ability, Panel, RouterBuilderPanelExt, lens,
    resource::{Resource, ResourceDef},
    schema::{Field, Schema},
    table::{Table, TextColumn},
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

/// The unique-email model form tests submit.
#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct Subscriber {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    #[unique]
    pub(crate) email: String,
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
        children: std::collections::HashMap::new(),
        mounts: std::sync::Arc::default(),
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

/// The mistakes a panel refused to mount with.
pub(crate) fn refusal(mounted: topcoat::Result<Router>) -> Vec<crate::DeclarationError> {
    let Err(error) = mounted else {
        panic!("the panel must not mount");
    };
    error
        .downcast_ref::<crate::MountError>()
        .unwrap_or_else(|| panic!("a declaration mistake refuses the panel, got {error}"))
        .errors()
        .to_vec()
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

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tagged")
            .policy(|_cx: &Cx, ability: Ability<'_, Tagged>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            })
            .table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Tagged.name),
            )))
            .form(Schema::new((
                Field::text(Tagged::fields().name()),
                Field::text(Tagged::fields().token()).unique(),
            )))
    }
}

/// [`Tagged`]'s record form: both columns, written through the derived write.
#[derive(crate::RecordForm)]
#[form(model = Tagged)]
pub(crate) struct TaggedForm {
    pub(crate) name: String,
    pub(crate) token: uuid::Uuid,
}
