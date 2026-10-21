//! Form decoding (urlencoded + streamed multipart) and create/edit handlers.

mod common;
mod decode;
mod import;
mod render;
mod submit;
mod unique;

pub use import::MAX_IMPORT_ROWS;

pub(crate) use self::{
    common::{MAX_FORM_BYTES, resource_edit, truthy},
    decode::parse_form_body,
    import::{resource_import, resource_import_post, resource_import_template},
    render::{FormChrome, render_form_page, resource_create},
    submit::{resource_create_post, resource_edit_post},
};
