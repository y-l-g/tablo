use super::*;

const TABLE: &str = r#"<table><thead><tr><th>Name</th></tr></thead><tbody>
<tr id="row-Ada-12345678"><td><input type="checkbox" value="Ada" aria-label="Select row"></td><td>Ada</td><td><div class="flex"><a href="/admin/dummies/Ada" aria-label="View">V</a><a href="/admin/dummies/Ada/edit" aria-label="Edit">E</a><button type="button" formaction="/admin/dummies/Ada/delete" aria-label="Delete">D</button></div></td></tr>
<tr id="group-Active-87654321" data-group="Active"><td colspan="3">Active (1 on this page)</td></tr>
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

/// A group holds the rows from its header to the next one; a row before any header is in none.
#[test]
fn groups_hold_the_rows_under_their_header() {
    let found = groups(TABLE);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "Active");
    let names: Vec<&str> = found[0]
        .rows
        .iter()
        .filter_map(|row| row.cells.iter().find(|cell| !cell.is_empty()))
        .map(String::as_str)
        .collect();
    assert_eq!(names, ["Ken", "NoBox", "Prefixed"]);
    assert!(groups("<table><tr id=\"row-Ada-1\"><td>Ada</td></tr></table>").is_empty());
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
    let html = r#"<div class="ac-field ac-field--error"><input name="name" aria-invalid="true" aria-describedby="name-error"><div id="name-error" class="ac-error" data-error="required">Name is required</div></div>
<div id="age-error" data-error="invalid">Age is at most 10</div><div id="tag-error" data-error="unavailable">Tag is not available</div>"#;
    assert_eq!(field_error(html, "name"), Some(ErrorKind::Required));
    assert_eq!(field_error(html, "age"), Some(ErrorKind::Invalid));
    assert_eq!(field_error(html, "tag"), Some(ErrorKind::Unavailable));
    assert_eq!(field_error(html, "email"), None);
    assert_eq!(
        field_error_message(html, "age").as_deref(),
        Some("Age is at most 10")
    );
}

#[test]
fn the_button_posting_an_action_tells_whether_it_asks_first() {
    let html = r#"<button type="button" formaction="/admin/posts/1/delete" data-confirm="">D</button>
<button type="submit" formaction="/admin/posts/1/actions/publish">P</button>"#;
    assert_eq!(confirms_first(html, "/admin/posts/1/delete"), Some(true));
    assert_eq!(
        confirms_first(html, "/admin/posts/1/actions/publish"),
        Some(false)
    );
    assert_eq!(confirms_first(html, "/admin/posts/1/actions/archive"), None);
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
fn filter_options_reads_a_checklist() {
    let html = r#"<div data-filter-name="status"><span>Status</span><button type="button">draft</button><div popover=""><label><span><input type="checkbox" name="f.status" value="draft" checked=""><svg></svg></span><span>Draft</span></label><label><span><input type="checkbox" name="f.status" value="published"></span><span>Published</span></label></div></div>"#;
    assert_eq!(
        filter_options(html, "status"),
        Some(vec![
            FilterOption {
                value: "draft".to_string(),
                label: "Draft".to_string(),
                selected: true,
            },
            FilterOption {
                value: "published".to_string(),
                label: "Published".to_string(),
                selected: false,
            },
        ])
    );
}

#[test]
fn filter_options_reads_a_checklist_with_no_option() {
    let html = r#"<div data-filter-name="status"><span id="f.status-label">Status</span><button type="button" popovertarget="f.status-options" aria-labelledby="f.status-label f.status-options-summary"><span id="f.status-options-summary">All</span></button><div id="f.status-options" popover="" role="group"><p>No options</p></div></div>"#;
    assert_eq!(
        filter_options(html, "status"),
        Some(Vec::new()),
        "a checklist with no option is still a control"
    );
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
            reason: EmptyReason::Search,
            clear: Some("/admin/users".to_string()),
            first_page: Some("/admin/users?q=x".to_string()),
        })
    );
    let bare = r#"<div data-empty="none"><p>No records yet</p></div>"#;
    assert_eq!(
        empty_table(bare).map(|empty| (empty.reason, empty.clear, empty.first_page)),
        Some((EmptyReason::NoRecords, None, None))
    );
    assert_eq!(empty_table(TABLE), None, "a table with rows is not empty");
}

#[test]
fn an_element_reads_through_nested_elements_of_its_own_name() {
    let html = r#"<div data-empty="filters"><div><svg></svg></div><p>None</p><div><a data-empty-link="clear" href="/admin/posts">Clear filters</a></div></div>"#;
    assert_eq!(
        empty_table(html).and_then(|empty| empty.clear).as_deref(),
        Some("/admin/posts"),
        "the link after the first nested `div` closes is still inside"
    );
}
