use super::*;
use crate::test_support::memory_db;

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
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
    let expr = f
        .to_expr(&"2024-01-15".into())
        .expect("date-only must build");
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
        .to_expr(&"2024-01-15T09:30:00Z".into())
        .expect("rfc3339 must build");
    let rows = Task::filter(expr).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(f.to_expr(&"not-a-date".into()).is_none());
}

#[test]
fn date_filter_recovers_plus_offsets_mangled_by_query_decode() {
    let f = DateFilter::new(Task::fields().created_at());
    // `+02:00` arrives as ` 02:00` after `+`-as-space decoding.
    assert!(f.to_expr(&"2024-01-15T09:30:00 02:00".into()).is_some());
    assert!(f.to_expr(&"2024-01-15T09:30:00+02:00".into()).is_some());
    assert!(f.to_expr(&"not-a-date".into()).is_none());
    assert!(f.to_expr(&"".into()).is_none());
}

#[test]
fn date_filter_on_the_last_representable_day_does_not_panic() {
    let f = DateFilter::new(Task::fields().created_at());
    assert!(
        f.to_expr(&"9999-12-30".into()).is_some(),
        "the last day builds a lower-bounded predicate"
    );
    assert!(
        f.to_expr(&"9999-12-31".into()).is_none(),
        "a day past the maximum is invalid, not a panic"
    );
    assert!(
        f.to_expr(&"-009999-01-01".into()).is_none(),
        "a day before the minimum is invalid"
    );
}

#[test]
fn a_variant_filter_accepts_only_a_declared_variant() {
    let f = vehicule_filter();
    assert_eq!(f.name(), "vehicule");
    assert_eq!(f.label(), "Véhicule");
    assert!(f.to_expr(&"".into()).is_none(), "empty yields no filter");
    assert!(f.to_expr(&"   ".into()).is_none(), "blank yields no filter");
    assert!(
        f.to_expr(&"Avion".into()).is_none(),
        "unknown yields no filter, got {:?}",
        f.to_expr(&"Avion".into()).is_some()
    );
    assert!(
        f.to_expr(&"Auto".into()).is_some(),
        "known variant must match"
    );
    assert!(
        f.to_expr(&"Moto".into()).is_some(),
        "known variant must match"
    );
    // Whitespace trims like SelectFilter.
    assert!(f.to_expr(&"  Moto  ".into()).is_some());
}

#[test]
fn a_select_filter_accepts_only_its_allow_list() {
    // Core owns the predicate contract.
    let f = SelectFilter::new(
        crate::lens!(Task.status),
        vec!["draft".to_string(), "published".to_string()],
    );
    assert_eq!(f.name(), "status");
    assert!(f.to_expr(&"published".into()).is_some());
    assert!(f.to_expr(&"draft".into()).is_some());
    assert!(f.to_expr(&"".into()).is_none(), "empty yields no filter");
    assert!(f.to_expr(&"   ".into()).is_none(), "blank yields no filter");
    assert!(
        f.to_expr(&"unknown".into()).is_none(),
        "off-allowlist yields no filter"
    );
    assert!(
        f.to_expr(&"Published".into()).is_none(),
        "allowlist is case-sensitive"
    );
    // Whitespace trims before the allowlist check.
    assert!(f.to_expr(&"  published  ".into()).is_some());
}

#[test]
fn a_select_filter_trims_an_options_padding() {
    let f = SelectFilter::new(crate::lens!(Task.featured), [" true", "false"]);
    assert!(
        f.misdeclared().is_none(),
        "the declaration check already trims, so the option is declared clean"
    );
    assert!(
        f.to_expr(&"true".into()).is_some(),
        "the predicate must list the option the control offers"
    );
}

#[test]
fn a_ternary_filter_reads_true_and_false_and_ignores_the_rest() {
    let f = TernaryFilter::new(Task::fields().featured());
    assert_eq!(f.name(), "featured");
    assert!(f.to_expr(&"true".into()).is_some());
    assert!(f.to_expr(&"false".into()).is_some());
    assert!(f.to_expr(&"".into()).is_none(), "empty yields no filter");
    assert!(f.to_expr(&"all".into()).is_none(), "`all` yields no filter");
    assert!(f.to_expr(&"yes".into()).is_none());
    assert!(f.to_expr(&"  true  ".into()).is_some(), "value trims");
    assert!(
        f.is_noop_value(&"all".into()),
        "`all` is the documented no-op"
    );
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
    let motos = Driver::filter(f.to_expr(&"Moto".into()).unwrap())
        .exec(&mut db2)
        .await
        .unwrap();
    assert_eq!(
        motos.len(),
        1,
        "Moto filter must hit one row, got {motos:?}"
    );
    assert_eq!(motos[0].name, "Bob");

    let autos = Driver::filter(f.to_expr(&"Auto".into()).unwrap())
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
    let both = search.and(f.to_expr(&"Moto".into()).unwrap());
    let rows = Driver::filter(both).exec(&mut db2).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Bob");

    // Same shared value, other variant excluded.
    let both = Driver::fields()
        .name()
        .starts_with("A".to_string())
        .and(f.to_expr(&"Moto".into()).unwrap());
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
        titles(&db, day.to_expr(&"2024-01-15".into()).unwrap()).await,
        ["First", "Second"]
    );
    assert_eq!(
        titles(&db, starts_at.to_expr(&"2024-01-15".into()).unwrap()).await,
        ["First", "Second"]
    );
    assert_eq!(
        titles(&db, starts_at.to_expr(&"2024-01-15T23:00".into()).unwrap()).await,
        ["Second"],
        "a date-time names one value"
    );
    assert_eq!(
        titles(&db, closed_at.to_expr(&"2024-01-15".into()).unwrap()).await,
        ["First"]
    );
    assert!(day.to_expr(&"not-a-date".into()).is_none());
}

/// Several values select the records holding any of them; one value off the list selects none.
#[tokio::test]
async fn a_select_filter_matches_any_of_several_values() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status) in [
        ("Draft", "draft"),
        ("Review", "review"),
        ("Published", "published"),
    ] {
        toasty::create!(Task {
            title: title.to_string(),
            status: status.to_string(),
            featured: false,
            created_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let f = SelectFilter::new(crate::lens!(Task.status), ["draft", "review", "published"]);
    let mut rows = Task::filter(
        f.to_expr(&FilterValue::from("draft").with("published"))
            .unwrap(),
    )
    .exec(&mut db)
    .await
    .unwrap();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    assert_eq!(
        rows.into_iter().map(|row| row.title).collect::<Vec<_>>(),
        ["Draft", "Published"]
    );
    assert!(
        f.to_expr(&FilterValue::from("draft").with("archived"))
            .is_none()
    );
    assert!(
        f.to_expr(&FilterValue::from("draft").with_part("from", "x"))
            .is_none()
    );
}

/// A filter taking one value selects nothing from several values, and no filter reads a value
/// beside a part.
#[test]
fn a_single_value_filter_refuses_several_values_and_parts() {
    let ternary = TernaryFilter::new(Task::fields().featured());
    assert!(
        ternary
            .to_expr(&FilterValue::from("true").with("false"))
            .is_none()
    );
    let date = DateFilter::new(Task::fields().created_at());
    assert!(
        date.to_expr(&FilterValue::from("2024-01-15").with("2024-01-16"))
            .is_none()
    );
    let mixed = FilterValue::from("Auto").with_part("to", "2024-01-16");
    assert!(ternary.to_expr(&mixed).is_none());
    assert!(date.to_expr(&mixed).is_none());
    assert!(vehicule_filter().to_expr(&mixed).is_none());
}

/// A single-day date filter refuses a part: its one input renders a value, not a range, so the
/// applied filter would show nothing and vanish on the next submit.
#[test]
fn a_single_day_date_filter_refuses_a_range() {
    let filter = DateFilter::new(Task::fields().created_at());
    assert!(filter.to_expr(&"2024-01-15".into()).is_some());
    assert!(
        filter
            .to_expr(&FilterValue::new().with_part("from", "2024-01-15"))
            .is_none(),
        "a part needs the range control"
    );
    assert!(
        DateFilter::new(Task::fields().created_at())
            .range()
            .to_expr(&FilterValue::new().with_part("from", "2024-01-15"))
            .is_some(),
        "the range control renders it"
    );
}

/// A date filter's `from` and `to` parts bound a range of whole days, either alone leaving it
/// open; a reversed range, another part, or a value beside them selects nothing.
#[tokio::test]
async fn a_date_range_keeps_the_days_its_parts_bound() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, ts) in [
        ("Before", "2024-01-14T23:59:59Z"),
        ("First", "2024-01-15T00:00:00Z"),
        ("Last", "2024-01-16T23:59:59Z"),
        ("After", "2024-01-17T00:00:00Z"),
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
    let f = DateFilter::new(Task::fields().created_at()).range();
    let titles = async |value: FilterValue| {
        let expr = f.to_expr(&value).expect("the range builds");
        let mut rows = Task::filter(expr).exec(&mut db.clone()).await.unwrap();
        rows.sort_by_key(|row| row.created_at);
        rows.into_iter().map(|row| row.title).collect::<Vec<_>>()
    };
    let range = |from: Option<&str>, to: Option<&str>| {
        let value = FilterValue::new();
        let value = from.map_or(value.clone(), |from| value.clone().with_part("from", from));
        to.map_or(value.clone(), |to| value.with_part("to", to))
    };
    assert_eq!(
        titles(range(Some("2024-01-15"), Some("2024-01-16"))).await,
        ["First", "Last"]
    );
    assert_eq!(
        titles(range(Some("2024-01-16"), None)).await,
        ["Last", "After"]
    );
    assert_eq!(
        titles(range(None, Some("2024-01-15"))).await,
        ["Before", "First"]
    );
    assert_eq!(
        titles(range(
            Some("2024-01-15T00:00:01Z"),
            Some("2024-01-17T00:00:00Z")
        ))
        .await,
        ["Last", "After"],
        "an exact time bounds the range at itself"
    );
    assert_eq!(
        titles(range(Some("2024-01-15"), Some("2024-01-15"))).await,
        ["First"],
        "a one-day range is that day"
    );
    for refused in [
        range(Some("2024-01-16"), Some("2024-01-15")),
        range(Some("2024-01-17T00:00:00Z"), Some("2024-01-15T00:00:00Z")),
        range(Some("2024-01-17T00:00:00Z"), Some("2024-01-15")),
        range(Some("not-a-date"), None),
        range(Some("2024-01-15"), Some("not-a-date")),
        FilterValue::new().with_part("since", "2024-01-15"),
        FilterValue::from("2024-01-15").with_part("to", "2024-01-16"),
    ] {
        assert!(
            f.to_expr(&refused).is_none(),
            "{refused:?} must select nothing"
        );
    }
    assert!(
        f.to_expr(&range(None, Some("9999-12-30"))).is_some(),
        "the last representable day bounds nothing past it"
    );
}

/// A keyed option travels as its key, never its label, and several keys select the records any
/// of their predicates does.
#[tokio::test]
async fn a_query_filter_matches_its_keys_and_any_of_several() {
    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status, featured) in [
        ("Draft", "draft", false),
        ("Starred", "draft", true),
        ("Published", "published", false),
    ] {
        toasty::create!(Task {
            title: title.to_string(),
            status: status.to_string(),
            featured,
            created_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let f = QueryFilter::<Task>::new("pick", "Pick")
        .option_keyed(
            "starred",
            "Starred posts",
            Task::fields().featured().eq(true),
        )
        .option(
            "Published",
            Task::fields().status().eq("published".to_string()),
        );
    let titles = async |value: FilterValue| {
        let expr = f.to_expr(&value).expect("the options build");
        let mut rows = Task::filter(expr).exec(&mut db.clone()).await.unwrap();
        rows.sort_by(|a, b| a.title.cmp(&b.title));
        rows.into_iter().map(|row| row.title).collect::<Vec<_>>()
    };
    assert_eq!(titles("starred".into()).await, ["Starred"]);
    assert_eq!(
        titles(FilterValue::from("starred").with("Published")).await,
        ["Published", "Starred"]
    );
    assert!(f.to_expr(&"Starred posts".into()).is_none());
    assert!(
        f.to_expr(&FilterValue::from("starred").with("archived"))
            .is_none()
    );
}

/// The built-in filters take a label over the field's, and a ternary filter its options' labels.
#[test]
fn a_built_in_filter_takes_a_label() {
    let select = SelectFilter::new(crate::lens!(Task.status), ["draft"]);
    assert_eq!(Filter::<Task>::label(&select), "Status");
    assert_eq!(Filter::<Task>::label(&select.label("State")), "State");
    let ternary = TernaryFilter::new(Task::fields().featured()).label("Starred");
    assert_eq!(Filter::<Task>::label(&ternary), "Starred");
    let date = DateFilter::new(Task::fields().created_at()).label("Created");
    assert_eq!(Filter::<Task>::label(&date), "Created");
}

#[derive(Debug, Clone, toasty::Model)]
struct Writer {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
struct Article {
    #[key]
    #[auto]
    id: uuid::Uuid,
    writer_id: uuid::Uuid,
}

/// The writers, but for the one named "Hidden", which the user may not view.
struct Writers;

impl crate::schema::OptionSource for Writers {
    type Model = Writer;
    fn label(_cx: &topcoat::context::Cx, writer: &Writer) -> String {
        writer.name.clone()
    }
    fn scoped_query(
        _cx: &topcoat::context::Cx,
    ) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Writer>>> {
        Ok(toasty::stmt::Query::all())
    }
    fn allows(_cx: &topcoat::context::Cx, ability: crate::Ability<'_, Writer>) -> bool {
        match ability {
            crate::Ability::View(writer) => writer.name != "Hidden",
            ability => ability.is_read(),
        }
    }
}

/// A relationship filter admits the keys of the writers the user may view, under the option cap
/// and past it, and refuses a hidden writer's key as it refuses a key no writer holds.
#[tokio::test]
async fn a_relationship_filter_admits_only_the_keys_its_source_shows() {
    for extra in [0, crate::schema::MAX_RELATIONSHIP_OPTIONS] {
        let mut db = memory_db(toasty::models!(Writer, Article)).await;
        let mut writer = async |name: String| {
            toasty::create!(Writer { name })
                .exec(&mut db)
                .await
                .unwrap()
                .id
                .to_string()
        };
        let (shown, hidden) = (writer("Shown".into()).await, writer("Hidden".into()).await);
        for i in 0..extra {
            writer(format!("Writer {i}")).await;
        }
        let cx = topcoat::context::CxTestBuilder::new()
            .app_context(db)
            .build();
        let filter = SelectFilter::relationship::<Writers>(crate::lens!(Article.writer_id));
        let admits = async |keys: &[&str]| {
            let value = keys
                .iter()
                .fold(FilterValue::new(), |value, key| value.with(*key));
            filter.admits(&cx, &value).await.unwrap()
        };
        let nobody = uuid::Uuid::new_v4().to_string();
        assert!(admits(&[&shown]).await, "{extra} more writers");
        assert!(!admits(&[&hidden]).await, "{extra} more writers");
        assert!(!admits(&[&nobody]).await, "{extra} more writers");
        assert!(!admits(&[&shown, &hidden]).await, "{extra} more writers");
        // A key matches as the option value spells it, under the cap and past it.
        assert!(
            !admits(&[&shown.to_uppercase()]).await,
            "{extra} more writers"
        );
    }
}
