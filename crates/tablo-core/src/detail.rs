//! A resource's detail page: the [`Detail`] declaration, its columns arranged in layout blocks.

use std::sync::Arc;

use toasty::stmt::{List, Query};
use topcoat::{context::Cx, view::*};

use crate::{
    DeclarationErrorKind,
    column::{
        BooleanColumn, BoxColumn, ColorColumn, Column, ComputedColumn, CountColumn, DateColumn,
        EmbeddedColumn, FileColumn, ImageColumn, RelationColumn, RepeaterColumn, TextColumn,
        include_relations, restrict_relations, unavailable_sources,
    },
    lens::shows,
    schema::{Grid, Group, Section},
};

/// The declaration of a detail page: [`Column`]s, the same ones a [`Table`](crate::Table) lists,
/// arranged in [`Section`], [`Group`] and [`Grid`] blocks.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     title: String,
/// #     featured: bool,
/// #     author_id: uuid::Uuid,
/// #     #[belongs_to(key = author_id, references = id)]
/// #     author: toasty::Deferred<Author>,
/// # }
/// use tablo_core::{BooleanColumn, Detail, RelationColumn, Section, TextColumn, lens, relation};
///
/// Detail::new(Section::new("Post").columns((
///     TextColumn::new(lens!(Post.title)),
///     BooleanColumn::new(lens!(Post.featured)),
///     RelationColumn::unscoped(relation!(Post.author), |a: &Author| a.name.clone()),
/// )));
/// ```
///
/// The detail page loads the relations its columns declare ([`Column::includes`]) and renders each
/// column's [`entry`](Column::entry) for the record.
pub struct Detail<M> {
    nodes: Vec<Node<M>>,
}

/// One block or column of a [`Detail`].
enum Node<M> {
    Column(BoxColumn<M>),
    Section(Box<Section<Detail<M>>>),
    Group(Box<Group<Detail<M>>>),
    Grid(Box<Grid<Detail<M>>>),
}

impl<M> Detail<M> {
    /// Declare a detail page of `children`: columns and blocks of them.
    pub fn new(children: impl IntoDetail<M>) -> Self {
        children.into_detail()
    }

    /// A detail page showing nothing, which turns the page off.
    pub fn empty() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Whether the declaration shows nothing.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Append `column` after the declared columns and blocks.
    pub fn column(mut self, column: impl Column<M> + 'static) -> Self {
        self.nodes.push(Node::Column(Arc::new(column)));
        self
    }

    /// Appends `other`'s columns and blocks after this one's.
    fn append(&mut self, other: Self) {
        self.nodes.extend(other.nodes);
    }

    /// Every column, in the order the page renders them.
    fn columns(&self) -> Vec<&BoxColumn<M>> {
        fn walk<'d, M>(nodes: &'d [Node<M>], out: &mut Vec<&'d BoxColumn<M>>) {
            for node in nodes {
                match node {
                    Node::Column(column) => out.push(column),
                    Node::Section(section) => walk(&section.children.nodes, out),
                    Node::Group(group) => walk(&group.children.nodes, out),
                    Node::Grid(grid) => walk(&grid.children.nodes, out),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.nodes, &mut out);
        out
    }

    /// Load the records `query` names as a page rendering this declaration reads them: with every
    /// relation the columns declare, once each, and the related records the columns' sources show
    /// the user checked, so [`render`](Self::render) shows only those. Pass the resource's scope,
    /// as in `detail.load(cx, scoped_query::<R>(cx)?)`.
    ///
    /// # Errors
    ///
    /// A source's scope refusing the request, or a database failure.
    pub async fn load(&self, cx: &Cx, query: Query<List<M>>) -> topcoat::Result<Vec<M>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        let mut db = crate::db::db(cx);
        let records = self
            .include_relations(query)
            .exec(&mut db)
            .await
            .map_err(crate::error::unavailable)?;
        self.restrict_relations(cx, &records, &mut db).await?;
        Ok(records)
    }

    /// Include in `query` every relation the columns declare, once each.
    pub(crate) fn include_relations(&self, query: Query<List<M>>) -> Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        include_relations(query, self.columns())
    }

    /// Check which related records `records`, loaded through [`Self::include_relations`], hold
    /// that the columns' sources show the user.
    pub(crate) async fn restrict_relations(
        &self,
        cx: &Cx,
        records: &[M],
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        restrict_relations(cx, self.columns(), records, ex).await
    }

    /// The columns labelling their records by a source the context's panel cannot load from.
    pub(crate) fn unavailable_sources(&self, cx: &Cx) -> Vec<DeclarationErrorKind> {
        unavailable_sources(cx, self.columns().into_iter().map(|c| &**c))
    }

    /// What is wrong with this declaration: each column whose path binds no single field.
    pub fn declaration_errors(&self) -> Vec<DeclarationErrorKind> {
        self.columns()
            .into_iter()
            .filter_map(|column| column.misdeclared())
            .collect()
    }

    /// Renders `record`'s entries in their blocks: each column's [`entry`](Column::entry),
    /// read-only.
    ///
    /// The panel's detail page renders its view this way. A page of the app's own renders one for
    /// a record it loaded through [`load`](Self::load): a scoped relation column over a record
    /// loaded otherwise shows no related record.
    pub fn render<'a>(&self, cx: &'a Cx, record: &M) -> BoxView<'a> {
        self.render_without(cx, record, &[])
    }

    /// Renders as [`render`](Self::render) does, without the columns reading one of the `hidden`
    /// fields, and without a block left with no column.
    pub(crate) fn render_without<'a>(
        &self,
        cx: &'a Cx,
        record: &M,
        hidden: &[String],
    ) -> BoxView<'a> {
        let views: Vec<BoxView<'a>> = self
            .nodes
            .iter()
            .filter(|node| match node {
                Node::Column(column) => shows(column.reads(), hidden),
                Node::Section(section) => {
                    section.children.is_empty() || section.children.shows_any(hidden)
                }
                Node::Group(group) => group.children.shows_any(hidden),
                Node::Grid(grid) => grid.children.shows_any(hidden),
            })
            .map(|node| match node {
                Node::Column(column) => column.entry(cx, record),
                Node::Section(section) => {
                    let body = (!section.children.is_empty())
                        .then(|| section.children.render_without(cx, record, hidden));
                    section.chrome(cx, body)
                }
                Node::Group(group) => {
                    group.chrome(cx, group.children.render_without(cx, record, hidden))
                }
                Node::Grid(grid) => {
                    grid.chrome(cx, grid.children.render_without(cx, record, hidden))
                }
            })
            .collect();
        stack(cx, views)
    }

    /// Whether any column shows to a request that may not view the `hidden` fields.
    fn shows_any(&self, hidden: &[String]) -> bool {
        self.columns()
            .into_iter()
            .any(|column| shows(column.reads(), hidden))
    }

    /// The names of the columns that do not say what they read.
    pub(crate) fn undeclared_reads(&self) -> Vec<&str> {
        self.columns()
            .into_iter()
            .filter(|column| column.reads().is_none())
            .map(|column| column.name())
            .collect()
    }
}

/// `views` one after another. A free function, so tablo-core compiles the view once rather than
/// once for each model with a detail page.
fn stack<'a>(cx: &'a Cx, views: Vec<BoxView<'a>>) -> BoxView<'a> {
    view! {
        cx =>
        for v in views {
            (v)
        }
    }
    .boxed()
}

impl<M> Default for Detail<M> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<M> std::fmt::Debug for Detail<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Detail")
            .field(
                "columns",
                &self
                    .columns()
                    .into_iter()
                    .map(|column| column.name())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Converts a column, a layout block of columns, a tuple of either, or a [`Detail`] into a
/// [`Detail`].
///
/// Every built-in column converts, as does a boxed `Arc<dyn Column<M>>`;
/// [`Detail::column`] appends any other [`Column`]. An app's column type joins a block as the
/// built-in ones do by implementing this trait:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # struct Initials;
/// # use topcoat::context::Cx;
/// # impl tablo_core::extend::Column<User> for Initials {
/// #     fn name(&self) -> &str { "initials" }
/// #     fn label(&self) -> &str { "Initials" }
/// #     fn text(&self, _cx: &Cx, row: &User) -> String { row.name.clone() }
/// # }
/// use tablo_core::{Detail, IntoDetail, Section};
///
/// impl IntoDetail<User> for Initials {
///     fn into_detail(self) -> Detail<User> {
///         Detail::empty().column(self)
///     }
/// }
///
/// Detail::new(Section::new("User").columns(Initials));
/// ```
pub trait IntoDetail<M> {
    fn into_detail(self) -> Detail<M>;
}

impl<M> IntoDetail<M> for Detail<M> {
    fn into_detail(self) -> Detail<M> {
        self
    }
}

impl<M> IntoDetail<M> for () {
    fn into_detail(self) -> Detail<M> {
        Detail::empty()
    }
}

impl<M> IntoDetail<M> for BoxColumn<M> {
    fn into_detail(self) -> Detail<M> {
        Detail {
            nodes: vec![Node::Column(self)],
        }
    }
}

/// The [`IntoDetail`] impls of the built-in columns: each is one column.
macro_rules! column_details {
    ($($ty:ident<$($g:ident),+>),+ $(,)?) => {
        $(
            impl<$($g),+> IntoDetail<M> for $ty<$($g),+>
            where
                Self: Column<M> + 'static,
            {
                fn into_detail(self) -> Detail<M> {
                    Detail::empty().column(self)
                }
            }
        )+
    };
}

column_details!(
    TextColumn<M, T>,
    DateColumn<M, T>,
    ComputedColumn<M>,
    RelationColumn<M>,
    CountColumn<M>,
    BooleanColumn<M>,
    FileColumn<M>,
    ColorColumn<M, T>,
    ImageColumn<M, T>,
    EmbeddedColumn<M, T>,
    RepeaterColumn<M, T>,
);

/// The `columns` builders of the layout blocks: each holds a detail page's columns and blocks.
macro_rules! block_columns {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl $ty<()> {
                /// Holds a detail page's columns and blocks.
                pub fn columns<M>(self, children: impl IntoDetail<M>) -> $ty<Detail<M>> {
                    self.holding(children.into_detail())
                }
            }
        )+
    };
}

block_columns!(Section, Group, Grid);

/// The single-node [`IntoDetail`] impls of every layout block, holding columns or nothing.
macro_rules! block_details {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<M> IntoDetail<M> for $ty<Detail<M>> {
                fn into_detail(self) -> Detail<M> {
                    Detail {
                        nodes: vec![Node::$ty(Box::new(self))],
                    }
                }
            }

            impl<M> IntoDetail<M> for $ty<()> {
                fn into_detail(self) -> Detail<M> {
                    self.holding(Detail::empty()).into_detail()
                }
            }
        )+
    };
}

block_details!(Section, Group, Grid);

/// The tuple impls of [`IntoDetail`], one arity per invocation.
macro_rules! into_detail_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<M, $($T),+> IntoDetail<M> for ($($T,)+)
        where
            $($T: IntoDetail<M>,)+
        {
            fn into_detail(self) -> Detail<M> {
                let ($($v,)+) = self;
                let mut detail = Detail::empty();
                $( detail.append($v.into_detail()); )+
                detail
            }
        }
    };
}

into_detail_tuples!(A => a);
into_detail_tuples!(A => a, B => b);
into_detail_tuples!(A => a, B => b, C => c);
into_detail_tuples!(A => a, B => b, C => c, D => d);
into_detail_tuples!(A => a, B => b, C => c, D => d, E => e);
into_detail_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_detail_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_detail_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);
into_detail_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i
);
into_detail_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j
);
into_detail_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k
);
into_detail_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k, L => l
);

#[cfg(test)]
mod tests;
