//! [`IntoColumns`]: what a table's column list is built from.

use std::sync::Arc;

#[cfg(feature = "rich-text")]
use super::RichTextColumn;
use super::{
    BooleanColumn, BoxColumn, ColorColumn, Column, ComputedColumn, CountColumn, DateColumn,
    EmbeddedColumn, FileColumn, ImageColumn, RelationColumn, RepeaterColumn, TextColumn,
};

/// Convert a single built-in column, a tuple of any [`Column`]s, a `Vec` or slice
/// of boxed columns, or `()` for none yet, into a table's column list.
///
/// [`Table::column`](crate::Table::column) appends past it.
pub trait IntoColumns<M> {
    #[doc(hidden)]
    fn into_columns(self) -> Vec<BoxColumn<M>>;
}

/// The [`IntoColumns`] impls of the built-in columns: each is one column.
macro_rules! column_conversions {
    ($($ty:ident<$($g:ident),+>),+ $(,)?) => {
        $(
            impl<$($g),+> IntoColumns<M> for $ty<$($g),+>
            where
                Self: Column<M> + 'static,
            {
                fn into_columns(self) -> Vec<BoxColumn<M>> {
                    vec![Arc::new(self)]
                }
            }
        )+
    };
}

column_conversions!(
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
#[cfg(feature = "rich-text")]
column_conversions!(RichTextColumn<M, T>);

impl<M> IntoColumns<M> for () {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        Vec::new()
    }
}

impl<M> IntoColumns<M> for Vec<BoxColumn<M>> {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        self
    }
}

impl<M> IntoColumns<M> for &[BoxColumn<M>] {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        self.to_vec()
    }
}

/// The tuple impls of [`IntoColumns`], one arity per invocation.
macro_rules! into_columns_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<M, $($T),+> IntoColumns<M> for ($($T,)+)
        where
            $($T: Column<M> + 'static,)+
        {
            fn into_columns(self) -> Vec<BoxColumn<M>> {
                let ($($v,)+) = self;
                vec![$(Arc::new($v) as BoxColumn<M>,)+]
            }
        }
    };
}

into_columns_tuples!(A => a);
into_columns_tuples!(A => a, B => b);
into_columns_tuples!(A => a, B => b, C => c);
into_columns_tuples!(A => a, B => b, C => c, D => d);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k, L => l
);
