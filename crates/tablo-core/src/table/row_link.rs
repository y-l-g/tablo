//! [`Table::row_link`]: a link among a row's actions.

use std::sync::Arc;

use derive_where::derive_where;
use topcoat::icon::IconData;

use super::Table;
use crate::{Reads, Text};

/// A link among a row's actions, such as a product's storefront page: unlike an
/// [`Action`](crate::Action), it posts nothing and runs no policy check, so it suits only a page
/// the user may already open.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Product { #[key] #[auto] id: uuid::Uuid, name: String, slug: String }
/// use tablo_core::{Reads, RowLink, Table, TextColumn, lens};
///
/// Table::new(TextColumn::new(lens!(Product.name))).row_link(
///     RowLink::new("Open product", |product: &Product| {
///         Some(format!("/shop/{}", product.slug))
///     })
///     .new_tab()
///     .reading(Reads::of(lens!(Product.slug))),
/// );
/// ```
#[derive_where(Clone)]
pub struct RowLink<M> {
    pub(crate) label: Text,
    url: crate::column::ReadFn<M, Option<String>>,
    pub(crate) icon: Option<IconData>,
    pub(crate) new_tab: bool,
    /// The fields `url` reads, when declared.
    pub(super) reads: Option<Reads<M>>,
}

impl<M> RowLink<M> {
    /// A link reading `label` to `url(row)`; a row it gives `None` shows no link. The URL is
    /// relative, or absolute over `http`, `https`, `mailto` or `tel`: a row whose URL has any
    /// other scheme, such as `javascript:`, shows no link either.
    pub fn new(
        label: impl Into<Text>,
        url: impl Fn(&M) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            url: Arc::new(url),
            icon: None,
            new_tab: false,
            reads: None,
        }
    }

    /// Show `icon` before the label.
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Open the link in a new tab.
    pub fn new_tab(mut self) -> Self {
        self.new_tab = true;
        self
    }

    /// Declare the fields the URL reads. A resource that
    /// [restricts a field](crate::ResourceDef::field_access) refuses the table until each of its
    /// links declares them, and a user who may not view one of them sees no such link.
    pub fn reading(mut self, reads: Reads<M>) -> Self {
        self.reads = Some(reads);
        self
    }

    /// Where the link leads from `row`, when it shows there.
    pub(crate) fn url_of(&self, row: &M) -> Option<String> {
        (self.url)(row).filter(|url| crate::column::link_href(url).is_some())
    }
}

impl<M> Table<M> {
    /// Add `link` to each row's actions, before its custom actions, after the links already
    /// declared.
    pub fn row_link(mut self, link: RowLink<M>) -> Self {
        self.row_links.push(link);
        self
    }

    /// The links each row shows.
    pub(crate) fn row_links(&self) -> &[RowLink<M>] {
        &self.row_links
    }

    /// Whether a row link does not declare what its URL reads.
    pub(crate) fn row_link_reads_undeclared(&self) -> bool {
        self.row_links.iter().any(|link| link.reads.is_none())
    }
}
