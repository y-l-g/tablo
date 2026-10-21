use super::{Column, Refusal, resolve_header, row_pairs};

fn column(key: &str, label: &str, multiple: bool) -> Column {
    Column {
        key: key.to_string(),
        label: label.to_string(),
        multiple,
        required: false,
    }
}

fn columns() -> Vec<Column> {
    vec![
        column("name", "Full name", false),
        column("email", "Email", false),
        column("categories", "Categories", true),
    ]
}

fn header(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_header_cell_names_a_column_by_key_or_by_label_in_any_case() {
    assert_eq!(
        resolve_header(&header(&["FULL NAME", " email ", ""]), &columns()),
        Ok(vec![Some(0), Some(1), None])
    );
}

#[test]
fn a_header_naming_no_column_refuses_the_file() {
    assert_eq!(
        resolve_header(&header(&["name", "tenant_id", "id"]), &columns()),
        Err(Refusal::UnknownColumns(header(&["tenant_id", "id"])))
    );
}

/// A multiple choice's control posts its key once per value, so only its column may repeat.
#[test]
fn only_a_multiple_choice_column_may_repeat() {
    assert_eq!(
        resolve_header(&header(&["categories", "Categories"]), &columns()),
        Ok(vec![Some(2), Some(2)])
    );
    assert_eq!(
        resolve_header(&header(&["name", "Full name"]), &columns()),
        Err(Refusal::RepeatedColumns(header(&["Full name"])))
    );
}

#[test]
fn a_row_short_of_cells_posts_its_missing_cells_blank() {
    let pairs = row_pairs(2, &header(&["Ada"]), &[Some(0), Some(1)], &columns()).ok();
    assert_eq!(
        pairs,
        Some(vec![
            ("name".to_string(), "Ada".to_string()),
            ("email".to_string(), String::new()),
        ])
    );
}

#[test]
fn a_value_under_no_column_refuses_the_row() {
    assert!(row_pairs(2, &header(&["Ada", "x"]), &[Some(0), None], &columns()).is_err());
    assert!(row_pairs(2, &header(&["Ada", "", "y"]), &[Some(0), None], &columns()).is_err());
    assert!(row_pairs(2, &header(&["Ada", " "]), &[Some(0), None], &columns()).is_ok());
}

#[test]
fn a_label_matches_in_any_case_of_any_script() {
    let columns = vec![column("grade", "Évaluation", false)];
    assert_eq!(
        resolve_header(&header(&["ÉVALUATION"]), &columns),
        Ok(vec![Some(0)])
    );
}

/// A label two fields carry names neither: the header names the column by its key.
#[test]
fn a_label_two_fields_carry_refuses_the_file() {
    let columns = vec![
        column("billing_name", "Name", false),
        column("shipping_name", "Name", false),
    ];
    assert_eq!(
        resolve_header(&header(&["name"]), &columns),
        Err(Refusal::AmbiguousColumns(header(&["name"])))
    );
    assert_eq!(
        resolve_header(&header(&["billing_name", "shipping_name"]), &columns),
        Ok(vec![Some(0), Some(1)])
    );
}
