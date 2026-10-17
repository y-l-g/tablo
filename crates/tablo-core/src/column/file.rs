//! [`FileColumn`]: an uploaded file's path rendered as a link to the file.

use derive_where::derive_where;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::Lens;

/// A column of a `String` field holding an uploaded file's path, rendered as a link to the file.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Doc { #[key] #[auto] id: uuid::Uuid, path: String }
/// tablo_core::FileColumn::new(tablo_core::lens!(Doc.path));
/// ```
///
/// The cell links a rooted path or an absolute `http(s)` URL, the same rule a
/// [`Field::file`](crate::Field::file) control applies, and shows any other value as text.
#[derive_where(Clone, Debug)]
pub struct FileColumn<M> {
    lens: Lens<M, String>,
    base: ColumnBase,
}

impl<M> FileColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` field `lens` reads.
    pub fn new(lens: Lens<M, String>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
        }
    }

    base_builders!();
}

impl<M> Column<M> for FileColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_column_methods!(binding);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).clone()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let path = self.lens.read(row);
        if path.trim().is_empty() {
            return crate::schema::value_cell(cx, path);
        }
        crate::schema::stored_upload(cx, path)
    }
}
