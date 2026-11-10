//! The declaration mistakes a panel refuses to mount with, and the one place their wording lives.

mod action_input;
mod area;
mod auth;
mod editable;
mod field_access;
mod field_rule;
mod kind;
mod locale;
mod many_to_many;
mod mount_error;
mod policy;
mod related_form;
mod segment;
mod site;
mod tenancy;
mod wording;

pub use action_input::ActionInputFault;
use action_input::fmt_action_input;
pub use area::AreaFault;
pub use auth::{
    AuthMailFault, OidcFault, ProfileFault, ResetFault, SignUpFault, TenantProfileFault,
    TenantRegistrationFault, TwoFactorFault, VerificationFault,
};
pub use editable::EditableColumnFault;
pub use field_access::FieldAccessFault;
pub use field_rule::FieldRuleFault;
use field_rule::fmt_field_rule;
pub use kind::DeclarationErrorKind;
pub use locale::LocaleFault;
pub use many_to_many::ManyToManyFault;
use many_to_many::fmt_many_to_many;
pub use mount_error::MountError;
pub use policy::PolicyFault;
pub use related_form::RelatedFormFault;
pub use segment::SegmentFault;
pub(crate) use segment::segment_fault;
pub use site::{DeclarationError, Site};
pub use tenancy::TenancyFault;

#[cfg(test)]
mod tests;
