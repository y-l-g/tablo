use std::collections::HashMap;

use topcoat::context::CxTestBuilder;

use super::{
    super::test_support::{DummyUser, attributes_of, opening_tag_at},
    *,
};
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
    test_support::Html as _,
};

#[tokio::test]
async fn searchable_select_renders_filter_input() {
    // Opt-in client-side option search; default selects stay bare.
    let cx = CxTestBuilder::new().build();
    let plain = Field::choice(DummyUser::fields().name()).options(vec!["a".to_string()]);
    let html = plain.render(&cx, None, None).await.html(&cx).await;
    assert!(
        !html.contains("data-options-filter"),
        "default select must stay bare, got {html}"
    );
    let searchable = Field::choice(DummyUser::fields().name())
        .options(vec!["a".to_string()])
        .searchable();
    let html = searchable.render(&cx, None, None).await.html(&cx).await;
    assert!(
        html.contains("data-options-filter"),
        "searchable select must render the filter hook, got {html}"
    );
    assert!(
        html.contains("data-select-filterable"),
        "searchable select must scope the filter, got {html}"
    );
    // The filter is only useful with a list it can narrow. The
    // native popup is browser chrome the script cannot touch, so the
    // searchable markup carries its own listbox — rendered empty and
    // hidden, and filled by `selects.js`.
    assert!(
        html.contains("data-options-combobox") && html.contains("data-options-list"),
        "searchable select must render the suggestion list, got {html}"
    );
    assert!(
        html.contains("role=\"listbox\""),
        "the suggestion list must be a listbox, got {html}"
    );
    let list_at = html.find("data-options-list").expect("the list");
    let list_tag_start = html[..list_at].rfind("<ul").expect("its <ul>");
    let list_tag_end = html[list_tag_start..].find('>').expect("the tag's end");
    // The `hidden` HTML boolean attribute, not a Tailwind class (`<ul>`'s
    // `class` carries none): the list must render hidden until the field is
    // used. Pinned as `hidden=""` so a class that merely contains the word
    // cannot satisfy it.
    assert!(
        html[list_tag_start..list_tag_start + list_tag_end].contains("hidden=\"\""),
        "the list must render hidden until the field is used, got {html}"
    );
    // The input is the combobox, statically wired to the list it
    // filters. It starts collapsed over the hidden list, and names the
    // listbox; `selects.js` keeps `aria-expanded` and
    // `aria-activedescendant` in step with the popup.
    let filter_attrs = attributes_of(&html, "data-options-filter");
    for expected in [
        "role=\"combobox\"",
        "aria-expanded=\"false\"",
        "aria-controls=\"name-options-list\"",
        "aria-autocomplete=\"list\"",
    ] {
        assert!(
            filter_attrs.iter().any(|attr| attr == expected),
            "the filter input must carry {expected}, got {filter_attrs:?}"
        );
    }
    let list_attrs = attributes_of(&html, "data-options-list");
    assert!(
        list_attrs
            .iter()
            .any(|attr| attr == "id=\"name-options-list\""),
        "the listbox must carry the id the combobox controls, got {list_attrs:?}"
    );
}

#[tokio::test]
async fn select_renders_through_the_select_primitive() {
    // The schema select composes the synced `select` primitive, so it
    // matches the `input` beside it and `selects.js` keeps finding the
    // control inside the filterable field.
    let cx = CxTestBuilder::new().build();
    let schema =
        Schema::new(Field::choice(DummyUser::fields().name()).options(vec!["a".to_string()]));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
    // The primitive's *chrome* is paint; what it composes is
    // structural — the native `<select>` sits inside the primitive's
    // wrapper `<span>`, which carries the checkmark style hook and the
    // chevron icon.
    let select_start = html.find("<select").expect("native select element");
    let wrapper_start = html[..select_start]
        .rfind("<span")
        .expect("the primitive's wrapper span");
    let wrapper_tag = opening_tag_at(&html, wrapper_start);
    assert!(
        wrapper_tag.contains("--select-checkmark"),
        "select must compose the select primitive's wrapper, got {wrapper_tag}"
    );
    assert!(
        html[select_start..].contains("<svg"),
        "the primitive's chevron must render, got {html}"
    );
    // `Attributes` renders in no guaranteed order (topcoat#122), so slice
    // the whole opening tag; quoting is honoured, so a `>` inside the
    // picker's Tailwind selectors does not end it early.
    let tag = opening_tag_at(&html, select_start);
    assert!(
        tag.contains("name=\"name\"")
            && tag.contains("id=\"name\"")
            && tag.contains("aria-invalid=\"false\""),
        "attributes must reach the native control, got {tag}"
    );
}
