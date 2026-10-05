//! Dropping an undrained streamed list body frees its pooled connection.

use tablo_core::{ReadOnly, Resource, ResourceDef, Table, TextColumn, lens};
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, mount, panel};

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

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("dummies")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(PoolDummy.name))).paginate(25))
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
    let router = mount(db, panel().resource::<PoolResource>()).expect("panel builds");

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
