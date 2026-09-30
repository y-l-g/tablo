//! Form decoding (urlencoded + streamed multipart) and create/edit handlers.
//!
//! Decoding helpers stay pure and request-free where possible so the size
//! caps and filename sanitization are unit-testable at the boundary.
//!
//! Body decoding lives in `decode`, the form-page shell and create page in
//! `render`, the app-side uniqueness probe in `unique`, the create/edit POST
//! pipelines in `submit`, and the helpers the pipelines share in `common`.

mod common;
mod decode;
mod render;
mod submit;
mod unique;

pub(crate) use self::{
    common::{MAX_FORM_BYTES, commit_write, resource_edit, truthy},
    decode::parse_form_body,
    render::resource_create,
    submit::{resource_create_post, resource_edit_post},
};
