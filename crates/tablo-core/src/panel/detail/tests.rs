use topcoat::context::CxTestBuilder;

use super::*;

#[derive(Debug, Clone, toasty::Model)]
struct Note {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
}

/// A resource with no label: the title keeps the page name and the record
/// key.
struct Unlabelled;

impl Resource for Unlabelled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn table(_cx: &Cx) -> crate::resource::Table<Note> {
        crate::resource::Table::new(
            |r: &Note| r.id.to_string(),
            crate::resource::TextColumn::r#for(Note::fields().title(), |r: &Note| r.title.clone()),
        )
    }
}

/// A resource that labels its records with the note's title.
struct Labelled;

impl Resource for Labelled {
    type Model = Note;
    type Form = crate::NoForm<Self::Model>;

    fn table(_cx: &Cx) -> crate::resource::Table<Note> {
        crate::resource::Table::new(
            |r: &Note| r.id.to_string(),
            crate::resource::TextColumn::r#for(Note::fields().title(), |r: &Note| r.title.clone()),
        )
    }

    fn record_label(_cx: &Cx, record: &Note) -> Option<String> {
        Some(record.title.clone())
    }
}

fn note() -> Note {
    Note {
        id: uuid::Uuid::nil(),
        title: "A Title".to_string(),
    }
}

#[test]
fn a_resource_without_a_label_titles_the_page_with_the_record_key() {
    let cx = CxTestBuilder::new().build();
    assert_eq!(
        detail_title::<Unlabelled>(&cx, &note(), "8f14e45f"),
        "Note 8f14e45f"
    );
}

#[test]
fn a_declared_label_titles_the_page() {
    let cx = CxTestBuilder::new().build();
    assert_eq!(
        detail_title::<Labelled>(&cx, &note(), "8f14e45f"),
        "A Title"
    );
}
