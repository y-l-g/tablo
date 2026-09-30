use toasty::Db;

use super::*;

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

fn vehicule_filter() -> VariantFilter<Driver> {
    VariantFilter::r#for(
        "vehicule",
        "Véhicule",
        vec![
            ("Auto".to_string(), Driver::fields().vehicule().is_auto()),
            ("Moto".to_string(), Driver::fields().vehicule().is_moto()),
        ],
    )
}

#[tokio::test]
async fn date_filter_date_only_matches_whole_day() {
    let mut db = Db::builder()
        .models(toasty::models!(Task))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
    let f = DateFilter::r#for(Task::fields().created_at());
    let expr = f.to_expr("2024-01-15").expect("date-only must build");
    let mut db2 = db.clone();
    let mut rows = Task::filter(expr).exec(&mut db2).await.unwrap();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(
        rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
        vec!["Morning", "Night"],
        "date-only must match the whole UTC day (GH #93)"
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
    let f = DateFilter::r#for(Task::fields().created_at());
    // `+02:00` arrives as ` 02:00` after `+`-as-space decoding.
    assert!(f.to_expr("2024-01-15T09:30:00 02:00").is_some());
    assert!(f.to_expr("2024-01-15T09:30:00+02:00").is_some());
    assert!(f.to_expr("not-a-date").is_none());
    assert!(f.to_expr("").is_none());
}

#[test]
fn date_filter_on_the_last_representable_day_does_not_panic() {
    let f = DateFilter::r#for(Task::fields().created_at());
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
fn variant_filter_to_expr_contract() {
    let f = vehicule_filter();
    assert_eq!(f.name(), "vehicule");
    assert_eq!(f.label_str(), "Véhicule");
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
    // Via the Filter enum + IntoFilters seam.
    let via_enum: Filter<Driver> = f.clone().into();
    assert_eq!(via_enum.name(), "vehicule");
    assert!(via_enum.to_expr("Moto").is_some());
    assert!(via_enum.to_expr("nope").is_none());
    let vec = f.into_filters();
    assert_eq!(vec.len(), 1);
}

#[test]
fn select_filter_to_expr_contract() {
    // GH #136: the only direct SelectFilter coverage lived in the
    // showcase (`table_state_parses_filters_and_filter_expr`); core owns
    // the predicate contract, the showcase owns HTTP wiring.
    let f = SelectFilter::r#for(
        Task::fields().status(),
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
    // Via the Filter enum seam.
    let via_enum: Filter<Task> = f.clone().into();
    assert_eq!(via_enum.name(), "status");
    assert!(via_enum.to_expr("published").is_some());
    assert!(via_enum.to_expr("nope").is_none());
}

#[test]
fn ternary_filter_to_expr_contract() {
    // GH #136: same relocation as the select contract above.
    let f = TernaryFilter::r#for(Task::fields().featured());
    assert_eq!(f.name(), "featured");
    assert!(f.to_expr("true").is_some());
    assert!(f.to_expr("false").is_some());
    assert!(f.to_expr("").is_none(), "empty yields no filter");
    assert!(f.to_expr("all").is_none(), "`all` yields no filter");
    assert!(f.to_expr("yes").is_none());
    assert!(f.to_expr("  true  ").is_some(), "value trims");
    let via_enum: Filter<Task> = f.clone().into();
    assert_eq!(via_enum.name(), "featured");
    assert!(via_enum.to_expr("true").is_some());
    assert!(via_enum.to_expr("all").is_none());
}

#[tokio::test]
async fn variant_filter_hits_only_the_variant() {
    let mut db = Db::builder()
        .models(toasty::models!(Driver))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
