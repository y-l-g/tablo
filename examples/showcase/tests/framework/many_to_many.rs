//! A many-to-many relation's table over test-local models: what the owner's and the related
//! resource's policies refuse, what the owner's hook hears, and the join models mounting refuses.

use std::sync::Mutex;

use tablo::{
    Ability, Committed, DeclarationErrorKind, ManyToManyFault, Mutation, Relation, Resource,
    ResourceDef, Table, TextColumn, lens,
};
use toasty::{Db, Deferred};
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{body_string, get, memory_db, mount, panel, post_fields, refusal};

#[derive(Debug, toasty::Model, Clone)]
struct Shelf {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[has_many]
    placements: Deferred<Vec<Placement>>,
    #[has_many(via = placements.book)]
    books: Deferred<Vec<Book>>,
}

#[derive(Debug, toasty::Model, Clone)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

/// The join model: an optional note is a column a link leaves empty.
#[derive(Debug, toasty::Model, Clone)]
struct Placement {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    shelf_id: Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: Deferred<Shelf>,
    #[index]
    book_id: Uuid,
    #[belongs_to(key = book_id, references = id)]
    book: Deferred<Book>,
    note: Option<String>,
}

/// What the shelf's hook heard: each mutation, with the shelf it names.
static HEARD: Mutex<Vec<(&'static str, Uuid)>> = Mutex::new(Vec::new());

/// Shelves named "Locked" open but refuse updates, so they refuse attaching and detaching too.
struct ShelfResource;

impl Resource for ShelfResource {
    type Model = Shelf;
    type Form = ShelfForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(|_cx: &Cx, ability: Ability<'_, Shelf>| match ability {
                Ability::Update(shelf) => shelf.name != "Locked",
                Ability::Create => false,
                _ => true,
            })
            .table(Table::new(TextColumn::new(lens!(Shelf.name))))
            .view(tablo::Detail::new(TextColumn::new(lens!(Shelf.name))))
            .relation(Relation::belongs_to_many::<BookResource>(
                Shelf::fields().books(),
            ))
    }

    async fn after_commit(_cx: &Cx, committed: Committed<Shelf>) -> topcoat::Result<()> {
        let mutation = match committed.mutation() {
            Mutation::Attach => "attach",
            Mutation::Detach => "detach",
            _ => "other",
        };
        let shelves = committed.records().iter().map(|shelf| shelf.id);
        HEARD
            .lock()
            .unwrap()
            .extend(shelves.map(|id| (mutation, id)));
        Ok(())
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Shelf)]
struct ShelfForm {
    name: String,
    #[form(relationship = BookResource)]
    books: Vec<Uuid>,
}

/// Books titled "Hidden" cannot be viewed.
struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo::NoForm<Book>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(|_cx: &Cx, ability: Ability<'_, Book>| match ability {
                Ability::View(book) => book.title != "Hidden",
                Ability::Create => false,
                _ => true,
            })
            .table(Table::new(TextColumn::new(lens!(Book.title)).searchable()))
    }
}

struct Library {
    router: Router,
    db: Db,
    open: Shelf,
    locked: Shelf,
    seen: Book,
    hidden: Book,
}

/// Both shelves hold both books.
async fn library() -> Library {
    let mut db = memory_db(toasty::models!(Shelf, Book, Placement)).await;
    let open = toasty::create!(Shelf { name: "Open" })
        .exec(&mut db)
        .await
        .unwrap();
    let locked = toasty::create!(Shelf { name: "Locked" })
        .exec(&mut db)
        .await
        .unwrap();
    let seen = toasty::create!(Book { title: "Seen" })
        .exec(&mut db)
        .await
        .unwrap();
    let hidden = toasty::create!(Book { title: "Hidden" })
        .exec(&mut db)
        .await
        .unwrap();
    for shelf in [&open, &locked] {
        for book in [&seen, &hidden] {
            toasty::create!(Placement {
                shelf_id: shelf.id,
                book_id: book.id,
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
    }
    let router = mount(
        db.clone(),
        panel()
            .resource::<ShelfResource>()
            .resource::<BookResource>(),
    )
    .expect("the panel mounts");
    Library {
        router,
        db,
        open,
        locked,
        seen,
        hidden,
    }
}

/// The books `shelf` holds, by the join rows that place them.
async fn held(db: &Db, shelf: &Shelf) -> Vec<Uuid> {
    let mut db = db.clone();
    let mut books: Vec<Uuid> = Placement::filter(Placement::fields().shelf_id().eq(shelf.id))
        .exec(&mut db)
        .await
        .unwrap()
        .into_iter()
        .map(|placement| placement.book_id)
        .collect();
    books.sort();
    books
}

fn relation(shelf: &Shelf) -> String {
    format!("/admin/shelves/{}/-/relations/books", shelf.id)
}

#[tokio::test]
async fn an_owner_the_user_may_not_update_neither_attaches_nor_detaches() {
    let Library {
        router,
        db,
        locked,
        seen,
        ..
    } = library().await;
    let before = held(&db, &locked).await;
    let html = body_string(get(&router, &format!("/admin/shelves/{}", locked.id)).await).await;
    assert!(
        !html.contains("/-/relations/books/"),
        "the table offers neither attach nor detach: {html}"
    );
    let book = seen.id.to_string();
    for (path, fields) in [
        (
            format!("{}/-/actions/attach", relation(&locked)),
            vec![("record", book.as_str())],
        ),
        (
            format!("{}/-/actions/detach", relation(&locked)),
            vec![("ids", book.as_str())],
        ),
        (
            format!("{}/{book}/-/actions/detach", relation(&locked)),
            vec![],
        ),
    ] {
        let resp = post_fields(&router, &path, &fields).await;
        assert_eq!(resp.status(), 403, "{path}");
    }
    let options = get(
        &router,
        &format!(
            "{}/-/actions/attach/options?field=record&q=",
            relation(&locked)
        ),
    )
    .await;
    assert_eq!(options.status(), 403, "the dialog's search");
    assert_eq!(held(&db, &locked).await, before);
}

#[tokio::test]
async fn a_book_the_user_may_not_view_is_not_detached() {
    let Library {
        router,
        db,
        open,
        hidden,
        ..
    } = library().await;
    let before = held(&db, &open).await;
    let resp = post_fields(
        &router,
        &format!("{}/{}/-/actions/detach", relation(&open), hidden.id),
        &[],
    )
    .await;
    assert_eq!(resp.status(), 403);
    assert_eq!(held(&db, &open).await, before);
}

#[tokio::test]
async fn the_owners_hook_hears_each_attach_and_detach() {
    let Library {
        router,
        db,
        open,
        seen,
        ..
    } = library().await;
    let mut db_q = db.clone();
    let fresh = toasty::create!(Book { title: "Fresh" })
        .exec(&mut db_q)
        .await
        .unwrap();
    let fresh = fresh.id.to_string();
    let seen = seen.id.to_string();
    let resp = post_fields(
        &router,
        &format!("{}/-/actions/attach", relation(&open)),
        &[("record", &fresh)],
    )
    .await;
    assert_eq!(resp.status(), 303);
    let resp = post_fields(
        &router,
        &format!("{}/{seen}/-/actions/detach", relation(&open)),
        &[],
    )
    .await;
    assert_eq!(resp.status(), 303);
    // Another test's shelf may land in between: keep only this one's.
    let heard: Vec<_> = HEARD
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, id)| *id == open.id)
        .map(|(mutation, _)| *mutation)
        .collect();
    assert_eq!(heard, ["attach", "detach"]);
}

#[tokio::test]
async fn an_edit_leaves_linked_the_books_the_user_may_not_view() {
    let Library {
        router,
        db,
        open,
        hidden,
        ..
    } = library().await;
    let html = body_string(get(&router, &format!("/admin/shelves/{}/edit", open.id)).await).await;
    assert!(
        !html.contains(&hidden.id.to_string()),
        "the form offers no hidden book: {html}"
    );
    // Unchecking every box the form offers unlinks only what it offered.
    let resp = post_fields(
        &router,
        &format!("/admin/shelves/{}/edit", open.id),
        &[("books", "")],
    )
    .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(held(&db, &open).await, vec![hidden.id]);
}

#[tokio::test]
async fn an_edit_keeps_its_links_past_the_option_cap() {
    let Library {
        router,
        db,
        open,
        seen,
        ..
    } = library().await;
    let mut db_q = db.clone();
    for n in 0..=tablo::schema::MAX_RELATIONSHIP_OPTIONS {
        let title = format!("Filler {n}");
        toasty::create!(Book { title })
            .exec(&mut db_q)
            .await
            .unwrap();
    }
    let seen = seen.id.to_string();
    let resp = post_fields(
        &router,
        &format!("/admin/shelves/{}/edit", open.id),
        &[("name", "Renamed"), ("books", ""), ("books", &seen)],
    )
    .await;
    assert_eq!(resp.status(), 303, "{}", body_string(resp).await);
    assert_eq!(held(&db, &open).await.len(), 2, "both links kept");
}

/// A join model holding a column a link cannot fill.
#[derive(Debug, toasty::Model, Clone)]
struct Club {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[has_many]
    memberships: Deferred<Vec<Membership>>,
    #[has_many(via = memberships.member)]
    members: Deferred<Vec<Book>>,
}

#[derive(Debug, toasty::Model, Clone)]
#[key(club_id, member_id)]
struct Membership {
    #[index]
    club_id: Uuid,
    #[belongs_to(key = club_id, references = id)]
    club: Deferred<Club>,
    #[index]
    member_id: Uuid,
    #[belongs_to(key = member_id, references = id)]
    member: Deferred<Book>,
    role: String,
}

struct ClubResource;

impl Resource for ClubResource {
    type Model = Club;
    type Form = ClubForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Club.name))))
            .relation(Relation::belongs_to_many::<BookResource>(
                Club::fields().members(),
            ))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Club)]
struct ClubForm {
    #[form(relationship = BookResource)]
    members: Vec<Uuid>,
}

#[tokio::test]
async fn mounting_refuses_a_join_model_a_link_cannot_fill() {
    let db = memory_db(toasty::models!(Club, Book, Membership)).await;
    let fault = ManyToManyFault::UnwritableColumn {
        model: "Membership".to_string(),
        column: "role".to_string(),
    };
    let kinds: Vec<DeclarationErrorKind> = refusal(mount(
        db,
        panel()
            .resource::<ClubResource>()
            .resource::<BookResource>(),
    ))
    .into_iter()
    .map(|error| error.kind)
    .collect();
    let refused = DeclarationErrorKind::ManyToMany {
        field: "members".to_string(),
        fault,
    };
    assert_eq!(
        kinds,
        [refused.clone(), refused],
        "the relation and the form field"
    );
}

/// A multiple choice over a column: no join model stores its list.
struct TaggedShelfResource;

impl Resource for TaggedShelfResource {
    type Model = Shelf;
    type Form = TaggedShelfForm;

    fn declare() -> ResourceDef<Self> {
        let c = TaggedShelfForm::controls();
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Shelf.name))))
            .form(tablo::Schema::new(
                c.name.choice().options(["a", "b"]).multiple(),
            ))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Shelf)]
struct TaggedShelfForm {
    name: String,
}

#[tokio::test]
async fn mounting_refuses_a_multiple_choice_over_a_column() {
    let db = memory_db(toasty::models!(Shelf, Book, Placement)).await;
    let kinds: Vec<DeclarationErrorKind> =
        refusal(mount(db, panel().resource::<TaggedShelfResource>()))
            .into_iter()
            .map(|error| error.kind)
            .collect();
    assert_eq!(
        kinds,
        [DeclarationErrorKind::ManyToMany {
            field: "name".to_string(),
            fault: ManyToManyFault::NoJoin,
        }]
    );
}
