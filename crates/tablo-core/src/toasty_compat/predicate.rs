//! The root fields a predicate reads, off its untyped expression: Toasty's typed `Expr` names
//! no field it reads.

use toasty_core::stmt::{ExprReference, Query, Visit, With, visit};

/// The names of `M`'s root fields `predicate` reads, and whether it reads the row some other way:
/// a whole-record reference, a database column, or a query scope the walk does not follow.
///
/// A relation predicate, such as `Post::fields().comments().any(..)`, reads the relation field it
/// starts at; its subquery's own fields belong to the related model.
pub(crate) fn root_fields<M: toasty::schema::Model>(
    predicate: &toasty::stmt::Expr<bool>,
) -> (Vec<String>, bool) {
    let untyped: toasty_core::stmt::Expr = predicate.clone().into();
    let mut walk = Walk::default();
    walk.visit_expr(&untyped);
    let model = M::schema();
    let names: Vec<Option<String>> = walk
        .fields
        .iter()
        .map(|index| model.as_root()?.fields.get(*index)?.name.app.clone())
        .collect();
    let unresolved = walk.unresolved || names.contains(&None);
    (names.into_iter().flatten().collect(), unresolved)
}

#[derive(Default)]
struct Walk {
    /// How many subqueries deep the walk is: a reference whose nesting equals it names the root.
    depth: usize,
    fields: Vec<usize>,
    unresolved: bool,
}

impl Visit for Walk {
    fn visit_stmt_query(&mut self, query: &Query) {
        self.depth += 1;
        visit::visit_stmt_query(self, query);
        self.depth -= 1;
    }

    fn visit_with(&mut self, _with: &With) {
        // A common table expression nests scopes the depth does not count.
        self.unresolved = true;
    }

    fn visit_expr_reference(&mut self, reference: &ExprReference) {
        match *reference {
            ExprReference::Field { nesting, index } if nesting == self.depth => {
                if !self.fields.contains(&index) {
                    self.fields.push(index);
                }
            }
            // A subquery's own model.
            ExprReference::Field { nesting, .. } | ExprReference::Model { nesting }
                if nesting < self.depth => {}
            _ => self.unresolved = true,
        }
    }
}

#[cfg(test)]
mod tests;
