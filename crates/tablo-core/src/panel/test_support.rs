//! Fixtures shared by the panel's `#[cfg(test)]` modules.
//!
//! Every module's resource declares the same two-column model and the same
//! table over it, and mounts the same panel; one copy lives here.

use toasty::Db;
use topcoat::{context::Cx, router::Body};

use crate::{
    Panel,
    resource::{Resource, Table, TextColumn},
    schema::{Schema, TextInput},
};

/// The two-column model a panel test's resource renders.
#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct Dummy {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

/// [`Dummy`]'s canonical table: display key, record key, one name column.
pub(crate) fn dummy_table(_cx: &Cx) -> Table<Dummy> {
    Table::<Dummy>::new(
        |d: &Dummy| d.id.to_string(),
        TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| d.name.clone()),
    )
}

/// A panel mounted at `/admin` with one list-only resource and the auth gate
/// off.
pub(crate) fn panel_for<R: Resource>(db: Db) -> Panel {
    Panel::new("admin")
        .app_context(db)
        .resource::<R>()
        .auth(crate::Auth::disabled())
}

/// [`panel_for`] for a resource with a form.
pub(crate) fn form_panel_for<R: crate::form::FormResource>(db: Db) -> Panel {
    Panel::new("admin")
        .app_context(db)
        .form_resource::<R>()
        .auth(crate::Auth::disabled())
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

    fn slug() -> String {
        "tagged".to_string()
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &Tagged) -> bool {
        true
    }

    fn can_create(_cx: &Cx) -> bool {
        true
    }

    fn can_update(_cx: &Cx, _record: &Tagged) -> bool {
        true
    }

    fn table(_cx: &Cx) -> crate::resource::Table<Tagged> {
        crate::resource::Table::new(
            |row: &Tagged| row.id.to_string(),
            crate::resource::TextColumn::r#for(Tagged::fields().name(), |row: &Tagged| {
                row.name.clone()
            }),
        )
    }
}

/// [`Tagged`]'s record form: both columns, written through the derived write.
#[derive(crate::RecordForm)]
#[record_form(model = Tagged)]
pub(crate) struct TaggedForm {
    pub(crate) name: String,
    pub(crate) token: uuid::Uuid,
}

impl crate::form::FormResource for TaggedResource {
    type Form = TaggedForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new((
            TextInput::r#for(Tagged::fields().name()),
            TextInput::typed::<Tagged, uuid::Uuid>(Tagged::fields().token()).unique(),
        ))
    }
}
