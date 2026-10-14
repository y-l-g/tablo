macro_rules! state_shell {
    (
        $cx:expr,
        $title:expr,
        $detail:expr,
        $action:expr,
        $attrs:expr,
        $container:expr,
        $title_class:expr,
        $detail_class:expr,
        $action_class:expr,
        $icon_data:expr,
        $icon_wrap:expr
    ) => {
        Ok(topcoat::view::view! { $cx =>
            <div class=(class!($container, $attrs.remove("class"))) ($attrs)>
                if let Some(wrap) = $icon_wrap {
                    <div class=(wrap) aria-hidden="true">icon(data: $icon_data.clone())</div>
                } else {
                    icon(data: $icon_data)
                }
                <p class=($title_class)>($title)</p>
                if !$detail.is_empty() {
                    <p class=($detail_class)>($detail)</p>
                }
                if let Some(action) = $action {
                    <div class=($action_class)>(action)</div>
                }
            </div>
        })
    };
}

pub(crate) use state_shell;
