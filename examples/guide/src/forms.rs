//! The Forms chapter's snippets.

use std::path::PathBuf;

use tablo::{
    Options, Uploader,
    extend::{Control, ControlInput},
    prelude::*,
};
use topcoat::{context::Cx, view::*};

use crate::{
    models::{Post, Role, Seo, Theme, User},
    resources::{AuthorResource, UserForm},
};

// ANCHOR: forms-controls-layout
pub fn account_layout() -> Schema<UserForm> {
    let c = UserForm::controls();
    Schema::new((
        Section::new("Account").schema((c.email.email(), c.role)),
        Grid::new(2).schema((c.name, c.age)),
    ))
}
// ANCHOR_END: forms-controls-layout

// ANCHOR: forms-color-control
struct Color;

impl Control for Color {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
        let attrs = input.attributes(cx); // id, name, value, required, aria-*
        view! { cx => <input type="color" (attrs)> }.boxed()
    }
}

// A record form's text control takes it with `.custom`; a page's field with `Field::custom`.
pub fn accent_control() -> CustomField<ThemeForm> {
    ThemeForm::controls().accent.custom(Color)
}

pub fn accent_field() -> CustomField {
    Field::custom(lens!(Theme.accent), Color)
}
// ANCHOR_END: forms-color-control

#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Theme)]
pub struct ThemeForm {
    pub accent: String,
}

// ANCHOR: forms-relationship-field
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Post)]
pub struct PostAuthorForm {
    // The source whose scoped query loads the options; each is labelled by its `record_title`.
    #[form(relationship = AuthorResource)]
    pub author_id: uuid::Uuid,
}

pub fn author_control() -> ChoiceField<PostAuthorForm> {
    PostAuthorForm::controls()
        .author_id
        .searchable()
        .label("Author")
}
// ANCHOR_END: forms-relationship-field

// ANCHOR: forms-role-options
pub fn role_fields() {
    Field::choice(lens!(User.role)).options(Role::options());
    SelectFilter::of(lens!(User.role));
}
// ANCHOR_END: forms-role-options

// ANCHOR: forms-uploader
struct DirUploader {
    dir: PathBuf,
}

impl Uploader for DirUploader {
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String, String> {
        let name = format!("{}-{filename}", uuid::Uuid::new_v4());
        tokio::fs::write(self.dir.join(&name), bytes)
            .await
            .map_err(|_| "the upload could not be written".to_string())?;
        Ok(format!("/uploads/{name}")) // the value the record stores
    }
}

pub fn uploads_panel(dir: PathBuf) -> Panel {
    Panel::new("admin")
        .uploads(DirUploader { dir: dir.clone() })
        .serve_dir("/uploads/{*file}", dir)
}
// ANCHOR_END: forms-uploader

// ANCHOR: forms-embedded-schema
pub fn seo_schema() -> Section {
    Section::new("SEO").schema(Seo::form(Post::fields().seo()))
}
// ANCHOR_END: forms-embedded-schema

// ANCHOR: forms-embedded-record-form
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    #[form(embed)]
    pub seo: Seo,
}
// ANCHOR_END: forms-embedded-record-form
