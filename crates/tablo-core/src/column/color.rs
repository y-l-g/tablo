//! [`ColorColumn`]: a CSS color rendered as a swatch beside the value.

use derive_where::derive_where;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth};
use crate::{Lens, values::FormScalar};

/// A column of a `String` field holding a CSS color, rendered as a swatch beside the value.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Team { #[key] #[auto] id: uuid::Uuid, color: String }
/// tablo_core::ColorColumn::new(tablo_core::lens!(Team.color));
/// ```
///
/// A hex color (`#3b82f6`) or a color function (`rgb(…)`, `hsl(…)`, `oklch(…)`, …) shows its
/// swatch; any other value shows as text alone, since it reaches a `style` attribute.
#[derive_where(Clone, Debug)]
pub struct ColorColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
}

impl<M, T> ColorColumn<M, T>
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
        }
    }

    base_builders!();
}

impl<M, T> Column<M> for ColorColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_form()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.text(cx, row).trim().to_string();
        if !is_css_color(&value) {
            return crate::schema::value_cell(cx, &value);
        }
        let swatch = format!("background-color: {value}");
        view! {
            cx =>
            <span class="inline-flex items-center gap-2">
                <span
                    class="size-4 shrink-0 rounded-sm ring-1 ring-foreground/15 ring-inset"
                    style=(swatch)
                    aria-hidden="true"
                ></span>
                <span class="font-mono text-xs">(value)</span>
            </span>
        }
        .boxed()
    }
}

impl<M, T> crate::column::ColumnInternals<M> for ColorColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);
}

/// Whether `value` is a hex color or a color function of plain arguments, safe inside a `style`
/// attribute: nothing in it can end the declaration or open another.
pub(super) fn is_css_color(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    const FUNCTIONS: &[&str] = &[
        "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(",
    ];
    let lower = value.to_ascii_lowercase();
    let Some(function) = FUNCTIONS.iter().find(|f| lower.starts_with(**f)) else {
        return false;
    };
    let Some(arguments) = lower[function.len()..].strip_suffix(')') else {
        return false;
    };
    arguments
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b" .,%/+-".contains(&b))
}
