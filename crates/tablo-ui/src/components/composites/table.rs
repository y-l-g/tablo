//! Tablo's table: columns headed by captions over a strong rule, rows ruled by hairlines and
//! marked as the pointer crosses them. The wrapper carries `data-slot="table"`, which each theme's
//! skin keys on.

use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const TABLE: StaticClass = class!(
    "w-full caption-bottom border-collapse text-sm tabular-nums",
    "[:not(:first-child)>&]:border-t [:not(:last-child)>&]:border-b border-rule",
);
const CAPTION: StaticClass = class!("mt-4 text-sm text-muted-foreground");
const HEAD_ROW: StaticClass = class!("bg-head [&_tr]:border-b [&_tr]:border-rule-strong");
const ROW: StaticClass = class!(
    "border-b border-rule transition-colors hover:bg-mark/60",
    "has-[[data-row-select]_input:checked]:bg-mark",
);
const HEAD: StaticClass = class!(
    "tablo-caption h-10 px-3 text-left align-middle whitespace-nowrap text-muted-foreground",
    "first:pl-5 last:pr-5",
);
const CELL: StaticClass =
    class!("h-12 px-3 py-1.5 align-middle whitespace-nowrap first:pl-5 last:pr-5");

/// The table, in a wrapper that scrolls it sideways when it is wider than its container.
#[component]
pub async fn table(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div data-slot="table" class="w-full overflow-x-auto">
            // The rules sit on the table, so a caption stays outside them.
            <table class=(class!(TABLE, attrs.remove("class"))) (attrs)>(child)</table>
        </div>
    })
}

/// The `<thead>`, on the head band's tint.
#[component]
pub async fn table_header(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <thead class=(class!(HEAD_ROW, attrs.remove("class"))) (attrs)>(child)</thead>
    })
}

/// The `<tbody>`; its last row drops its rule, which the table's edge draws.
#[component]
pub async fn table_body(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <tbody
            class=(class!("[&_tr:last-child]:border-0", attrs.remove("class")))
            (attrs)
        >
            (child)
        </tbody>
    })
}

/// The `<tfoot>`: totals over a strong rule.
#[component]
pub async fn table_footer(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <tfoot
            class=(class!(
                "border-t border-rule-strong bg-head font-medium [&>tr]:last:border-b-0",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </tfoot>
    })
}

/// A `<tr>`.
#[component]
pub async fn table_row(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <tr class=(class!(ROW, attrs.remove("class"))) (attrs)>(child)</tr> })
}

/// A column's `<th>`, set as a caption.
#[component]
pub async fn table_head(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <th class=(class!(HEAD, attrs.remove("class"))) (attrs)>(child)</th> })
}

/// A `<td>`.
#[component]
pub async fn table_cell(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <td class=(class!(CELL, attrs.remove("class"))) (attrs)>(child)</td> })
}

/// A `<caption>` under the table.
#[component]
pub async fn table_caption(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <caption class=(class!(CAPTION, attrs.remove("class"))) (attrs)>
            (child)
        </caption>
    })
}
