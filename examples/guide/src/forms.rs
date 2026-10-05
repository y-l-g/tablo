//! The Forms chapter's snippets.

use std::path::PathBuf;

use tablo::prelude::*;
use tablo_core::{Control, ControlInput, Options, Uploader};
use topcoat::{context::Cx, view::*};

use crate::{
    models::{Author, Post, Role, Seo, Theme, User},
    resources::{AuthorResource, UserForm},
};

// ANCHOR: forms-controls-layout
pub fn account_layout() -> Schema {
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

pub fn accent_field() -> CustomField {
    Field::custom(Theme::fields().accent(), Color)
}
// ANCHOR_END: forms-color-control

// ANCHOR: forms-relationship-field
pub fn author_field() -> ChoiceField {
    Field::choice(Post::fields().author_id())
        // The source, whose scoped query loads the options, and each option's label.
        .relationship::<AuthorResource>(|a: &Author| a.name.clone())
        .searchable()
        .label("Author")
}
// ANCHOR_END: forms-relationship-field

// ANCHOR: forms-role-options
pub fn role_fields() {
    Field::choice(User::fields().role()).options(Role::options());
    SelectFilter::new(User::fields().role(), Role::options());
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
#[derive(Debug, Clone, tablo_core::RecordForm)]
#[form(model = Post)]
pub struct PostForm {
    pub title: String,
    #[form(embed)]
    pub seo: Seo,
}
// ANCHOR_END: forms-embedded-record-form
