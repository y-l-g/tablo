#[derive(Debug, Clone, toasty::Embed)]
struct Meta {
    note: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Doc {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    meta: Meta,
}

fn doc() -> Doc {
    Doc {
        id: uuid::Uuid::nil(),
        title: "Draft".to_string(),
        meta: Meta {
            note: "Check".to_string(),
        },
    }
}

/// The steps a path takes from its model, for comparing two paths.
fn steps<M, T>(path: toasty::stmt::Path<M, T>) -> Vec<usize> {
    toasty_core::stmt::Path::from(path)
        .projection
        .as_slice()
        .to_vec()
}

#[test]
fn a_lens_pairs_a_field_path_with_its_reader() {
    let lens = lens!(Doc.title);
    assert_eq!(lens.read(&doc()), "Draft");
    assert_eq!(steps(lens.path().clone()), steps(Doc::fields().title()));
}

#[test]
fn a_lens_reaches_an_embedded_leaf() {
    let lens = lens!(Doc.meta.note);
    assert_eq!(lens.read(&doc()), "Check");
    assert_eq!(
        steps(lens.into()),
        steps(Doc::fields().meta().note()),
        "the path and the reader name the same leaf"
    );
}

#[test]
fn a_lens_names_its_model_by_path() {
    let lens = lens!(crate::lens::tests::Doc.title);
    assert_eq!(lens.read(&doc()), "Draft");
}
