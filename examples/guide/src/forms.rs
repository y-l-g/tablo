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

// ANCHOR: forms-conditions
#[derive(tablo::RecordForm)]
#[form(model = User)]
pub struct AccessForm {
    #[form(options, blank = Role::Member)]
    pub role: Role,
    pub sso_managed: bool,
}

pub fn access_layout() -> Schema<AccessForm> {
    let c = AccessForm::controls();
    let sso = c.sso_managed.visible_when(&c.role, ["admin"]);
    Schema::new(Section::new("Access").schema((c.role, sso)))
}
// ANCHOR_END: forms-conditions

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

#[derive(Debug, Clone, toasty::Model)]
pub struct Country {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct City {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub country_id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Address {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub country_id: uuid::Uuid,
    pub city_id: uuid::Uuid,
}

pub struct CountryResource;

impl Resource for CountryResource {
    type Model = Country;
    type Form = NoForm<Country>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Country.name))))
            .record_title(lens!(Country.name))
    }
}

pub struct CityResource;

impl Resource for CityResource {
    type Model = City;
    type Form = NoForm<City>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(City.name))))
            .record_title(lens!(City.name))
    }
}

// ANCHOR: forms-dependent-choice
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Address)]
pub struct AddressForm {
    #[form(relationship = CountryResource)]
    pub country_id: uuid::Uuid,
    #[form(relationship = CityResource)]
    pub city_id: uuid::Uuid,
}

pub fn address_layout() -> Schema<AddressForm> {
    let c = AddressForm::controls();
    // The cities whose `country_id` is the chosen country.
    let city = c
        .city_id
        .depends_on(&c.country_id, City::fields().country_id());
    Schema::new((c.country_id, city))
}
// ANCHOR_END: forms-dependent-choice

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

// ANCHOR: forms-repeater
#[derive(Debug, Clone, toasty::Embed, tablo::RepeaterItem)]
pub struct Step {
    #[form(multiline = 2)]
    pub instruction: String,
    #[form(blank = 0)]
    pub minutes: i64,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Recipe {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
    // Stored in one column.
    #[document]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Recipe)]
pub struct RecipeForm {
    pub title: String,
    #[form(repeat)]
    pub steps: Vec<Step>,
}
// ANCHOR_END: forms-repeater

// ANCHOR: forms-repeater-layout
pub fn recipe_form() -> Schema<RecipeForm> {
    let c = RecipeForm::controls();
    Schema::new((c.title, c.steps.label("Method").add_label("Add step")))
}

pub fn recipe_view() -> Detail<Recipe> {
    Detail::new(RepeaterColumn::new(lens!(Recipe.steps)).label("Method"))
}
// ANCHOR_END: forms-repeater-layout

// ANCHOR: forms-many-to-many-models
#[derive(Debug, Clone, toasty::Model)]
pub struct Article {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
    #[has_many]
    pub taggings: toasty::Deferred<Vec<Tagging>>,
    // The tags the join rows reach.
    #[has_many(via = taggings.tag)]
    pub tags: toasty::Deferred<Vec<Tag>>,
}

#[derive(Debug, Clone, toasty::Model)]
pub struct Tag {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
    #[has_many]
    pub taggings: toasty::Deferred<Vec<Tagging>>,
    #[has_many(via = taggings.article)]
    pub articles: toasty::Deferred<Vec<Article>>,
}

/// The join model: one row per article and tag.
#[derive(Debug, Clone, toasty::Model)]
#[key(article_id, tag_id)]
pub struct Tagging {
    #[index]
    pub article_id: uuid::Uuid,
    #[belongs_to(key = article_id, references = id)]
    pub article: toasty::Deferred<Article>,
    #[index]
    pub tag_id: uuid::Uuid,
    #[belongs_to(key = tag_id, references = id)]
    pub tag: toasty::Deferred<Tag>,
}
// ANCHOR_END: forms-many-to-many-models

// ANCHOR: forms-many-to-many-field
#[derive(Debug, Clone, tablo::RecordForm)]
#[form(model = Article)]
pub struct ArticleForm {
    pub title: String,
    // Named like the `via` field, holding the keys of the tags it reaches.
    #[form(relationship = TagResource)]
    pub tags: Vec<uuid::Uuid>,
}

pub fn article_layout() -> Schema<ArticleForm> {
    let c = ArticleForm::controls();
    Schema::new((c.title, c.tags.searchable()))
}
// ANCHOR_END: forms-many-to-many-field

pub struct ArticleResource;

impl Resource for ArticleResource {
    type Model = Article;
    type Form = ArticleForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .form(article_layout())
            .record_title(lens!(Article.title))
    }
}

pub struct TagResource;

impl Resource for TagResource {
    type Model = Tag;
    type Form = NoForm<Tag>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Tag.name)).searchable()))
            .record_title(lens!(Tag.name))
            // ANCHOR: tag-relations
            // The tag's `via` field: its articles, which its page attaches and detaches.
            .relation(Relation::belongs_to_many::<ArticleResource>(
                Tag::fields().articles(),
            ))
        // ANCHOR_END: tag-relations
    }
}
