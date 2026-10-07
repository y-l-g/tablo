//! A resource's detail page: the [`Detail`] declaration, its columns arranged in layout blocks.

use std::sync::Arc;

use toasty::stmt::{List, Query};
use topcoat::{context::Cx, view::*};

use crate::{
    DeclarationErrorKind, EmbeddedForm,
    schema::{FieldResolver, Grid, Group, Section},
    table::{
        BooleanColumn, BoxColumn, Column, ComputedColumn, CountColumn, EmbeddedColumn, FileColumn,
        RelationColumn, TextColumn, include_relations,
    },
};

/// The declaration of a detail page: [`Column`]s, the same ones a [`Table`](crate::Table) lists,
/// arranged in [`Section`], [`Group`] and [`Grid`] blocks.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
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
///     RelationColumn::new(relation!(Post.author), |a: &Author| a.name.clone()),
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

    /// Include in `query` every relation the columns declare, once each: what a page rendering
    /// this declaration loads, as in `detail.include_relations(scoped_query::<R>(cx)?)`.
    pub fn include_relations(&self, query: Query<List<M>>) -> Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        include_relations(query, self.columns())
    }

    /// What is wrong with this declaration: each column whose path binds no single field.
    pub fn declaration_errors(&self) -> Vec<DeclarationErrorKind> {
        self.columns()
            .into_iter()
            .filter_map(|column| column.misdeclared())
            .collect()
    }

    /// Binds the declaration's embedded paths and values to `db`'s app schema.
    ///
    /// A panel binds the detail pages it mounts; bind one built outside a panel so its
    /// [`declaration_errors`](Self::declaration_errors) reports no unbound path. A detail page with
    /// no embedded path or value is bound from the start.
    pub fn bind(self, db: &toasty::Db) -> Self {
        self.bind_with(&FieldResolver::of_db(db));
        self
    }

    /// Binds every column through `resolver`.
    pub(crate) fn bind_with(&self, resolver: &FieldResolver) {
        for column in self.columns() {
            column.bind(resolver);
        }
    }

    /// Renders `record`'s entries in their blocks: each column's [`entry`](Column::entry),
    /// read-only.
    ///
    /// The panel's detail page renders its view this way. A page of the app's own renders one for
    /// a record it loaded through [`include_relations`](Self::include_relations), after
    /// [`bind`](Self::bind) when it shows an embedded value.
    pub fn render<'a>(&self, cx: &'a Cx, record: &M) -> BoxView<'a> {
        let views: Vec<BoxView<'a>> = self
            .nodes
            .iter()
            .map(|node| match node {
                Node::Column(column) => column.entry(cx, record),
                Node::Section(section) => {
                    let body =
                        (!section.children.is_empty()).then(|| section.children.render(cx, record));
                    section.chrome(cx, body)
                }
                Node::Group(group) => group.chrome(cx, group.children.render(cx, record)),
                Node::Grid(grid) => grid.chrome(cx, grid.children.render(cx, record)),
            })
            .collect();
        view! {
            cx =>
            for v in views {
                (v)
            }
        }
        .boxed()
    }
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
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # struct Initials;
/// # impl tablo_core::Column<User> for Initials {
/// #     fn name(&self) -> &str { "initials" }
/// #     fn label(&self) -> &str { "Initials" }
/// #     fn text(&self, row: &User) -> String { row.name.clone() }
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

impl<M, T> IntoDetail<M> for TextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    fn into_detail(self) -> Detail<M> {
        Detail::empty().column(self)
    }
}

impl<M, T> IntoDetail<M> for EmbeddedColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: EmbeddedForm + Send + Sync + 'static,
{
    fn into_detail(self) -> Detail<M> {
        Detail::empty().column(self)
    }
}

/// The [`IntoDetail`] impls of the built-in columns over the model alone.
macro_rules! column_details {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<M> IntoDetail<M> for $ty<M>
            where
                M: toasty::schema::Model + Send + Sync + 'static,
            {
                fn into_detail(self) -> Detail<M> {
                    Detail::empty().column(self)
                }
            }
        )+
    };
}

column_details!(
    BooleanColumn,
    ComputedColumn,
    CountColumn,
    FileColumn,
    RelationColumn
);

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

#[cfg(test)]
mod tests;
