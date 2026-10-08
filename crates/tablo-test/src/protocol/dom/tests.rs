use super::*;

const TABLE: &str = r#"<table><thead><tr><th>Name</th></tr></thead><tbody>
<tr id="row-Ada-12345678"><td><input type="checkbox" value="Ada" aria-label="Select row"></td><td>Ada</td><td><div class="flex"><a href="/admin/dummies/Ada" aria-label="View">V</a><a href="/admin/dummies/Ada/edit" aria-label="Edit">E</a><button type="button" formaction="/admin/dummies/Ada/delete" aria-label="Delete">D</button></div></td></tr>
<tr id="group-Active-87654321"><td colspan="3">Active (1 on this page)</td></tr>
<tr id="row-Ken-abcdef12"><td></td><td>Ken</td><td></td></tr>
<tr><td colspan="3">No records yet</td></tr>
<tr id="row-NoBox-99999999"><td></td><td>NoBox</td><td><div class="flex"><a href="/admin/dummies/NoBox" aria-label="View">V</a></div></td></tr>
<tr id="row-Prefixed-aaaaaaaa"><td></td><td>Prefixed</td><td><div class="flex"><button type="button" formaction="/admin/rel/Prefixed/delete?return=%2Fadmin%2Fposts" aria-label="Delete">D</button></div></td></tr>
</tbody></table>"#;

#[test]
fn rows_skips_group_headers_and_reads_actions() {
    let found = rows(TABLE);
    assert_eq!(found.len(), 4);
    assert_eq!(found[0].select_value.as_deref(), Some("Ada"));
    assert!(found[0].cells.iter().any(|cell| cell == "Ada"));
    assert_eq!(found[0].actions.view.as_deref(), Some("/admin/dummies/Ada"));
    assert_eq!(
        found[0].actions.edit.as_deref(),
        Some("/admin/dummies/Ada/edit")
    );
    assert_eq!(found[1].select_value, None);
    assert!(found[1].cells.iter().any(|cell| cell == "Ken"));
    assert_eq!(found[1].actions.view, None);
}

#[test]
fn row_actions_finds_by_key() {
    let actions = row_actions(TABLE, "Ada").expect("Ada has actions");
    assert_eq!(actions.view.as_deref(), Some("/admin/dummies/Ada"));
    assert_eq!(actions.edit.as_deref(), Some("/admin/dummies/Ada/edit"));
    assert_eq!(
        actions.delete_action.as_deref(),
        Some("/admin/dummies/Ada/delete")
    );
    let without_checkbox =
        row_actions(TABLE, "NoBox").expect("a row without a checkbox matches by URL");
    assert_eq!(
        without_checkbox.view.as_deref(),
        Some("/admin/dummies/NoBox")
    );
    assert_eq!(without_checkbox.edit, None);
    let prefixed = row_actions(TABLE, "Prefixed").expect("a delete action matches by URL");
    assert_eq!(
        prefixed.delete_action.as_deref(),
        Some("/admin/rel/Prefixed/delete?return=%2Fadmin%2Fposts")
    );
    assert_eq!(row_actions(TABLE, "Ad"), None);
    assert_eq!(row_actions(TABLE, "Nobody"), None);
}

#[test]
fn field_error_reads_the_error_slot() {
    let html = r#"<div class="ac-field ac-field--error"><input name="name" aria-invalid="true" aria-describedby="name-error"><div id="name-error" class="ac-error">name is required</div></div>"#;
    assert_eq!(
        field_error(html, "name").as_deref(),
        Some("name is required")
    );
    assert_eq!(field_error(html, "email"), None);
}

#[test]
fn filter_options_reads_values_and_selection() {
    let html = r#"<label>Status<select name="f.status" data-filter-name="status"><option value="">All</option><option value="draft">draft</option><option value="published" selected="">published</option></select></label><label>Created<input type="date" name="f.created_at" data-filter-name="created_at" value="2024-01-15"></label>"#;
    let options = filter_options(html, "status").expect("status has options");
    assert_eq!(options.len(), 3);
    assert!(options.iter().any(|option| option.value == "draft"));
    assert_eq!(
        options
            .iter()
            .find(|option| option.value == "published")
            .map(|option| option.selected),
        Some(true)
    );
    assert_eq!(filter_options(html, "created_at"), None);
    assert_eq!(filter_options(html, "other"), None);
}

#[test]
fn single_quotes_and_entities_decode() {
    let html =
        "<select data-filter-name='status'><option value='a&amp;b'>A &amp; B</option></select>";
    let options = filter_options(html, "status").expect("options");
    assert_eq!(options[0].value, "a&b");
    assert_eq!(options[0].label, "A & B");
}

#[test]
fn empty_table_reads_the_reason_and_its_links() {
    let html = r#"<table><tbody><tr><td colspan="2"><div class="x" data-empty="search"><p>No matches</p><div><a data-empty-link="clear" href="/admin/users">Clear search</a><a href="/admin/users?q=x" data-empty-link="first-page">Back</a></div></div></td></tr></tbody></table>"#;
    assert_eq!(
        empty_table(html),
        Some(EmptyTable {
            reason: "search".to_string(),
            clear: Some("/admin/users".to_string()),
            first_page: Some("/admin/users?q=x".to_string()),
        })
    );
    let bare = r#"<div data-empty="none"><p>No records yet</p></div>"#;
    assert_eq!(
        empty_table(bare).map(|empty| (empty.reason, empty.clear, empty.first_page)),
        Some(("none".to_string(), None, None))
    );
    assert_eq!(empty_table(TABLE), None, "a table with rows is not empty");
}
