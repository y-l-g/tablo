//! `ResourceDef::query`: each panel narrows the rows one resource type serves, after the
//! resource's own narrowing, and every loader reads the narrowed rows.

use tablo::{
    Allow, Auth, NoForm, Panel, Resource, ResourceDef, RouterBuilderPanelExt, SelectFilter, Table,
    TextColumn, lens,
    testing::{body_string, filter_options, row_keys},
};
use toasty::Db;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use uuid::Uuid;

use crate::common::{get, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    owner: String,
    title: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Comment {
    #[key]
    #[auto]
    id: Uuid,
    note_id: Uuid,
    body: String,
}

/// Every note but a draft, on every panel.
struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("notes")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .record_title(lens!(Note.title))
            .query(|_cx, query| query.filter(Note::fields().title().ne("Draft")))
    }
}

struct CommentResource;

impl Resource for CommentResource {
    type Model = Comment;
    type Form = NoForm<Comment>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().slug("comments").policy(Allow).table(
            Table::new(TextColumn::new(lens!(Comment.body))).filters(SelectFilter::relationship::<
                NoteResource,
            >(lens!(
                Comment.note_id
            ))),
        )
    }
}

struct Seeded {
    router: Router,
    mine: Note,
    theirs: Note,
}

/// The admin panel serves every note; the portal narrows them to the ones `me` owns.
async fn seeded() -> Seeded {
    let mut db: Db = test_db(toasty::models!(Note, Comment)).await;
    let mut notes = Vec::new();
    for (owner, title) in [("me", "Mine"), ("them", "Theirs"), ("me", "Draft")] {
        notes.push(
            toasty::create!(Note {
                owner: owner.to_string(),
                title: title.to_string(),
            })
            .exec(&mut db)
            .await
            .unwrap(),
        );
    }
    toasty::create!(Comment {
        note_id: notes[0].id,
        body: "first".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = Router::builder()
        .discover()
        .with(db)
        .panel(
            Panel::new("admin")
                .auth(Auth::disabled())
                .resource::<NoteResource>()
                .resource::<CommentResource>(),
        )
        .expect("admin panel builds")
        .panel(
            Panel::new("portal")
                .auth(Auth::disabled())
                .resource_with::<NoteResource>(|def| {
                    def.query(|_cx, query| query.filter(Note::fields().owner().eq("me")))
                })
                .resource::<CommentResource>(),
        )
        .expect("portal panel builds")
        .build();
    let mut notes = notes.into_iter();
    let (mine, theirs) = (notes.next().unwrap(), notes.next().unwrap());
    Seeded {
        router,
        mine,
        theirs,
    }
}

async fn page(router: &Router, uri: &str) -> String {
    body_string(get(router, uri).await).await
}

/// Each panel lists the rows its own narrowing leaves, after the resource's: the admin panel
/// every note but the draft, the portal the one note `me` owns that is not a draft.
#[tokio::test]
async fn each_panel_lists_the_rows_its_narrowing_leaves() {
    let Seeded {
        router,
        mine,
        theirs,
    } = seeded().await;

    let mut admin = row_keys(&page(&router, "/admin/notes").await);
    admin.sort();
    let mut expected = vec![mine.id.to_string(), theirs.id.to_string()];
    expected.sort();
    assert_eq!(admin, expected);
    assert_eq!(
        row_keys(&page(&router, "/portal/notes").await),
        [mine.id.to_string()]
    );
}

/// A row the portal's narrowing leaves out does not exist there: its detail page answers 404,
/// the export leaves it out, and a relationship filter does not offer it.
#[tokio::test]
async fn a_row_the_narrowing_leaves_out_reaches_no_loader() {
    let Seeded {
        router,
        mine,
        theirs,
    } = seeded().await;

    let theirs_page = format!("/notes/{}", theirs.id);
    assert_eq!(
        get(&router, &format!("/admin{theirs_page}")).await.status(),
        200
    );
    assert_eq!(
        get(&router, &format!("/portal{theirs_page}"))
            .await
            .status(),
        404
    );

    let csv = page(&router, "/portal/notes/-/export").await;
    assert!(csv.contains("Mine") && !csv.contains("Theirs"), "{csv}");

    let options = filter_options(&page(&router, "/portal/comments").await, "note_id")
        .expect("the comments list offers a note filter");
    let offered: Vec<String> = options
        .into_iter()
        .map(|option| option.value)
        .filter(|value| !value.is_empty())
        .collect();
    assert_eq!(offered, [mine.id.to_string()]);
}
