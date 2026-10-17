use super::*;
use crate::{
    form::FieldErrorKind,
    schema::{Field, Source},
    test_support::Html,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, crate::Options)]
enum Kind {
    Docs,
    Blog,
}

#[derive(Debug, Clone, PartialEq, toasty::Embed, crate::FormInput)]
struct Link {
    label: String,
    #[form(optional)]
    url: String,
    #[form(options = Kind, optional)]
    kind: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Page {
    #[key]
    #[auto]
    id: uuid::Uuid,
    shown: String,
    #[document]
    links: Vec<Link>,
}

/// A body posting `pairs`, in order.
fn posted(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A row's or a form's text values.
fn texts(pairs: &[(&str, &str)]) -> FormValues {
    pairs.iter().copied().collect()
}

fn link(label: &str, url: &str) -> Link {
    Link {
        label: label.to_string(),
        url: url.to_string(),
        kind: String::new(),
    }
}

fn repeater() -> Schema {
    Schema::new(Field::repeater(Page::fields().links()))
}

#[test]
fn a_read_takes_the_rows_in_the_order_the_key_lists_them() {
    let values = repeater()
        .read(&posted(&[
            ("links", "4, 1"),
            ("links.1.label", "Docs"),
            ("links.1.url", "https://docs.example"),
            ("links.4.label", "Blog"),
        ]))
        .unwrap();
    assert_eq!(
        values.rows("links").unwrap(),
        [
            texts(&[("label", "Blog")]),
            texts(&[("label", "Docs"), ("url", "https://docs.example")])
        ]
    );
}

#[test]
fn a_read_leaves_what_no_listed_row_of_the_item_posts_unclaimed() {
    let (values, unclaimed) = repeater()
        .read_unclaimed(&posted(&[
            ("links", "0"),
            ("links.0.label", "Docs"),
            ("links.0.secret", "x"),
            ("links.1.label", "Unlisted"),
        ]))
        .unwrap();
    assert_eq!(values.rows("links").unwrap(), [texts(&[("label", "Docs")])]);
    assert_eq!(unclaimed, ["links.0.secret", "links.1.label"]);
}

#[test]
fn a_submission_not_posting_the_repeater_reads_nothing_and_an_empty_list_reads_no_rows() {
    let values = repeater().read(&posted(&[("title", "kept")])).unwrap();
    assert!(values.is_empty(), "an edit keeps the stored rows");

    let values = repeater().read(&posted(&[("links", "")])).unwrap();
    assert_eq!(values.rows("links"), Some(&[][..]));
}

#[test]
fn a_read_refuses_an_order_that_lists_no_row_or_a_row_twice() {
    let rows = (0..=MAX_ROWS)
        .map(|row| row.to_string())
        .collect::<Vec<_>>();
    let mut too_many = posted(&[("links", &rows.join(","))]);
    for row in &rows {
        too_many.push((format!("links.{row}.label"), "a".to_string()));
    }
    let error = repeater().read(&too_many).unwrap_err();
    assert!(error.contains("more than"), "{error}");
    too_many.pop();
    too_many[0].1 = rows[..MAX_ROWS].join(",");
    repeater().read(&too_many).expect("the cap itself reads");

    for order in ["first", "-1", "0,0", "0"] {
        assert!(
            repeater().read(&posted(&[("links", order)])).is_err(),
            "{order} is refused"
        );
    }
}

#[test]
fn the_items_parse_from_the_rows_and_write_back_to_them() {
    let cx = crate::test_support::cx();
    let items = vec![link("Docs", "https://docs.example"), link("Blog", "")];
    let values = FormValues::from_iter([("links", write_items(&items))]);
    assert_eq!(parse_items::<Link>(&cx, "links", &values).unwrap(), items);
    assert_eq!(
        parse_items::<Link>(&cx, "links", &FormValues::new()).unwrap(),
        Vec::new(),
        "a repeater posting nothing holds no items"
    );
}

#[test]
fn a_refusal_names_the_key_its_row_posts() {
    let cx = crate::test_support::cx();
    let rows = vec![
        texts(&[("label", "Docs")]),
        texts(&[("label", ""), ("url", "x")]),
        texts(&[("label", " ")]),
    ];
    let values = FormValues::from_iter([("links", rows)]);
    let errors = parse_items::<Link>(&cx, "links", &values).unwrap_err();
    let keys: Vec<&str> = errors.iter().map(|error| error.key.as_str()).collect();
    assert_eq!(keys, ["links.1.label", "links.2.label"]);
    assert!(matches!(errors[0].kind, FieldErrorKind::Required));

    let errors = parse_items::<Link>(&cx, "links", &texts(&[("links", "Docs")])).unwrap_err();
    assert_eq!(
        errors[0].key, "links",
        "a value holding no rows is the repeater's own"
    );
}

#[tokio::test]
async fn the_repeater_renders_a_row_per_item_and_a_blank_row_to_copy() {
    let cx = crate::test_support::cx();
    let values = FormValues::from_iter([(
        "links",
        write_items(&[link("Docs", "https://docs.example"), link("Blog", "")]),
    )]);
    let mut errors = crate::form::FieldErrors::new();
    errors.push(FieldError::required("links.1.label"));
    let html = repeater()
        .render(&cx, Source::form(&values, &errors).scoped("dialog"))
        .await
        .html(&cx)
        .await;
    for needle in [
        r#"data-repeater="links""#,
        r#"data-repeater-next="2""#,
        r#"name="links" value="0,1""#,
        r#"name="links.0.label""#,
        r#"id="dialog-links.0.label""#,
        r#"value="https://docs.example""#,
        r#"id="dialog-links.1.label-error""#,
        "Label is required",
        r#"name="links.__row__.url""#,
        r#"data-repeater-row="__row__""#,
        "Add item",
    ] {
        assert!(html.contains(needle), "no {needle} in {html}");
    }
    let template = &html[html.find("<template").unwrap()..];
    assert!(
        !template.contains("is required"),
        "the blank row carries no error: {template}"
    );
}

#[tokio::test]
async fn each_row_answers_to_its_items_control_rules() {
    let cx = crate::test_support::cx();
    let schema = repeater();
    let values = schema
        .read(&posted(&[
            ("links", "0,1"),
            ("links.0.label", "Docs"),
            ("links.0.kind", "docs"),
            ("links.1.label", "Blog"),
            ("links.1.kind", "bogus"),
        ]))
        .unwrap();
    let errors = schema.checked(&cx, &values).await;
    let keys: Vec<&str> = errors.iter().map(|error| error.key.as_str()).collect();
    assert_eq!(
        keys,
        ["links.1.kind"],
        "only the second row's choice is refused"
    );
}

#[tokio::test]
async fn a_repeater_hidden_by_a_condition_posts_nothing() {
    let cx = crate::test_support::cx();
    let shown = Field::choice(Page::fields().shown()).options(["yes", "no"]);
    let links = Field::repeater(Page::fields().links()).visible_when(&shown, ["yes"]);
    let schema = Schema::new((shown, links));
    let values = FormValues::from_iter([
        ("shown", FormValue::from("no")),
        ("links", write_items(&[link("Docs", "")])),
    ]);
    assert!(schema.hidden_fields(&values).contains("links"));
    let errors = crate::form::FieldErrors::new();
    let html = schema
        .render(&cx, Source::form(&values, &errors))
        .await
        .html(&cx)
        .await;
    let own = html[..html.find("data-repeater=").unwrap()]
        .rfind("<fieldset")
        .unwrap();
    let guard = &html[html[..own].rfind("<fieldset").unwrap()..own];
    assert!(
        guard.contains("disabled") && guard.contains("hidden"),
        "the guard disables the rows: {guard}"
    );
}

#[test]
fn the_column_lists_each_item_as_label_value_pairs() {
    let cx = crate::test_support::cx();
    let page = Page {
        id: uuid::Uuid::nil(),
        shown: String::new(),
        links: vec![
            Link {
                kind: "docs".to_string(),
                ..link("Guide", "https://docs.example")
            },
            Link {
                kind: "blog".to_string(),
                ..link("Notes", "")
            },
        ],
    };
    let column = crate::RepeaterColumn::new(crate::lens!(Page.links));
    assert_eq!(
        crate::table::Column::text(&column, &cx, &page),
        "Label: Guide, Url: https://docs.example, Kind: Docs; Label: Notes, Url: , Kind: Blog"
    );
}

/// The add button's opening tag in `html`.
fn add_button(html: &str) -> &str {
    let at = html.find("data-repeater-add").expect("an add button");
    let start = html[..at].rfind("<button").expect("a button tag");
    &html[start..at + html[at..].find('>').expect("the tag ends")]
}

/// A repeater holding as many rows as it takes renders its add button disabled, and names the
/// cap the script keeps it at.
#[tokio::test]
async fn a_full_repeater_disables_its_add_button() {
    let cx = crate::test_support::cx();
    for (count, disabled) in [(MAX_ROWS - 1, false), (MAX_ROWS, true)] {
        let items = vec![link("Docs", ""); count];
        let values = FormValues::from_iter([("links", write_items(&items))]);
        let html = repeater()
            .render(&cx, Source::form(&values, &crate::form::FieldErrors::new()))
            .await
            .html(&cx)
            .await;
        assert!(html.contains(&format!(r#"data-repeater-max="{MAX_ROWS}""#)));
        let tag = add_button(&html);
        let attribute = tag
            .split(' ')
            .any(|part| part == "disabled" || part.starts_with("disabled="));
        assert_eq!(attribute, disabled, "{count} rows: {tag}");
    }
}
