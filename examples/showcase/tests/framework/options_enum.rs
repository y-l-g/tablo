//! An `Options` enum field filtered against the database: the posted option value, not Toasty's
//! stored discriminant, selects the rows.

use tablo::{SelectFilter, extend::Filter as _};
use toasty::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum Stage {
    Open,
    #[option(value = "arch", label = "Archive")]
    Archived,
}

#[derive(Debug, toasty::Model)]
struct Ticket {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    stage: Stage,
    previous: Option<Stage>,
}

async fn db() -> Db {
    let mut db = Db::builder()
        .models(toasty::models!(Ticket))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for (title, stage, previous) in [
        ("open", Stage::Open, None),
        ("archived", Stage::Archived, Some(Stage::Open)),
        ("reopened", Stage::Open, Some(Stage::Archived)),
    ] {
        toasty::create!(Ticket {
            title: title.to_string(),
            stage,
            previous,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

async fn titles(db: &mut Db, filter: &SelectFilter<Ticket>, value: &str) -> Vec<String> {
    let predicate = filter.to_expr(value).expect("a listed option selects rows");
    let mut titles: Vec<String> = Ticket::filter(predicate)
        .exec(db)
        .await
        .unwrap()
        .into_iter()
        .map(|ticket| ticket.title)
        .collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn a_select_filter_matches_the_variant_its_option_value_names() {
    let mut db = db().await;
    let stage = SelectFilter::of(Ticket::fields().stage());
    assert_eq!(titles(&mut db, &stage, "arch").await, ["archived"]);
    assert_eq!(titles(&mut db, &stage, "open").await, ["open", "reopened"]);
    assert!(
        stage.to_expr("archived").is_none(),
        "the variant's stored name is not an option value"
    );
    let previous = SelectFilter::of(Ticket::fields().previous());
    assert_eq!(titles(&mut db, &previous, "arch").await, ["reopened"]);
}
