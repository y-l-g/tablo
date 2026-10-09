use toasty::{
    Deferred,
    stmt::{List, Path, Query},
};

use super::*;
use crate::test_support::memory_db;

#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    #[has_many]
    taggings: Deferred<Vec<Tagging>>,
    #[has_many(via = taggings.tag)]
    tags: Deferred<Vec<Tag>>,
}

#[derive(Debug, Clone, toasty::Model)]
struct Tag {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Tagging {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    post_id: uuid::Uuid,
    #[belongs_to(key = post_id, references = id)]
    post: Deferred<Post>,
    #[index]
    tag_id: uuid::Uuid,
    #[belongs_to(key = tag_id, references = id)]
    tag: Deferred<Tag>,
    note: Option<String>,
}

/// A join model whose extra column a link cannot fill.
#[derive(Debug, Clone, toasty::Model)]
struct Group {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[has_many]
    memberships: Deferred<Vec<Membership>>,
    #[has_many(via = memberships.member)]
    members: Deferred<Vec<Tag>>,
}

#[derive(Debug, Clone, toasty::Model)]
#[key(group_id, member_id)]
struct Membership {
    #[index]
    group_id: uuid::Uuid,
    #[belongs_to(key = group_id, references = id)]
    group: Deferred<Group>,
    #[index]
    member_id: uuid::Uuid,
    #[belongs_to(key = member_id, references = id)]
    member: Deferred<Tag>,
    role: String,
}

async fn db() -> toasty::Db {
    memory_db(toasty::models!(Post, Tag, Tagging, Group, Membership)).await
}

async fn tag(db: &mut toasty::Db, name: &str) -> Tag {
    toasty::create!(Tag { name }).exec(db).await.unwrap()
}

/// The tag ids `post` is linked to, read through Toasty's own `via`.
async fn linked(db: &mut toasty::Db, post: &Post) -> Vec<uuid::Uuid> {
    let mut ids: Vec<_> = post
        .tags()
        .exec(db)
        .await
        .unwrap()
        .into_iter()
        .map(|tag| tag.id)
        .collect();
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<uuid::Uuid>) -> Vec<uuid::Uuid> {
    ids.sort();
    ids
}

#[tokio::test]
async fn links_and_unlinks_one_join_row_per_key() {
    let mut db = db().await;
    let schema = AppSchema::of_db(&db);
    let join = JoinTable::of::<Post>(&schema, "tags").expect("a join model");
    let post = toasty::create!(Post { title: "p" })
        .exec(&mut db)
        .await
        .unwrap();
    let (a, b, c) = (
        tag(&mut db, "a").await,
        tag(&mut db, "b").await,
        tag(&mut db, "c").await,
    );
    let keys = |tags: &[&Tag]| tags.iter().map(|t| t.id.to_string()).collect::<Vec<_>>();
    join.link(&post, &keys(&[&a, &b, &c]), &mut db)
        .await
        .unwrap();
    assert_eq!(linked(&mut db, &post).await, sorted(vec![a.id, b.id, c.id]));
    join.unlink(&post, &keys(&[&a, &c]), &mut db).await.unwrap();
    assert_eq!(linked(&mut db, &post).await, vec![b.id]);
    let rows: Vec<Tagging> = Tagging::all().exec(&mut db).await.unwrap();
    assert_eq!(rows.len(), 1, "unlinking deletes the join rows");
}

#[tokio::test]
async fn unlinking_touches_only_the_owners_rows() {
    let mut db = db().await;
    let join = JoinTable::of::<Post>(&AppSchema::of_db(&db), "tags").unwrap();
    let one = toasty::create!(Post { title: "one" })
        .exec(&mut db)
        .await
        .unwrap();
    let two = toasty::create!(Post { title: "two" })
        .exec(&mut db)
        .await
        .unwrap();
    let shared = tag(&mut db, "shared").await;
    let key = vec![shared.id.to_string()];
    join.link(&one, &key, &mut db).await.unwrap();
    join.link(&two, &key, &mut db).await.unwrap();
    join.unlink(&one, &key, &mut db).await.unwrap();
    assert!(linked(&mut db, &one).await.is_empty());
    assert_eq!(linked(&mut db, &two).await, vec![shared.id]);
}

#[tokio::test]
async fn linked_to_selects_the_targets_of_one_owner() {
    let mut db = db().await;
    let join = JoinTable::of::<Post>(&AppSchema::of_db(&db), "tags").unwrap();
    let one = toasty::create!(Post { title: "one" })
        .exec(&mut db)
        .await
        .unwrap();
    let two = toasty::create!(Post { title: "two" })
        .exec(&mut db)
        .await
        .unwrap();
    let (a, b, _) = (
        tag(&mut db, "a").await,
        tag(&mut db, "b").await,
        tag(&mut db, "unlinked").await,
    );
    join.link(&one, &[a.id.to_string()], &mut db).await.unwrap();
    join.link(&two, &[b.id.to_string()], &mut db).await.unwrap();
    let via: Path<Post, List<Tag>> = Post::fields().tags().into();
    let tags: Vec<Tag> = Query::<List<Tag>>::all()
        .filter(linked_to(&one, via))
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(tags.iter().map(|t| t.id).collect::<Vec<_>>(), vec![a.id]);
}

#[tokio::test]
async fn refuses_a_field_that_is_no_join() {
    let db = db().await;
    let schema = AppSchema::of_db(&db);
    for field in ["title", "taggings", "missing"] {
        assert_eq!(
            JoinTable::of::<Post>(&schema, field).err(),
            Some(ManyToManyFault::NotJoinModel),
            "{field}"
        );
    }
}

#[tokio::test]
async fn refuses_a_join_model_with_a_column_a_link_cannot_fill() {
    let db = db().await;
    assert_eq!(
        JoinTable::of::<Group>(&AppSchema::of_db(&db), "members").err(),
        Some(ManyToManyFault::UnwritableColumn {
            model: "Membership".to_string(),
            column: "role".to_string(),
        })
    );
}

#[tokio::test]
async fn links_a_record_once_however_its_key_is_spelled() {
    let mut db = db().await;
    let join = JoinTable::of::<Post>(&AppSchema::of_db(&db), "tags").unwrap();
    let post = toasty::create!(Post { title: "p" })
        .exec(&mut db)
        .await
        .unwrap();
    let a = tag(&mut db, "a").await;
    let spellings = [a.id.to_string(), a.id.to_string().to_uppercase()];
    join.link(&post, &spellings, &mut db).await.unwrap();
    let rows: Vec<Tagging> = Tagging::all().exec(&mut db).await.unwrap();
    assert_eq!(rows.len(), 1);
}
