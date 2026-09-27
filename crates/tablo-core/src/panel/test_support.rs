//! Fixtures shared by the panel's `#[cfg(test)]` modules.
//!
//! Every module's resource declares the same two-column model and the same
//! table over it, and mounts the same panel; one copy lives here.

use std::collections::HashMap;

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
pub(crate) fn dummy_table(cx: &Cx) -> Table<Dummy> {
    Table::<Dummy>::r#for(cx)
        .key(|d: &Dummy| d.id.to_string())
        .columns(TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
            d.name.clone()
        }))
}

/// A panel mounted at `/admin` with one resource and the auth gate off.
pub(crate) fn panel_for<R: Resource>(db: Db) -> Panel {
    Panel::new("admin")
        .app_context(db)
        .resource::<R>()
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

    fn table(cx: &Cx) -> crate::resource::Table<Tagged> {
        crate::resource::Table::r#for(cx)
            .key(|row: &Tagged| row.id.to_string())
            .columns(crate::resource::TextColumn::r#for(
                Tagged::fields().name(),
                |row: &Tagged| row.name.clone(),
            ))
    }

    fn form(_cx: &Cx) -> Schema {
        Schema::new((
            TextInput::r#for(Tagged::fields().name()),
            TextInput::typed::<Tagged, uuid::Uuid>(Tagged::fields().token()).unique(),
        ))
    }

    fn hydrate_form_values(_cx: &Cx, record: &Tagged) -> HashMap<String, String> {
        HashMap::from([
            ("name".to_string(), record.name.clone()),
            ("token".to_string(), record.token.to_string()),
        ])
    }

    async fn create_record(
        _cx: &Cx,
        values: HashMap<String, String>,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Tagged> {
        toasty::create!(Tagged {
            name: values.get("name").cloned().unwrap_or_default(),
            token: submitted_token(&values),
        })
        .exec(&mut *ex)
        .await
        .map_err(|error| -> topcoat::Error { error.into() })
    }

    async fn update_record(
        _cx: &Cx,
        mut record: Tagged,
        values: HashMap<String, String>,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Tagged> {
        if let Some(name) = values.get("name") {
            record.name = name.clone();
        }
        if values.contains_key("token") {
            record.token = submitted_token(&values);
        }
        toasty::update!(record {
            name: record.name.clone(),
            token: record.token,
        })
        .exec(&mut *ex)
        .await
        .map_err(|error| -> topcoat::Error { error.into() })?;
        Ok(record)
    }
}

/// The submitted token, or the nil UUID when it does not parse.
pub(crate) fn submitted_token(values: &HashMap<String, String>) -> uuid::Uuid {
    values
        .get("token")
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .unwrap_or(uuid::Uuid::nil())
}
