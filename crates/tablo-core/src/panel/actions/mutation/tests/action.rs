use toasty::Db;

use super::*;
use crate::{
    Ability, Action, ResourceDef,
    panel::test_support::{Dummy, dummy_table, mount, panel_for},
};

struct Rename;

impl Action<Hidden> for Rename {
    const NAME: &'static str = "rename";

    fn label(_cx: &Cx) -> String {
        "Rename".to_string()
    }

    async fn run(_: &Cx, _: &[Dummy], _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

/// A resource whose policy allows `View` alone: it refuses `ViewAny` and `RunAny`.
struct Hidden;

impl Resource for Hidden {
    type Model = Dummy;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("dummies")
            .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::View(_)))
            .table(dummy_table())
            .action::<Rename>()
    }
}

#[tokio::test]
async fn an_unknown_action_answers_a_refused_caller_like_a_known_one() {
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(db, panel_for::<Hidden>()).expect("panel builds");
    for name in ["rename", "missing"] {
        let response = router
            .handle(
                http::Request::builder()
                    .uri(format!("/admin/dummies/-/actions/{name}"))
                    .method(http::Method::POST)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(
            response.status(),
            http::StatusCode::FORBIDDEN,
            "`{name}` answers 403 without `ViewAny` or `RunAny`, naming no action"
        );
    }
}
