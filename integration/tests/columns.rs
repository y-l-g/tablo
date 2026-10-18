//! What a list's cells show for the column kinds that carry markup of their own: a swatch, a
//! thumbnail, a description under the value.

use tablo::{ColorColumn, ImageColumn, ReadOnly, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, panel_router, rows};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Brand {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    owner: String,
    color: String,
    logo: String,
}

struct BrandResource;

impl Resource for BrandResource {
    type Model = Brand;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("brands")
            .policy(ReadOnly)
            .table(Table::new((
                TextColumn::new(lens!(Brand.name)).description(|brand: &Brand| brand.owner.clone()),
                ColorColumn::new(lens!(Brand.color)),
                ImageColumn::new(lens!(Brand.logo)),
            )))
    }
}

/// The same brands, their logos allowed from another host.
struct RemoteBrandResource;

impl Resource for RemoteBrandResource {
    type Model = Brand;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("brands")
            .policy(ReadOnly)
            .table(Table::new(ImageColumn::new(lens!(Brand.logo)).remote()))
    }
}

async fn brands(rows: &[(&str, &str, &str)]) -> Db {
    let mut db = memory_db(toasty::models!(Brand)).await;
    for (name, color, logo) in rows {
        toasty::create!(Brand {
            name: name.to_string(),
            owner: format!("{name} Corp"),
            color: color.to_string(),
            logo: logo.to_string(),
        })
        .exec(&mut db)
        .await
        .expect("seed brand");
    }
    db
}

async fn list<R: Resource>(db: Db) -> String {
    body_string(get(&panel_router::<R>(db), "/admin/brands").await).await
}

#[tokio::test]
async fn a_cell_shows_its_description_under_the_value() {
    let html = list::<BrandResource>(brands(&[("Acme", "", "")]).await).await;
    let row = rows(&html).pop().expect("one row");
    assert!(
        row.cells
            .iter()
            .any(|cell| cell.contains("Acme") && cell.contains("Acme Corp")),
        "{row:?}"
    );
}

/// A color paints a swatch only when nothing in it can leave the `style` declaration.
#[tokio::test]
async fn a_swatch_paints_only_a_plain_color() {
    let html = list::<BrandResource>(
        brands(&[
            ("Plain", "#3b82f6", ""),
            ("Hostile", "red; background-image: url(x)", ""),
        ])
        .await,
    )
    .await;
    assert!(html.contains("background-color: #3b82f6"), "{html}");
    assert!(!html.contains("background-color: red"), "{html}");
    assert!(
        rows(&html).iter().any(|row| row
            .cells
            .iter()
            .any(|cell| cell.contains("background-image: url(x)"))),
        "a refused color stays readable as text: {html}"
    );
}

/// A thumbnail loads a path on this site, and an `http(s)` URL only for a remote column, which
/// sends no referrer.
#[tokio::test]
async fn a_thumbnail_loads_only_a_path_it_may() {
    let remote = "https://cdn.example/logo.png";
    let db = brands(&[
        ("Local", "", "/uploads/logo.png"),
        ("Script", "", "javascript:alert(1)"),
        ("Elsewhere", "", "//evil.example/x.png"),
        ("Remote", "", remote),
    ])
    .await;

    let html = list::<BrandResource>(db.clone()).await;
    let images: Vec<&str> = html
        .match_indices("<img")
        .map(|(at, _)| &html[at..])
        .collect();
    assert_eq!(images.len(), 1, "only the rooted path loads: {html}");
    assert!(images[0].contains("src=\"/uploads/logo.png\""), "{html}");

    let html = list::<RemoteBrandResource>(db).await;
    assert!(
        html.contains(&format!("src=\"{remote}\""))
            && html.contains("referrerpolicy=\"no-referrer\""),
        "{html}"
    );
    assert!(!html.contains("src=\"javascript:"), "{html}");
    assert!(!html.contains("src=\"//evil"), "{html}");
}
