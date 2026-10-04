use super::*;
#[derive(Debug, toasty::Model)]
struct DummyUser {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[unique]
    email: String,
}

#[test]
fn composite_pk_has_no_url_representation() {
    // Composite keys fail loudly (programmer error), never a
    // per-id 404 that hides the misconfiguration.
    #[derive(Debug, Clone, toasty::Model)]
    struct Pair {
        #[key]
        a: String,
        #[key]
        b: String,
        name: String,
    }
    assert!(pk_is_composite::<Pair>());
    assert!(!pk_is_composite::<DummyUser>());
    assert!(pk_eq_expr::<Pair>("anything").is_none());
}

#[test]
fn pk_in_expr_builds_one_in_predicate_and_fails_closed() {
    // Single IN predicate; empty lists and unparseable ids yield
    // None (empty posts redirect with a toast before reaching here;
    // an unparseable id cannot exist, so the batch must not
    // silently drop it — the handler maps None to 404).
    assert!(pk_in_expr::<DummyUser>(&[]).is_none());
    assert!(pk_in_expr::<DummyUser>(&["not-a-uuid"]).is_none());
    let a = uuid::Uuid::new_v4().to_string();
    let b = uuid::Uuid::new_v4().to_string();
    assert!(pk_in_expr::<DummyUser>(&[a.as_str(), b.as_str()]).is_some());
    assert!(pk_in_expr::<DummyUser>(&[a.as_str(), "not-a-uuid"]).is_none());
}

#[derive(Debug, Clone, toasty::Model)]
struct TemporalPk {
    #[key]
    at: jiff::Timestamp,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct ZonedPk {
    #[key]
    at: jiff::Zoned,
    name: String,
}

#[tokio::test]
async fn zoned_pks_parse_from_url_ids() {
    // Zoned PKs parse from canonical forms, in lockstep with the cursor
    // codec; garbage stays a 404.
    let z = jiff::civil::date(2024, 1, 15)
        .at(9, 30, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .unwrap();
    assert!(pk_eq_expr::<ZonedPk>(&z.to_string()).is_some());
    assert!(pk_eq_expr::<ZonedPk>("not-a-time").is_none());
    // Round-trip through sqlite: the parsed value filters the row.
    let mut db = toasty::Db::builder()
        .models(toasty::models!(ZonedPk))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(ZonedPk {
        at: z.clone(),
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let mut db2 = db.clone();
    let expr = pk_eq_expr::<ZonedPk>(&z.to_string()).unwrap();
    let rows = ZonedPk::filter(expr).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");
}

#[tokio::test]
async fn temporal_pks_parse_from_url_ids() {
    // Parse level: canonical forms resolve, garbage does not.
    assert!(pk_eq_expr::<TemporalPk>("2024-01-15T09:30:00Z").is_some());
    assert!(pk_eq_expr::<TemporalPk>("not-a-time").is_none());
    // Round-trip through sqlite: the parsed value filters the row.
    let mut db = toasty::Db::builder()
        .models(toasty::models!(TemporalPk))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(TemporalPk {
        at: "2024-01-15T09:30:00Z".parse::<jiff::Timestamp>().unwrap(),
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let mut db2 = db.clone();
    let expr = pk_eq_expr::<TemporalPk>("2024-01-15T09:30:00Z").unwrap();
    let rows = TemporalPk::filter(expr).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");
}

#[derive(Debug, Clone, toasty::Model)]
#[key(owner, slot)]
struct Seat {
    owner: String,
    slot: i64,
    label: String,
}

#[test]
fn pk_text_spells_the_key_as_its_url_id() {
    let user = DummyUser {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
        email: "ada@example.com".to_string(),
    };
    assert_eq!(pk_text(&user), user.id.to_string());
    assert!(pk_eq_expr::<DummyUser>(&pk_text(&user)).is_some());
    let seat = Seat {
        owner: "ada".to_string(),
        slot: 2,
        label: "Aisle".to_string(),
    };
    assert_eq!(pk_text(&seat), "ada,2");
}

#[tokio::test]
async fn pk_filter_selects_one_record_by_a_composite_key() {
    let mut db = toasty::Db::builder()
        .models(toasty::models!(Seat))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for (owner, slot) in [("ada", 1), ("ada", 2), ("bob", 2)] {
        toasty::create!(Seat {
            owner: owner.to_string(),
            slot,
            label: format!("{owner}-{slot}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let target = Seat {
        owner: "ada".to_string(),
        slot: 2,
        label: String::new(),
    };
    let rows = Seat::filter(pk_filter(&target))
        .exec(&mut db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "ada-2");
}

#[tokio::test]
async fn pk_expr_matches_an_optional_foreign_key() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Sheet {
        #[key]
        #[auto]
        id: uuid::Uuid,
        owner_id: Option<uuid::Uuid>,
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(DummyUser, Sheet))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let owner = toasty::create!(DummyUser {
        name: "Ada".to_string(),
        email: "ada@example.com".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    for owner_id in [Some(owner.id), None] {
        toasty::create!(Sheet { owner_id })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let rows = Sheet::filter(
        Sheet::fields()
            .owner_id()
            .eq(pk_expr::<DummyUser, Option<uuid::Uuid>>(&owner)),
    )
    .exec(&mut db)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].owner_id, Some(owner.id));
}
