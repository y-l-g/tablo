//! The time zone a request reads and shows wall-clock times in, from the signed-in user, else the
//! panel: a timestamp's control, in a repeater's rows too, its `DateColumn` day and its
//! `DateFilter` days.

use jiff::{
    Timestamp,
    tz::{Offset, TimeZone},
};
use tablo::{
    Allow, Auth, DateColumn, DateFilter, Panel, PanelUser, Resource, ResourceDef, Table,
    TextColumn,
    auth::{AuthSession, Authenticator},
    lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_string, input_value, mount, rows, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Shift {
    #[key]
    #[auto]
    id: Uuid,
    label: String,
    starts_at: Timestamp,
    #[document]
    #[default(Vec::<Break>::new())]
    breaks: Vec<Break>,
}

/// One break in a shift: an item of a `#[document]` list.
#[derive(Debug, Clone, toasty::Embed, tablo::FormInput)]
struct Break {
    starts_at: Timestamp,
}

#[derive(tablo::RecordForm)]
#[form(model = Shift)]
struct ShiftForm {
    label: String,
    starts_at: Timestamp,
    #[form(repeat)]
    breaks: Vec<Break>,
}

struct ShiftResource;

impl Resource for ShiftResource {
    type Model = Shift;
    type Form = ShiftForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().slug("shifts").policy(Allow).table(
            Table::new((
                TextColumn::new(lens!(Shift.label)),
                DateColumn::new(lens!(Shift.starts_at)),
            ))
            .filters(DateFilter::new(Shift::fields().starts_at())),
        )
    }
}

/// An account whose own zone is a fixed offset from UTC, in hours, or none.
#[derive(Debug, Clone, toasty::Model)]
struct Planner {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    offset_hours: Option<i8>,
}

impl PanelUser for Planner {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn time_zone(&self) -> Option<TimeZone> {
        self.offset_hours.map(hours)
    }
}

struct PlannerAuth;

impl Authenticator for PlannerAuth {
    type User = Planner;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<Planner>> {
        Ok(None)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<Planner>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        Ok(Planner::filter(Planner::fields().id().eq(id))
            .first()
            .exec(&mut tablo::db::db(cx))
            .await?)
    }
}

/// The zone `hours` ahead of UTC.
fn hours(hours: i8) -> TimeZone {
    TimeZone::fixed(Offset::from_hours(hours).unwrap())
}

async fn db() -> Db {
    test_db(toasty::models!(Shift, Planner, AuthSession)).await
}

/// The panel two hours ahead of UTC, signing planners in when `auth` says so.
fn router(db: &Db, auth: Auth) -> Router {
    router_in(db, auth, hours(2))
}

fn router_in(db: &Db, auth: Auth, zone: TimeZone) -> Router {
    mount(
        db.clone(),
        Panel::new("admin")
            .auth(auth)
            .time_zone(zone)
            .resource::<ShiftResource>(),
    )
    .expect("panel builds")
}

async fn shift(db: &Db, label: &str, starts_at: &str) -> Shift {
    toasty::create!(Shift {
        label: label.to_string(),
        starts_at: starts_at.parse::<Timestamp>().unwrap(),
        breaks: Vec::<Break>::new(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn stored(db: &Db, label: &str) -> Timestamp {
    stored_shift(db, label).await.starts_at
}

async fn stored_shift(db: &Db, label: &str) -> Shift {
    Shift::filter(Shift::fields().label().eq(label.to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the shift")
}

/// A wall-clock time entered in the panel's zone stores the instant it names there, and the edit
/// form shows the stored instant back in that zone; saving it untouched keeps its seconds.
#[tokio::test]
async fn a_timestamp_control_reads_and_shows_the_panels_zone() {
    let db = db().await;
    let router = router(&db, Auth::disabled());
    let client = TestClient::new(&router);

    let response = client
        .submit(
            "/admin/shifts/-/create",
            "label=Evening&starts_at=2026-10-20T14%3A00",
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        stored(&db, "Evening").await,
        "2026-10-20T12:00:00Z".parse::<Timestamp>().unwrap()
    );

    let early = shift(&db, "Early", "2026-10-20T05:06:07Z").await;
    let edit = format!("/admin/shifts/{}/edit", early.id);
    let html = body_string(client.get(&edit).await).await;
    assert_eq!(
        input_value(&html, "starts_at").as_deref(),
        Some("2026-10-20T07:06")
    );
    let response = client
        .submit(&edit, "label=Early&starts_at=2026-10-20T07%3A06")
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        stored(&db, "Early").await,
        "2026-10-20T05:06:07Z".parse::<Timestamp>().unwrap(),
        "the untouched timestamp"
    );
}

/// A signed-in user's own zone wins over the panel's; a user naming none reads the panel's.
#[tokio::test]
async fn a_users_zone_wins_over_the_panels() {
    let db = db().await;
    let router = router(&db, Auth::custom(PlannerAuth));
    for (name, offset_hours, expected) in [
        ("Ada", Some(-5), "2026-10-20T19:00:00Z"),
        ("Grace", None, "2026-10-20T12:00:00Z"),
    ] {
        let planner = toasty::create!(Planner {
            name: name.to_string(),
            offset_hours,
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
        let client = TestClient::new(&router)
            .sign_in(&db, "admin", &planner)
            .await;
        let response = client
            .submit(
                "/admin/shifts/-/create",
                &format!("label={name}&starts_at=2026-10-20T14%3A00"),
            )
            .await;
        assert_eq!(response.status(), 303, "{name}");
        assert_eq!(
            stored(&db, name).await,
            expected.parse::<Timestamp>().unwrap(),
            "{name}"
        );
    }
}

/// A timestamp's calendar day, in its column and in its filter, is the day in the request's zone:
/// 23:30 UTC on the 20th is already the 21st two hours east.
#[tokio::test]
async fn a_timestamps_day_is_the_day_in_the_zone() {
    let db = db().await;
    let router = router(&db, Auth::disabled());
    let client = TestClient::new(&router);
    shift(&db, "Late", "2026-10-20T23:30:00Z").await;
    shift(&db, "Evening", "2026-10-20T21:00:00Z").await;

    let html = body_string(client.get("/admin/shifts").await).await;
    let listed = rows(&html);
    for (label, day) in [("Evening", "Oct 20, 2026"), ("Late", "Oct 21, 2026")] {
        assert!(
            listed
                .iter()
                .any(|row| row.cells.iter().any(|cell| cell == label)
                    && row.cells.iter().any(|cell| cell == day)),
            "{label} on {day}: {listed:?}"
        );
    }

    for (day, expected) in [("2026-10-20", "Evening"), ("2026-10-21", "Late")] {
        let html = body_string(
            client
                .get(&format!("/admin/shifts?f.starts_at={day}"))
                .await,
        )
        .await;
        let listed = rows(&html);
        assert_eq!(listed.len(), 1, "{day}: {listed:?}");
        assert!(
            listed[0].cells.iter().any(|cell| cell == expected),
            "{day}: {listed:?}"
        );
    }
}

/// A stored instant in the hour a zone repeats shows as a wall-clock time that also names the
/// hour's earlier instant: posting it back untouched keeps the stored one, in a repeater's row as
/// in a top-level control.
#[tokio::test]
async fn an_untouched_time_in_a_repeated_hour_keeps_its_instant() {
    let db = db().await;
    let paris = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
    let router = router_in(&db, Auth::disabled(), paris);
    let client = TestClient::new(&router);
    // 02:30 CET, after the clocks went back from 03:00 CEST.
    let second = "2026-10-25T01:30:00Z".parse::<Timestamp>().unwrap();
    let night = toasty::create!(Shift {
        label: "Night".to_string(),
        starts_at: second,
        breaks: vec![Break { starts_at: second }],
    })
    .exec(&mut db.clone())
    .await
    .unwrap();

    let edit = format!("/admin/shifts/{}/edit", night.id);
    let html = body_string(client.get(&edit).await).await;
    for key in ["starts_at", "breaks.0.starts_at"] {
        assert_eq!(
            input_value(&html, key).as_deref(),
            Some("2026-10-25T02:30"),
            "{key}"
        );
    }
    let response = client
        .submit(
            &edit,
            "label=Night&starts_at=2026-10-25T02%3A30\
             &breaks=0&breaks.0.starts_at=2026-10-25T02%3A30",
        )
        .await;
    assert_eq!(response.status(), 303);
    let night = stored_shift(&db, "Night").await;
    assert_eq!(night.starts_at, second, "the untouched timestamp");
    assert_eq!(night.breaks[0].starts_at, second, "the untouched row");
}
