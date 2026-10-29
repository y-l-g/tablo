//! Creating a related record from a creatable choice: the link shows only while the related
//! resource lets the user list and create, a refused framed save stays in pick mode, a framed
//! create lands on the record the choice takes or says it cannot be chosen, and mounting refuses a
//! creatable choice that is not a single select over a resource's records.

use tablo::{
    Ability, Allow, DeclarationErrorKind, Resource, ResourceDef, Schema, Table, TextColumn, lens,
    testing::{CreateOptionLink, Picked, create_option_links, field_error, picked, posts_picking},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_string, form_body, get, mount, panel, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Author {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    author_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Author)]
struct AuthorForm {
    name: String,
}

/// Every author but the one named "Hidden", which the user may still create.
fn all_but_hidden(_cx: &Cx, ability: Ability<'_, Author>) -> bool {
    match ability {
        Ability::View(author) => author.name != "Hidden",
        _ => true,
    }
}

struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("authors")
            .policy(all_but_hidden)
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            .record_title(lens!(Author.name))
    }
}

/// The same authors, which the user may read but not create.
struct ReadOnlyAuthorResource;

impl Resource for ReadOnlyAuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("read-only-authors")
            .policy_fn(|_cx, ability| !matches!(ability, Ability::Create))
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            .record_title(lens!(Author.name))
    }
}

/// The same authors, which the user may create but not list.
struct UnlistedAuthorResource;

impl Resource for UnlistedAuthorResource {
    type Model = Author;
    type Form = AuthorForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("unlisted-authors")
            .policy_fn(|_cx, ability| !matches!(ability, Ability::ViewAny))
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            .record_title(lens!(Author.name))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct BookForm {
    title: String,
    #[form(relationship = AuthorResource)]
    author_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct ReadOnlyBookForm {
    title: String,
    #[form(relationship = ReadOnlyAuthorResource)]
    author_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct UnlistedBookForm {
    title: String,
    #[form(relationship = UnlistedAuthorResource)]
    author_id: Uuid,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        let c = BookForm::controls();
        ResourceDef::new()
            .slug("books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
            .form(Schema::new((c.title, c.author_id.creatable())))
    }
}

struct ReadOnlyBookResource;

impl Resource for ReadOnlyBookResource {
    type Model = Book;
    type Form = ReadOnlyBookForm;

    fn declare() -> ResourceDef<Self> {
        let c = ReadOnlyBookForm::controls();
        ResourceDef::new()
            .slug("read-only-books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
            .form(Schema::new((c.title, c.author_id.creatable())))
    }
}

struct UnlistedBookResource;

impl Resource for UnlistedBookResource {
    type Model = Book;
    type Form = UnlistedBookForm;

    fn declare() -> ResourceDef<Self> {
        let c = UnlistedBookForm::controls();
        ResourceDef::new()
            .slug("unlisted-books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
            .form(Schema::new((c.title, c.author_id.creatable())))
    }
}

async fn router() -> (Db, Router) {
    let db = test_db(toasty::models!(Author, Book)).await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<AuthorResource>()
            .resource::<ReadOnlyAuthorResource>()
            .resource::<BookResource>()
            .resource::<ReadOnlyBookResource>()
            .resource::<UnlistedAuthorResource>()
            .resource::<UnlistedBookResource>(),
    )
    .expect("panel builds");
    (db, router)
}

#[tokio::test]
async fn the_link_shows_only_while_the_related_resource_lets_the_user_create() {
    let (_db, router) = router().await;

    let html = body_string(get(&router, "/admin/books/-/create").await).await;
    assert_eq!(
        create_option_links(&html),
        [CreateOptionLink {
            select: "author_id".to_string(),
            href: "/admin/authors/-/create".to_string(),
        }]
    );

    let html = body_string(get(&router, "/admin/read-only-books/-/create").await).await;
    assert!(
        create_option_links(&html).is_empty(),
        "a related resource refusing Create offers no link"
    );

    let html = body_string(get(&router, "/admin/unlisted-books/-/create").await).await;
    assert!(
        create_option_links(&html).is_empty(),
        "a related resource refusing ViewAny offers no link: its record could not be chosen"
    );
}

#[tokio::test]
async fn a_refused_framed_save_stays_in_pick_mode() {
    let (_db, router) = router().await;
    let client = TestClient::new(&router);

    let refused = client
        .submit(
            "/admin/authors/-/create?pick=1",
            &form_body(&[("name", "")]),
        )
        .await;
    let html = body_string(refused).await;
    assert!(
        field_error(&html, "name").is_some(),
        "a blank name is refused"
    );
    assert!(
        posts_picking(&html),
        "the re-rendered form posts back in pick mode, so its next save lands in the dialog"
    );
    let unframed = body_string(client.get("/admin/authors/-/create").await).await;
    assert!(!posts_picking(&unframed));
}

/// Posts a framed create of an author named `name`, following the save to the page it lands on.
async fn framed_create(router: &Router, name: &str) -> Option<Picked> {
    let client = TestClient::new(router);
    let saved = client
        .submit(
            "/admin/authors/-/create?pick=1",
            &form_body(&[("name", name)]),
        )
        .await;
    assert_eq!(saved.status(), 303, "a framed create saves and redirects");
    let landing = saved
        .headers()
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("the save names its landing")
        .to_string();
    picked(&body_string(client.get(&landing).await).await)
}

#[tokio::test]
async fn a_framed_create_lands_on_the_record_the_choice_takes() {
    let (db, router) = router().await;

    let landed = framed_create(&router, "Grace").await;
    let grace = Author::filter(Author::fields().name().eq("Grace"))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the framed create wrote the author");
    assert_eq!(
        landed,
        Some(Picked::Option {
            value: grace.id.to_string(),
            label: "Grace".to_string(),
        })
    );
}

#[tokio::test]
async fn a_record_the_user_may_not_view_is_created_but_not_chosen() {
    let (db, router) = router().await;

    assert_eq!(
        framed_create(&router, "Hidden").await,
        Some(Picked::Unchoosable)
    );
    assert!(
        Author::filter(Author::fields().name().eq("Hidden"))
            .first()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_some(),
        "the create itself went through"
    );
}

#[tokio::test]
async fn mounting_refuses_a_creatable_choice_that_is_not_a_single_select() {
    struct RadioBookResource;

    impl Resource for RadioBookResource {
        type Model = Book;
        type Form = BookForm;

        fn declare() -> ResourceDef<Self> {
            let c = BookForm::controls();
            ResourceDef::new()
                .slug("radio-books")
                .policy(Allow)
                .table(Table::new(TextColumn::new(lens!(Book.title))))
                .form(Schema::new((c.title, c.author_id.radio().creatable())))
        }
    }

    let db = test_db(toasty::models!(Author, Book)).await;
    let errors = refusal(mount(
        db,
        panel()
            .resource::<AuthorResource>()
            .resource::<RadioBookResource>(),
    ));
    assert_eq!(
        errors
            .into_iter()
            .map(|error| error.kind)
            .collect::<Vec<_>>(),
        [DeclarationErrorKind::UncreatableChoice {
            field: "author_id".to_string(),
        }]
    );
}
