//! [`RecordTitle`]: the column a resource names its records by.

use toasty::stmt::Expr;

use crate::{Lens, form::FormScalar, table::contains_expr};

/// A record's title column, as [`ResourceDef::record_title`] declares it.
pub(crate) struct RecordTitle<M> {
    read: Box<dyn Fn(&M) -> String + Send + Sync>,
    search: TitleSearch,
    /// The model's root field the title reads.
    pub(crate) field: Option<String>,
}

/// The substring match on a title column for a search term.
type TitleSearch = Box<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

impl<M: toasty::schema::Model + Send + Sync + 'static> RecordTitle<M> {
    pub(super) fn new<T>(lens: Lens<M, T>) -> Self
    where
        T: FormScalar + toasty::schema::Field<Inner = String> + Send + Sync + 'static,
    {
        let path = lens.clone();
        let field = crate::toasty_compat::model::root_field::<M>(
            &crate::toasty_compat::model::ModelPath::of(lens.path()),
        );
        Self {
            read: Box::new(move |record| lens.read(record).to_form()),
            search: Box::new(move |term| contains_expr(&path, term)),
            field,
        }
    }

    /// The column's value on `record`, or `None` when it is empty.
    pub(crate) fn read(&self, record: &M) -> Option<String> {
        Some((self.read)(record)).filter(|title| !title.trim().is_empty())
    }

    /// The substring match on the column for `term`, or `None` for a blank term.
    pub(crate) fn search_expr(&self, term: &str) -> Option<Expr<bool>> {
        (self.search)(term)
    }
}
