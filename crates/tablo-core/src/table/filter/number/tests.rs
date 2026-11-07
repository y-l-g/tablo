use toasty::stmt::Expr;

use super::*;
use crate::test_support::{cx, memory_db};

#[derive(Debug, Clone, toasty::Model)]
struct Book {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    pages: i64,
    rating: Option<f64>,
}

async fn shelf() -> toasty::Db {
    let mut db = memory_db(toasty::models!(Book)).await;
    for (title, pages, rating) in [
        ("Short", 90, Some(3.5)),
        ("Middle", 300, None),
        ("Long", 1200, Some(4.5)),
    ] {
        toasty::create!(Book {
            title: title.to_string(),
            pages,
            rating,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

async fn titles(db: &toasty::Db, expr: Expr<bool>) -> Vec<String> {
    let mut db = db.clone();
    let mut titles: Vec<String> = Book::filter(expr)
        .exec(&mut db)
        .await
        .unwrap()
        .into_iter()
        .map(|book| book.title)
        .collect();
    titles.sort();
    titles
}

fn range(min: Option<&str>, max: Option<&str>) -> FilterValue {
    let mut value = FilterValue::new();
    if let Some(min) = min {
        value = value.with_part("min", min);
    }
    if let Some(max) = max {
        value = value.with_part("max", max);
    }
    value
}

#[tokio::test]
async fn a_number_filter_matches_one_value_or_the_range_its_bounds_include() {
    let db = shelf().await;
    let pages = NumberFilter::new(Book::fields().pages()).range();
    let exact = pages.to_expr(&cx(), &"300".into()).unwrap();
    assert_eq!(titles(&db, exact).await, ["Middle"]);
    let both = pages
        .to_expr(&cx(), &range(Some("90"), Some("300")))
        .unwrap();
    assert_eq!(
        titles(&db, both).await,
        ["Middle", "Short"],
        "both ends included"
    );
    let open = pages.to_expr(&cx(), &range(Some("301"), None)).unwrap();
    assert_eq!(titles(&db, open).await, ["Long"]);
    let upper = pages.to_expr(&cx(), &range(None, Some("89"))).unwrap();
    assert!(titles(&db, upper).await.is_empty());
}

#[tokio::test]
async fn an_optional_number_holding_nothing_matches_no_bound() {
    let db = shelf().await;
    let rating = NumberFilter::new(Book::fields().rating()).range();
    let low = rating.to_expr(&cx(), &range(None, Some("5"))).unwrap();
    assert_eq!(titles(&db, low).await, ["Long", "Short"]);
    let fraction = rating.to_expr(&cx(), &"4.5".into()).unwrap();
    assert_eq!(titles(&db, fraction).await, ["Long"]);
}

#[test]
fn a_number_filter_refuses_what_bounds_nothing_it_can_read() {
    let pages = NumberFilter::new(Book::fields().pages()).range();
    for refused in [
        FilterValue::from("ninety"),
        FilterValue::from("1.5"),
        range(Some("400"), Some("300")),
        range(Some("x"), None),
        FilterValue::new().with_part("from", "1"),
        FilterValue::from("1").with_part("min", "1"),
    ] {
        assert!(pages.to_expr(&cx(), &refused).is_none(), "{refused:?}");
    }
    let single = NumberFilter::new(Book::fields().pages());
    assert!(
        single.to_expr(&cx(), &range(Some("1"), None)).is_none(),
        "a filter without a range renders no bound input, so it reads no part"
    );
}
