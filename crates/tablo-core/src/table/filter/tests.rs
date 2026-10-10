use super::*;
use crate::test_support::memory_db;

#[derive(Debug, Clone, toasty::Model)]
struct Task {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    status: String,
    featured: bool,
    created_at: jiff::Timestamp,
}

#[derive(Debug, Clone, PartialEq, toasty::Embed)]
enum Vehicule {
    Auto {
        #[shared(puissance)]
        puissance: String,
        seats: String,
    },
    Moto {
        #[shared(puissance)]
        puissance: String,
        cc: String,
    },
}

#[derive(Debug, Clone, toasty::Model)]
struct Driver {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    vehicule: Vehicule,
}

#[derive(Debug, Clone, toasty::Model)]
struct Event {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    day: jiff::civil::Date,
    starts_at: jiff::civil::DateTime,
    closed_at: Option<jiff::Timestamp>,
}

fn vehicule_filter() -> QueryFilter<Driver> {
    QueryFilter::new("vehicule", "Véhicule")
        .option("Auto", Driver::fields().vehicule().is_auto())
        .option("Moto", Driver::fields().vehicule().is_moto())
}

#[tokio::test]
async fn date_filter_date_only_matches_whole_day() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, ts) in [
        ("Morning", "2024-01-15T09:30:00Z"),
        ("Night", "2024-01-15T23:59:59Z"),
        ("Next", "2024-01-16T00:00:01Z"),
    ] {
        toasty::create!(Task {
            title: title.to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: ts.parse::<jiff::Timestamp>().unwrap(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let f = DateFilter::new(Task::fields().created_at());
    let expr = f.to_expr("2024-01-15").expect("date-only must build");
    let mut db2 = db.clone();
    let mut rows = Task::filter(expr).exec(&mut db2).await.unwrap();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(
        rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
        vec!["Morning", "Night"],
        "date-only must match the whole UTC day"
    );
    // Exact RFC3339 instants still match exactly.
    let expr = f
        .to_expr("2024-01-15T09:30:00Z")
        .expect("rfc3339 must build");
    let rows = Task::filter(expr).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(f.to_expr("not-a-date").is_none());
}

#[test]
fn date_filter_recovers_plus_offsets_mangled_by_query_decode() {
    let f = DateFilter::new(Task::fields().created_at());
    // `+02:00` arrives as ` 02:00` after `+`-as-space decoding.
    assert!(f.to_expr("2024-01-15T09:30:00 02:00").is_some());
    assert!(f.to_expr("2024-01-15T09:30:00+02:00").is_some());
    assert!(f.to_expr("not-a-date").is_none());
    assert!(f.to_expr("").is_none());
}

#[test]
fn date_filter_on_the_last_representable_day_does_not_panic() {
    let f = DateFilter::new(Task::fields().created_at());
    assert!(
        f.to_expr("9999-12-30").is_some(),
        "the last day builds a lower-bounded predicate"
    );
    assert!(
        f.to_expr("9999-12-31").is_none(),
        "a day past the maximum is invalid, not a panic"
    );
    assert!(
        f.to_expr("-009999-01-01").is_none(),
        "a day before the minimum is invalid"
    );
}

#[test]
fn a_variant_filter_accepts_only_a_declared_variant() {
    let f = vehicule_filter();
    assert_eq!(f.name(), "vehicule");
    assert_eq!(f.label(), "Véhicule");
    assert!(f.to_expr("").is_none(), "empty yields no filter");
    assert!(f.to_expr("   ").is_none(), "blank yields no filter");
    assert!(
        f.to_expr("Avion").is_none(),
        "unknown yields no filter, got {:?}",
        f.to_expr("Avion").is_some()
    );
    assert!(f.to_expr("Auto").is_some(), "known variant must match");
    assert!(f.to_expr("Moto").is_some(), "known variant must match");
    // Whitespace trims like SelectFilter.
    assert!(f.to_expr("  Moto  ").is_some());
}

#[test]
fn a_select_filter_accepts_only_its_allow_list() {
    // Core owns the predicate contract.
    let f = SelectFilter::new(
        crate::lens!(Task.status),
        vec!["draft".to_string(), "published".to_string()],
    );
    assert_eq!(f.name(), "status");
    assert!(f.to_expr("published").is_some());
    assert!(f.to_expr("draft").is_some());
    assert!(f.to_expr("").is_none(), "empty yields no filter");
    assert!(f.to_expr("   ").is_none(), "blank yields no filter");
    assert!(
        f.to_expr("unknown").is_none(),
        "off-allowlist yields no filter"
    );
    assert!(
        f.to_expr("Published").is_none(),
        "allowlist is case-sensitive"
    );
    // Whitespace trims before the allowlist check.
    assert!(f.to_expr("  published  ").is_some());
}

#[test]
fn a_ternary_filter_reads_true_and_false_and_ignores_the_rest() {
    let f = TernaryFilter::new(Task::fields().featured());
    assert_eq!(f.name(), "featured");
    assert!(f.to_expr("true").is_some());
    assert!(f.to_expr("false").is_some());
    assert!(f.to_expr("").is_none(), "empty yields no filter");
    assert!(f.to_expr("all").is_none(), "`all` yields no filter");
    assert!(f.to_expr("yes").is_none());
    assert!(f.to_expr("  true  ").is_some(), "value trims");
    assert!(f.is_noop_value("all"), "`all` is the documented no-op");
}

#[tokio::test]
async fn query_filter_hits_only_the_variant() {
    let mut db = memory_db(toasty::models!(Driver)).await;
    // Same shared `puissance` value in both variants — the variant gate
    // must exclude the other variant (acceptance).
    toasty::create!(Driver {
        name: "Alice",
        vehicule: Vehicule::Auto {
            puissance: "80".to_string(),
            seats: "4".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Driver {
        name: "Bob",
        vehicule: Vehicule::Moto {
            puissance: "80".to_string(),
            cc: "600".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Driver {
        name: "Cara",
        vehicule: Vehicule::Auto {
            puissance: "120".to_string(),
            seats: "2".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .unwrap();

    let f = vehicule_filter();
    let mut db2 = db.clone();
    let motos = Driver::filter(f.to_expr("Moto").unwrap())
        .exec(&mut db2)
        .await
        .unwrap();
    assert_eq!(
        motos.len(),
        1,
        "Moto filter must hit one row, got {motos:?}"
    );
    assert_eq!(motos[0].name, "Bob");

    let autos = Driver::filter(f.to_expr("Auto").unwrap())
        .exec(&mut db2)
        .await
        .unwrap();
    assert_eq!(
        autos.len(),
        2,
        "Auto filter must hit two rows, got {autos:?}"
    );

    // Composes with search via AND (the loader's contract).
    let search = Driver::fields().name().starts_with("B".to_string());
    let both = search.and(f.to_expr("Moto").unwrap());
    let rows = Driver::filter(both).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Bob");

    // Same shared value, other variant excluded.
    let both = Driver::fields()
        .name()
        .starts_with("A".to_string())
        .and(f.to_expr("Moto").unwrap());
    let rows = Driver::filter(both).exec(&mut db2).await.unwrap();
    assert!(
        rows.is_empty(),
        "Alice shares puissance 80 but is Auto, must not match Moto: {rows:?}"
    );
}

/// A civil date, a civil date-time and a nullable timestamp each filter by calendar day.
#[tokio::test]
async fn date_filter_matches_a_day_on_each_date_type() {
    use jiff::civil::date;

    let mut db = memory_db(toasty::models!(Event)).await;
    for (title, day, hour, closed) in [
        ("First", 15, 0, Some("2024-01-15T23:59:59Z")),
        ("Second", 15, 23, None),
        ("Third", 16, 0, Some("2024-01-16T00:00:00Z")),
    ] {
        toasty::create!(Event {
            title: title.to_string(),
            day: date(2024, 1, day),
            starts_at: date(2024, 1, day).at(hour, 0, 0, 0),
            closed_at: closed.map(|ts| ts.parse::<jiff::Timestamp>().unwrap()),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    async fn titles(db: &toasty::Db, expr: Expr<bool>) -> Vec<String> {
        let mut rows = Event::filter(expr).exec(&mut db.clone()).await.unwrap();
        rows.sort_by(|a, b| a.title.cmp(&b.title));
        rows.into_iter().map(|row| row.title).collect()
    }

    let day = DateFilter::new(Event::fields().day());
    let starts_at = DateFilter::new(Event::fields().starts_at());
    let closed_at = DateFilter::new(Event::fields().closed_at());
    assert_eq!(
        titles(&db, day.to_expr("2024-01-15").unwrap()).await,
        ["First", "Second"]
    );
    assert_eq!(
        titles(&db, starts_at.to_expr("2024-01-15").unwrap()).await,
        ["First", "Second"]
    );
    assert_eq!(
        titles(&db, starts_at.to_expr("2024-01-15T23:00").unwrap()).await,
        ["Second"],
        "a date-time names one value"
    );
    assert_eq!(
        titles(&db, closed_at.to_expr("2024-01-15").unwrap()).await,
        ["First"]
    );
    assert!(day.to_expr("not-a-date").is_none());
}
