//! Fixtures shared by the panel's `#[cfg(test)]` modules.
//!
//! The models panel tests declare, the canonical name table, and the panel
//! mounting helpers; one copy lives here.

use toasty::Db;
use topcoat::router::{Router, RouterBuilderDiscoverExt};

use crate::{Panel, RouterBuilderPanelExt, Table, TextColumn, lens, resource::Resource};

/// The two-column model a panel test's resource renders.
#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
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
#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
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
        realm: std::sync::Arc::new(crate::auth::Realm {
            prefix: prefix.to_string(),
            auth,
            login_hint: None,
            time_zone: jiff::tz::TimeZone::UTC,
            catalog: crate::i18n::Catalog::new(&["en".to_owned()], &[]).expect("`en` builds"),
            notifications: false,
        }),
        nav_items: Vec::new(),
        brand: None,
        dark_mode: false,
        shell_assets: None,
        children: std::collections::HashMap::new(),
        mounts: std::sync::Arc::default(),
        root_redirect: None,
        user_menu: Vec::new(),
        mounted_pages: std::collections::HashMap::new(),
        uploads: None,
        served_paths: Vec::new(),
        urls: std::collections::HashMap::new(),
        endpoints: Vec::new(),
        searches: Vec::new(),
    }
}

/// `builder` as a request to `panel` on a router that mounts it: the panel and its realm, and the
/// host auth renders through.
pub(crate) fn serving(
    builder: topcoat::context::CxTestBuilder,
    panel: &std::sync::Arc<super::state::PanelState>,
) -> topcoat::context::CxTestBuilder {
    builder
        .app_context(crate::auth::AuthHost(&super::host::PanelHost))
        .request_context(super::state::CurrentPanel(std::sync::Arc::clone(panel)))
        .request_context(crate::auth::CurrentRealm(std::sync::Arc::clone(
            &panel.realm,
        )))
}

/// `panel` mounted on a router holding `db`, the way an app mounts one.
pub(crate) fn mount(db: Db, panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)?
        .build())
}

/// The mistakes a panel was refused with.
pub(crate) fn refusal<T>(mounted: topcoat::Result<T>) -> Vec<crate::DeclarationError> {
    let Err(error) = mounted else {
        panic!("the panel must not build");
    };
    error
        .downcast_ref::<crate::MountError>()
        .unwrap_or_else(|| panic!("a declaration mistake refuses the panel, got {error}"))
        .errors()
        .to_vec()
}

/// Seed `rows` [`Dummy`] rows in a single batched insert.
///
/// The export chunker tests seed a chunk's worth of rows, more than a per-row
/// `toasty::create!` loop can afford, so `create_many` accumulates the inserts into one
/// statement. `name` maps a row index to its label.
pub(crate) async fn seed_dummies(db: &mut Db, rows: usize, name: impl Fn(usize) -> String) {
    let mut create = Dummy::create_many();
    for i in 0..rows {
        create = create.item(Dummy::create().name(name(i)));
    }
    create.exec(&mut *db).await.unwrap();
}
