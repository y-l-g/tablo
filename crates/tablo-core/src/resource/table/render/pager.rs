//! Previous/Next pagination links from the executed page’s real cursors.

use tablo_ui::{
    pagination, pagination_content, pagination_item, pagination_next, pagination_previous,
};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::super::{
    super::state::{TablePage, TableSignals, TableState},
    Table,
};

impl<M> Table<M> {
    /// Previous/Next pagination links from the executed page's real cursors.
    /// Empty when the table is not paginated or the page has no neighbors —
    /// no invented page numbers. Links preserve the search and sort state;
    /// cursors travel via `?after=`/`?before=`.
    ///
    /// With `signals` (a live table) each link also writes its cursor signal
    /// and clears the opposite one; `href` stays the no-JS fallback.
    pub(super) async fn render_pager<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        page: &TablePage<M>,
        signals: Option<&TableSignals>,
    ) -> Result<Vec<BoxView<'a>>> {
        if self.page_size.is_none() {
            return Ok(Vec::new());
        }
        // Cursors only carry ordering values; the loader re-applies search and
        // sort, so the links must carry that state along.
        let next_href = page
            .next_cursor
            .as_ref()
            .map(|cursor| state.with_after(path, cursor));
        let prev_href = page
            .prev_cursor
            .as_ref()
            .map(|cursor| state.with_before(path, cursor));
        if prev_href.is_none() && next_href.is_none() {
            return Ok(Vec::new());
        }
        let prev_item: Option<BoxView<'a>> = prev_href.map(|href| {
            let attrs = match (signals, page.prev_cursor.as_deref()) {
                (Some(signals), Some(cursor)) => {
                    let wire = crate::resource::cursor_before(cursor);
                    let signal = signals.cursor.clone();
                    attributes! {
                        cx =>
                        href=(href.clone())
                        @click=$(|e: Event| {
                            e.prevent_default();
                            signal.set(wire.clone());
                        })
                    }
                }
                _ => attributes! { cx => href=(href) },
            };
            view! { cx => pagination_item(pagination_previous(attrs: attrs)) }.boxed()
        });
        let next_item: Option<BoxView<'a>> = next_href.map(|href| {
            let attrs = match (signals, page.next_cursor.as_deref()) {
                (Some(signals), Some(cursor)) => {
                    let wire = crate::resource::cursor_after(cursor);
                    let signal = signals.cursor.clone();
                    attributes! {
                        cx =>
                        href=(href.clone())
                        @click=$(|e: Event| {
                            e.prevent_default();
                            signal.set(wire.clone());
                        })
                    }
                }
                _ => attributes! { cx => href=(href) },
            };
            view! { cx => pagination_item(pagination_next(attrs: attrs)) }.boxed()
        });
        let pager = view! {
            cx =>
            <div class="border-t border-border p-3">
                pagination(
                    pagination_content(
                        if let Some(item) = prev_item {
                            (item)
                        }
                        if let Some(item) = next_item {
                            (item)
                        }
                    )
                )
            </div>
        };
        Ok(vec![pager.boxed()])
    }
}
#[cfg(test)]
mod tests {
    use topcoat::context::CxTestBuilder;

    use super::{super::core::tests::User, *};
    use crate::{
        TablePage, TableState,
        resource::{Sort, TextColumn},
    };

    #[tokio::test]
    async fn paginate_zero_is_a_render_error_not_a_panic() {
        let cx = CxTestBuilder::new().build();
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        // Zero page size is a programmer error: a descriptive error
        // the streamed list renders in-region, never a per-request panic.
        let zero = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .paginate(0);
        let page: TablePage<User> = rows.into();
        let err = match zero
            .render_with_state(&cx, page, &TableState::default(), "/admin/users")
            .await
        {
            Ok(_) => panic!("paginate(0) must error"),
            Err(err) => err,
        };
        assert!(
            err.to_string().contains("per_page > 0"),
            "error must name the contract, got {err}"
        );
    }

    #[tokio::test]
    async fn group_by_survives_pager_and_labels_page_local_counts() {
        let cx = CxTestBuilder::new().build();
        let grouped = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
        )
        .group_by("status", |u| u.name.clone())
        .paginate(1);
        let state = TableState {
            group_by: Some("status".to_string()),
            sort: Some(Sort {
                column: "name".to_string(),
                descending: false,
            }),
            ..TableState::default()
        };
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        let page = TablePage {
            rows,
            next_cursor: Some("abc".to_string()),
            prev_cursor: None,
        };
        let html = grouped
            .render_with_state(&cx, page, &state, "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("on this page"),
            "group header must be page-local, got {html}"
        );
        assert!(
            html.contains("group_by") && html.contains("after=abc"),
            "pager must preserve group_by, got {html}"
        );
    }

    #[tokio::test]
    async fn void_window_links_back_to_first_page() {
        // a cursor past the last row (rows deleted under pagination)
        // must offer navigation, never a pager-less dead end.
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
        )
        .paginate(1);
        let void_page = TablePage {
            rows: Vec::new(),
            next_cursor: None,
            prev_cursor: None,
        };
        let state = TableState {
            after: Some("abc".to_string()),
            sort: Some(Sort {
                column: "name".to_string(),
                descending: false,
            }),
            ..TableState::default()
        };
        let html = tbl
            .render_with_state(&cx, void_page, &state, "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("Back to first page"),
            "void window must link home, got {html}"
        );

        // A genuinely empty first page stays pager-less (its empty-state
        // already offers Clear links).
        let empty_first = TablePage {
            rows: Vec::new(),
            next_cursor: None,
            prev_cursor: None,
        };
        let html = tbl
            .render_with_state(&cx, empty_first, &TableState::default(), "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("Back to first page"),
            "empty first page must stay pager-less, got {html}"
        );
    }
}
