//! [`ImageColumn`]: an uploaded image's path rendered as a thumbnail.

use derive_where::derive_where;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::{Lens, values::FormScalar};

/// A column of a `String` field holding an uploaded image's path, rendered as a thumbnail.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Product { #[key] #[auto] id: uuid::Uuid, image: String }
/// tablo_core::ImageColumn::new(tablo_core::lens!(Product.image)).circular();
/// ```
///
/// A rooted path, such as an upload's, shows as a 40px square thumbnail; an empty value shows
/// the table's dash, and any other value its text. An absolute `http(s)` URL shows as an image
/// only once [`remote`](Self::remote) allows it.
#[derive_where(Clone, Debug)]
pub struct ImageColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    size: u16,
    circular: bool,
    remote: bool,
}

impl<M, T> ImageColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` or `Option<String>` field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: toasty::schema::Field<Inner = String>,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            size: 40,
            circular: false,
            remote: false,
        }
    }

    /// Show the thumbnail `pixels` wide and tall.
    pub fn size(mut self, pixels: u16) -> Self {
        self.size = pixels;
        self
    }

    /// Crop the thumbnail to a circle, for an avatar.
    pub fn circular(mut self) -> Self {
        self.circular = true;
        self
    }

    /// Also show an absolute `http(s)` URL as an image. Every browser that renders the table
    /// then loads it from that host as the page opens, so whoever can write the field learns who
    /// views the list, and when: allow it for a field only trusted users write.
    pub fn remote(mut self) -> Self {
        self.remote = true;
        self
    }

    base_builders!();
}

impl<M, T> Column<M> for ImageColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_form()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let src = self.text(cx, row).trim().to_string();
        let shown = if self.remote {
            crate::schema::is_linkable(&src)
        } else {
            crate::schema::is_rooted(&src)
        };
        if !shown {
            return crate::schema::value_cell(cx, &src);
        }
        let src = crate::schema::file_href(cx, &src);
        let class = if self.circular {
            "shrink-0 rounded-full object-cover ring-1 ring-foreground/10"
        } else {
            "shrink-0 rounded-md object-cover ring-1 ring-foreground/10"
        };
        let size = self.size.to_string();
        view! {
            cx =>
            <img
                src=(src)
                alt=""
                loading="lazy"
                referrerpolicy="no-referrer"
                width=(size.clone())
                height=(size)
                class=(class)
            >
        }
        .boxed()
    }
}

impl<M, T> crate::column::ColumnInternals<M> for ImageColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);
}
