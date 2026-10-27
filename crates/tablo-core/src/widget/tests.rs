use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};

use super::{MountedWidget, Span, Widget, WidgetDef};

struct RecentOrdersWidget;

impl Widget for RecentOrdersWidget {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <p>"orders"</p> })
    }
}

struct Revenue;

impl Widget for Revenue {
    fn declare() -> WidgetDef {
        WidgetDef::new().columns(0)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => <p>"revenue"</p> })
    }
}

#[test]
fn a_widget_is_named_after_its_type_without_a_trailing_widget() {
    assert_eq!(
        MountedWidget::new::<RecentOrdersWidget>().name,
        "recent-orders"
    );
    assert_eq!(MountedWidget::new::<Revenue>().name, "revenue");
}

#[test]
fn a_widget_spans_at_least_one_column() {
    assert_eq!(MountedWidget::new::<Revenue>().span, Span::Columns(1));
}
