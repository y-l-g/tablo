//! Filtered choices over HTTP: a parent category choice offers only the top-level categories, in
//! its form, through the options endpoint and on submit.

use tablo::{Allow, Resource, ResourceDef, Schema, Table, TextColumn, lens};
use toasty::Db;
use topcoat::router::Router;
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, post_fields, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Category {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    parent_id: Option<Uuid>,
    #[default(false)]
    archived: bool,
}

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Listing {
    #[key]
    #[auto]
    id: Uuid,
    category_id: Uuid,
    subcategory_id: Uuid,
}

struct CategoryResource;

impl Resource for CategoryResource {
    type Model = Category;
    type Form = CategoryForm;

    fn declare() -> ResourceDef<Self> {
        let c = CategoryForm::controls();
        let parent = c
            .parent_id
            .options_where(|_cx| Category::fields().parent_id().is_none());
        ResourceDef::new()
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Category.name))))
            .record_title(lens!(Category.name))
            .form(Schema::new((c.name, parent)))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Category)]
struct CategoryForm {
    name: String,
    #[form(relationship = CategoryResource)]
    parent_id: Option<Uuid>,
}

struct ListingResource;

impl Resource for ListingResource {
    type Model = Listing;
    type Form = ListingForm;

    fn declare() -> ResourceDef<Self> {
        let c = ListingForm::controls();
        let category = c
            .category_id
            .options_where(|_cx| Category::fields().parent_id().is_none());
        let subcategory = c
            .subcategory_id
            .depends_on(&category, Category::fields().parent_id())
            .options_where(|_cx| Category::fields().archived().eq(false));
        ResourceDef::new()
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Listing.id))))
            .form(Schema::new((category, subcategory)))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Listing)]
struct ListingForm {
    #[form(relationship = CategoryResource)]
    category_id: Uuid,
    #[form(relationship = CategoryResource)]
    subcategory_id: Uuid,
}

/// Books with Novels and the archived Pamphlets under it, and the panel over them.
struct Shelf {
    db: Db,
    router: Router,
    books: Uuid,
    novels: Uuid,
    pamphlets: Uuid,
}

async fn shelf() -> Shelf {
    let mut db = test_db(toasty::models!(Category, Listing)).await;
    let books = toasty::create!(Category {
        name: "Books",
        parent_id: None,
        archived: false,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let mut child = async |name: &str, archived: bool| {
        toasty::create!(Category {
            name: name,
            parent_id: Some(books.id),
            archived: archived,
        })
        .exec(&mut db)
        .await
        .unwrap()
        .id
    };
    let novels = child("Novels", false).await;
    let pamphlets = child("Pamphlets", true).await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<CategoryResource>()
            .resource::<ListingResource>(),
    )
    .expect("panel builds");
    Shelf {
        db,
        router,
        books: books.id,
        novels,
        pamphlets,
    }
}

/// The form offers only the categories the filter selects, and a submission naming another is
/// refused before anything is written.
#[tokio::test]
async fn a_filtered_choice_offers_and_accepts_only_the_rows_it_selects() {
    let shelf = shelf().await;
    let html = body_string(get(&shelf.router, "/admin/categories/-/create").await).await;
    assert!(html.contains(&shelf.books.to_string()), "{html}");
    assert!(
        !html.contains(&shelf.novels.to_string()),
        "a subcategory is no parent option: {html}"
    );

    let create = async |parent: Uuid| {
        post_fields(
            &shelf.router,
            "/admin/categories/-/create",
            &[("name", "Poetry"), ("parent_id", &parent.to_string())],
        )
        .await
    };
    let response = create(shelf.novels).await;
    assert!(!response.status().is_redirection());
    let html = body_string(response).await;
    assert_eq!(
        tablo::testing::field_error(&html, "parent_id"),
        Some(tablo::testing::ErrorKind::Invalid),
        "{html}"
    );
    let mut db = shelf.db.clone();
    let count = async |db: &mut Db| Category::all().exec(db).await.unwrap().len();
    assert_eq!(count(&mut db).await, 3, "nothing is written");

    let response = create(shelf.books).await;
    assert!(
        response.status().is_redirection(),
        "{}",
        body_string(response).await
    );
    assert_eq!(count(&mut db).await, 4);
}

/// A dependent choice's filter narrows the parent value's rows: the options endpoint and the
/// submission both drop the archived subcategory.
#[tokio::test]
async fn a_dependent_choice_applies_its_filter_to_the_parent_values_rows() {
    let shelf = shelf().await;
    let response = get(
        &shelf.router,
        &format!(
            "/admin/listings/-/options?field=subcategory_id&parent={}",
            shelf.books
        ),
    )
    .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(html.contains(&shelf.novels.to_string()), "{html}");
    assert!(
        !html.contains(&shelf.pamphlets.to_string()),
        "an archived subcategory is filtered out: {html}"
    );

    let response = post_fields(
        &shelf.router,
        "/admin/listings/-/create",
        &[
            ("category_id", &shelf.books.to_string()),
            ("subcategory_id", &shelf.pamphlets.to_string()),
        ],
    )
    .await;
    assert!(!response.status().is_redirection());
    assert_eq!(
        tablo::testing::field_error(&body_string(response).await, "subcategory_id"),
        Some(tablo::testing::ErrorKind::Invalid)
    );
    let mut db = shelf.db.clone();
    assert!(Listing::all().exec(&mut db).await.unwrap().is_empty());
}
