//! GH #306: dropping an undrained streamed list body frees its pooled connection.

use tablo_core::resource::{Resource, Table, TextColumn};
use topcoat::context::Cx;
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, panel};

#[derive(Debug, Clone, toasty::Model)]
struct PoolDummy {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

struct PoolResource;

impl Resource for PoolResource {
    type Model = PoolDummy;
    type Form = tablo_core::NoForm<Self::Model>;

    fn slug() -> String {
        "dummies".to_string()
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &PoolDummy) -> bool {
        true
    }

    fn table(_cx: &Cx) -> Table<PoolDummy> {
        Table::new(
            |d: &PoolDummy| d.id.to_string(),
            TextColumn::r#for(PoolDummy::fields().name(), |d: &PoolDummy| d.name.clone()),
        )
        .paginate(25)
    }
}

#[tokio::test]
async fn a_dropped_list_body_frees_the_pool_for_the_next_request() {
    let mut db = memory_db(toasty::models!(PoolDummy)).await;
    toasty::create!(PoolDummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed a row");
    let router = panel(db)
        .resource::<PoolResource>()
        .build()
        .expect("panel builds");

    // Abandon the streamed list body without draining it.
    let first = get(&router, "/admin/dummies").await;
    assert!(first.status().is_success());
    drop(first.into_body());

    // A regression hangs here; the timeout fails the test instead of stalling the suite.
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        get(&router, "/admin/dummies"),
    )
    .await
    .expect("the next request must not wait on the pool");
    assert!(second.status().is_success());
    let html = body_string(second).await;
    assert!(
        html.contains("Ada"),
        "the next request must render rows, got {html}"
    );
}
