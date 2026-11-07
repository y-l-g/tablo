use super::*;
use crate::test_support::{cx, memory_db};

#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    cover: Option<String>,
}

#[tokio::test]
async fn a_presence_filter_splits_records_by_whether_the_field_holds_a_value() {
    let mut db = memory_db(toasty::models!(Post)).await;
    for (title, cover) in [("Covered", Some("a.png")), ("Bare", None)] {
        toasty::create!(Post {
            title: title.to_string(),
            cover: cover.map(str::to_string),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cover = PresenceFilter::new(Post::fields().cover());
    for (value, expected) in [("set", "Covered"), ("empty", "Bare")] {
        let expr = cover.to_expr(&cx(), &value.into()).unwrap();
        let rows = Post::filter(expr).exec(&mut db).await.unwrap();
        let titles: Vec<&str> = rows.iter().map(|post| post.title.as_str()).collect();
        assert_eq!(titles, [expected], "{value}");
    }
    assert!(cover.to_expr(&cx(), &"yes".into()).is_none());
}
