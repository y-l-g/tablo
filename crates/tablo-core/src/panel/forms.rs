//! The create, edit and import handlers, and the form page they render.

mod common;
mod import;
mod render;
mod submit;
mod unique;

pub(crate) use self::{
    common::{parse_form_body, resource_edit, truthy},
    import::{resource_import, resource_import_post, resource_import_template},
    render::{Carried, FormChrome, render_form_page, resource_create},
    submit::{resource_create_post, resource_edit_post},
};
