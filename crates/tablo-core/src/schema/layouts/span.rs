//! How many of its grid's columns a block or a field spans.

use topcoat::{context::Cx, view::*};

/// How many of the columns of the [`Grid`](crate::Grid) holding it a block or a field spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Span {
    /// That many columns, clamped to 1..12, and never more than the grid lays out.
    Columns(u8),
    /// Every column.
    Full,
}

impl Span {
    pub(crate) fn columns(columns: u8) -> Self {
        Self::Columns(columns.clamp(1, 12))
    }

    fn class(self) -> &'static str {
        // `--tablo-cols` holds how many columns the grid lays out at its current width, so a
        // span never asks a stacked grid for implicit columns. Static literals for the Tailwind
        // scanner, as in `grid_chrome`.
        match self {
            Self::Full => "col-span-full",
            Self::Columns(1) => "col-span-1",
            Self::Columns(2) => "col-span-[min(2,var(--tablo-cols,1))]",
            Self::Columns(3) => "col-span-[min(3,var(--tablo-cols,1))]",
            Self::Columns(4) => "col-span-[min(4,var(--tablo-cols,1))]",
            Self::Columns(5) => "col-span-[min(5,var(--tablo-cols,1))]",
            Self::Columns(6) => "col-span-[min(6,var(--tablo-cols,1))]",
            Self::Columns(7) => "col-span-[min(7,var(--tablo-cols,1))]",
            Self::Columns(8) => "col-span-[min(8,var(--tablo-cols,1))]",
            Self::Columns(9) => "col-span-[min(9,var(--tablo-cols,1))]",
            Self::Columns(10) => "col-span-[min(10,var(--tablo-cols,1))]",
            Self::Columns(11) => "col-span-[min(11,var(--tablo-cols,1))]",
            Self::Columns(_) => "col-span-[min(12,var(--tablo-cols,1))]",
        }
    }
}

/// `body` spanning `span` columns of the grid holding it, or as it is without a span.
pub(crate) fn spanned<'a>(cx: &'a Cx, span: Option<Span>, body: BoxView<'a>) -> BoxView<'a> {
    match span {
        Some(span) => view! { <div class=(span.class())>(body)</div> }.boxed(),
        None => body,
    }
}
