//! Form decoding (urlencoded + streamed multipart) and create/edit handlers.

mod common;
mod decode;
mod render;
mod submit;
mod unique;

pub(crate) use self::{
    common::{MAX_FORM_BYTES, resource_edit, truthy},
    decode::parse_form_body,
    render::resource_create,
    submit::{resource_create_post, resource_edit_post},
};
