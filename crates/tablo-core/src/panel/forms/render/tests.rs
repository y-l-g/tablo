#[test]
fn create_form_multipart_predicate_follows_file_upload() {
    // GH #136 layer rule: core owns the `has_file_upload` predicate
    // (see also `has_file_upload_detects_nested` for nested containers);
    // the showcase (`posts_create_form_is_multipart` /
    // `users_create_form_stays_urlencoded`) owns the HTTP enctype wiring
    // (`render_form_page` maps this predicate to
    // `enctype="multipart/form-data"` one-to-one).
    use crate::schema::{FileUpload, Schema, TextInput};

    #[derive(Debug, toasty::Model, Clone)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
        title: String,
    }
    let with_file = Schema::new(FileUpload::r#for(Doc::fields().path()));
    let without_file = Schema::new(TextInput::r#for(Doc::fields().title()));
    assert!(
        with_file.has_file_upload(),
        "file schema must report an upload"
    );
    assert!(
        !without_file.has_file_upload(),
        "plain schema must report no upload"
    );
}
