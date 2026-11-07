use toasty::stmt::Expr;

use super::*;
use crate::test_support::{cx, memory_db};

#[derive(Debug, Clone, toasty::Model)]
struct Contact {
    #[key]
    #[auto]
    id: uuid::Uuid,
    email: String,
    note: Option<String>,
}

async fn contacts() -> toasty::Db {
    let mut db = memory_db(toasty::models!(Contact)).await;
    for (email, note) in [
        ("ada@example.com", Some("100% reliable")),
        ("grace@navy.mil", Some("admiral")),
        ("linus@example.org", None),
    ] {
        toasty::create!(Contact {
            email: email.to_string(),
            note: note.map(str::to_string),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

async fn emails(db: &toasty::Db, expr: Expr<bool>) -> Vec<String> {
    let mut db = db.clone();
    let mut emails: Vec<String> = Contact::filter(expr)
        .exec(&mut db)
        .await
        .unwrap()
        .into_iter()
        .map(|contact| contact.email)
        .collect();
    emails.sort();
    emails
}

fn term(term: &str, op: &str) -> FilterValue {
    FilterValue::from(term).with_part("op", op)
}

#[tokio::test]
async fn a_text_filter_matches_its_term_the_way_the_op_part_names() {
    let db = contacts().await;
    let email = TextFilter::new(Contact::fields().email());
    let cases = [
        (
            FilterValue::from("EXAMPLE"),
            vec!["ada@example.com", "linus@example.org"],
        ),
        (term("example", "excludes"), vec!["grace@navy.mil"]),
        (term("ada", "starts"), vec!["ada@example.com"]),
        (term(".org", "ends"), vec!["linus@example.org"]),
        (term("grace@navy.mil", "is"), vec!["grace@navy.mil"]),
        (term("grace", "is"), vec![]),
    ];
    for (value, expected) in cases {
        let expr = email.to_expr(&cx(), &value).unwrap();
        assert_eq!(emails(&db, expr).await, expected, "{value:?}");
    }
}

#[tokio::test]
async fn a_text_filters_wildcards_match_only_themselves() {
    let db = contacts().await;
    let note = TextFilter::new(Contact::fields().note());
    let percent = note.to_expr(&cx(), &"0%".into()).unwrap();
    assert_eq!(emails(&db, percent).await, ["ada@example.com"]);
    let underscore = note.to_expr(&cx(), &term("a_miral", "is")).unwrap();
    assert!(emails(&db, underscore).await.is_empty());
}

#[tokio::test]
async fn excluding_a_term_keeps_an_optional_field_holding_nothing() {
    let db = contacts().await;
    let note = TextFilter::new(Contact::fields().note());
    let expr = note.to_expr(&cx(), &term("admiral", "excludes")).unwrap();
    assert_eq!(
        emails(&db, expr).await,
        ["ada@example.com", "linus@example.org"]
    );
}

#[test]
fn a_text_filter_reads_only_the_ways_it_offers() {
    let email = TextFilter::new(Contact::fields().email()).matching([TextMatch::EndsWith]);
    assert!(email.to_expr(&cx(), &term("x", "contains")).is_none());
    assert!(email.to_expr(&cx(), &term("x", "nonsense")).is_none());
    assert!(
        email.to_expr(&cx(), &"x".into()).is_some(),
        "the first way applies"
    );
    let all = TextFilter::new(Contact::fields().email());
    for refused in [
        FilterValue::from("a").with("b"),
        FilterValue::from("a").with_part("to", "b"),
    ] {
        assert!(all.to_expr(&cx(), &refused).is_none(), "{refused:?}");
    }
    let way_alone = FilterValue::new().with_part("op", "starts");
    assert!(all.to_expr(&cx(), &way_alone).is_none());
    assert!(
        all.is_noop_value(&way_alone),
        "an empty term is no filter, not an invalid one"
    );
}
