use toasty_core::stmt::{Visit, visit};

use super::*;
use crate::test_support::{DummyUser, tableless_db};

/// Whether each `LIKE` in `predicate` is case-insensitive, in visit order.
fn like_cases(predicate: Expr<bool>) -> Vec<bool> {
    struct Cases(Vec<bool>);
    impl Visit for Cases {
        fn visit_expr_like(&mut self, like: &ExprLike) {
            self.0.push(like.case_insensitive);
            visit::visit_expr_like(self, like);
        }
    }
    let mut cases = Cases(Vec::new());
    cases.visit_expr(&toasty_core::stmt::Expr::from(predicate));
    cases.0
}

fn two_column_search() -> Expr<bool> {
    let fields = DummyUser::fields();
    fields
        .name()
        .like_with_escape("%ada%".to_string(), '\\')
        .or(fields.email().like_with_escape("%ada%".to_string(), '\\'))
}

#[test]
fn ilike_reaches_every_like_of_an_or() {
    assert_eq!(like_cases(two_column_search()), [false, false]);
    assert_eq!(like_cases(ilike(two_column_search())), [true, true]);
}

/// SQLite has no `ILIKE`, and Toasty refuses one there, so its search keeps `LIKE`.
#[tokio::test]
async fn a_driver_without_ilike_keeps_like() {
    let db = tableless_db(toasty::models!(DummyUser)).await;
    assert_eq!(
        like_cases(case_insensitive_search(&db, two_column_search())),
        [false, false]
    );
}
