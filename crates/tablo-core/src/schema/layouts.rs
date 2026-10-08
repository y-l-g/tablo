//! Holds the `Section`, `Group`, and `Grid` blocks that lay out a form's fields or a detail page's
//! columns.

use tablo_ui::{card_content, card_header, card_title, field_group as ui_field_group};
use topcoat::{
    Result,
    context::Cx,
    view::{StaticClass, class, *},
};

use super::{
    Schema,
    condition::{Condition, Watched},
    fields::Field,
    tree::{IntoSchema, Source, render_nodes},
};
use crate::detail::{Detail, IntoDetail};

/// Renders a `Section`'s titled panel.
const PANEL: StaticClass = class!(
    "flex flex-col gap-5 rounded-xl border border-border bg-card py-6 text-card-foreground shadow-sm"
);

/// A titled block holding a form's fields ([`schema`](Self::schema)) or a detail page's columns
/// ([`columns`](Self::columns)).
#[derive(Debug)]
pub struct Section<C = Schema> {
    title: String,
    pub(crate) children: C,
    extra_class: Option<String>,
    pub(crate) condition: Option<Condition>,
}

impl Section<()> {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            children: (),
            extra_class: None,
            condition: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Section<Schema<F>> {
        self.holding(children.into_schema())
    }

    /// Holds a detail page's columns and blocks.
    pub fn columns<M>(self, children: impl IntoDetail<M>) -> Section<Detail<M>> {
        self.holding(children.into_detail())
    }
}

impl<C> Section<C> {
    /// Adds an additive `class` hook on the panel container.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.extra_class = Some(class.into());
        self
    }

    pub(crate) fn holding<D>(self, children: D) -> Section<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Section<D> {
        Section {
            title: self.title,
            children: f(self.children),
            extra_class: self.extra_class,
            condition: self.condition,
        }
    }

    /// The titled panel around `body`, or the title alone when the section holds nothing.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: Option<BoxView<'a>>) -> BoxView<'a> {
        let title = self.title.clone();
        let extra = self.extra_class.clone();
        match body {
            Some(body) => view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6" },
                        (body)
                    )
                </div>
            }
            .boxed(),
            None => view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                </div>
            }
            .boxed(),
        }
    }
}

impl Section<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let body = if self.children.nodes.is_empty() {
            None
        } else {
            Some(render_nodes(cx, &self.children.nodes, fields, source).await?)
        };
        Ok(self.chrome(cx, body))
    }
}

/// An unlabelled block grouping a form's fields or a detail page's columns.
#[derive(Debug)]
pub struct Group<C = Schema> {
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Default for Group<()> {
    fn default() -> Self {
        Self {
            children: (),
            condition: None,
        }
    }
}

impl Group<()> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Group<Schema<F>> {
        self.holding(children.into_schema())
    }

    /// Holds a detail page's columns and blocks.
    pub fn columns<M>(self, children: impl IntoDetail<M>) -> Group<Detail<M>> {
        self.holding(children.into_detail())
    }
}

impl<C> Group<C> {
    pub(crate) fn holding<D>(self, children: D) -> Group<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Group<D> {
        Group {
            children: f(self.children),
            condition: self.condition,
        }
    }

    /// The group around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        view! { cx => ui_field_group((body)) }.boxed()
    }
}

impl Group<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let body = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(self.chrome(cx, body))
    }
}

/// A block of `cols` columns, clamped to 1..12, laying out a form's fields or a detail page's
/// columns.
#[derive(Debug)]
pub struct Grid<C = Schema> {
    cols: u8,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Grid<()> {
    pub fn new(cols: u8) -> Self {
        Self {
            cols: cols.clamp(1, 12),
            children: (),
            condition: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Grid<Schema<F>> {
        self.holding(children.into_schema())
    }

    /// Holds a detail page's columns and blocks.
    pub fn columns<M>(self, children: impl IntoDetail<M>) -> Grid<Detail<M>> {
        self.holding(children.into_detail())
    }
}

impl<C> Grid<C> {
    pub(crate) fn holding<D>(self, children: D) -> Grid<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Grid<D> {
        Grid {
            cols: self.cols,
            children: f(self.children),
            condition: self.condition,
        }
    }

    /// The grid around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        // Static literals for Tailwind scanner — `format!("grid grid-cols-{}")` would be
        // purged because Tailwind only sees literal substrings.
        let class: &'static str = match self.cols {
            1 => "grid grid-cols-1 gap-4",
            2 => "grid grid-cols-2 gap-4",
            3 => "grid grid-cols-3 gap-4",
            4 => "grid grid-cols-4 gap-4",
            5 => "grid grid-cols-5 gap-4",
            6 => "grid grid-cols-6 gap-4",
            7 => "grid grid-cols-7 gap-4",
            8 => "grid grid-cols-8 gap-4",
            9 => "grid grid-cols-9 gap-4",
            10 => "grid grid-cols-10 gap-4",
            11 => "grid grid-cols-11 gap-4",
            _ => "grid grid-cols-12 gap-4",
        };
        view! { cx => <div class=(class)>(body)</div> }.boxed()
    }
}

impl Grid<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let body = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(self.chrome(cx, body))
    }
}

/// Generates `visible_when` for every layout block holding a form's fields.
macro_rules! conditional_blocks {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<F> $ty<Schema<F>> {
                /// Shows the block and every field it holds only while `watched` posts one of
                /// `values`, as [`TextField::visible_when`](crate::TextField::visible_when) shows a
                /// field.
                pub fn visible_when(
                    mut self,
                    watched: &impl Watched<F>,
                    values: impl IntoIterator<Item = impl Into<String>>,
                ) -> Self {
                    self.condition = Some(Condition::new(watched, values));
                    self
                }
            }
        )+
    };
}

conditional_blocks!(Section, Group, Grid);
