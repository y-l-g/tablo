//! Holds the `Section`, `Group`, and `Grid` containers that compose form layout.

use tablo_ui::{card_content, card_header, card_title, field_group as ui_field_group};
use topcoat::{
    Result,
    context::Cx,
    view::{StaticClass, class, *},
};

use super::{
    Schema,
    fields::Field,
    tree::{IntoSchema, Source, render_nodes},
};

/// Renders a `Section`'s titled panel.
const PANEL: StaticClass = class!(
    "flex flex-col gap-5 rounded-xl border border-border bg-card py-6 text-card-foreground shadow-sm"
);

/// Holds a titled container with an optional child `Schema`.
#[derive(Debug)]
pub struct Section {
    title: String,
    pub(crate) children: Schema,
    extra_class: Option<String>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            children: Schema::empty(),
            extra_class: None,
        }
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    /// Adds an additive `class` hook on the panel container.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.extra_class = Some(class.into());
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let title = self.title.clone();
        let extra = self.extra_class.clone();
        if !self.children.nodes.is_empty() {
            let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
            Ok(view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6" },
                        (child_view)
                    )
                </div>
            }
            .boxed())
        } else {
            Ok(view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                </div>
            }
            .boxed())
        }
    }
}

/// Holds an unlabelled container for grouping fields.
#[derive(Debug, Default)]
pub struct Group {
    pub(crate) children: Schema,
}

impl Group {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(view! { cx => ui_field_group((child_view)) }.boxed())
    }
}

/// Holds a column container with `cols` clamped to 1..12.
#[derive(Debug)]
pub struct Grid {
    cols: u8,
    pub(crate) children: Schema,
}

impl Grid {
    pub fn new(cols: u8) -> Self {
        Self {
            cols: cols.clamp(1, 12),
            children: Schema::empty(),
        }
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
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
        let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(view! { cx => <div class=(class)>(child_view)</div> }.boxed())
    }
}

#[cfg(test)]
mod tests;
