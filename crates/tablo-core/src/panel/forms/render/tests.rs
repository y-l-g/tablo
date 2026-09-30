#[test]
fn create_form_multipart_predicate_follows_file_upload() {
    // GH #136 layer rule: core owns the "does the form hold a file field"
    // predicate `render_form_page` maps to `enctype="multipart/form-data"`;
    // the showcase (`posts_create_form_is_multipart` /
    // `users_create_form_stays_urlencoded`) owns the HTTP enctype wiring.
    use crate::schema::{Field, Schema};

    #[derive(Debug, toasty::Model, Clone)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
        title: String,
    }
    let with_file = Schema::new(Field::file(Doc::fields().path()));
    let without_file = Schema::new(Field::text(Doc::fields().title()));
    assert!(
        with_file.fields().any(|field| field.is_file()),
        "file schema must report an upload"
    );
    assert!(
        !without_file.fields().any(|field| field.is_file()),
        "plain schema must report no upload"
    );
}
