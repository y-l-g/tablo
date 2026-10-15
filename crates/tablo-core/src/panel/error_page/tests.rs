use topcoat::router::error::{
    bad_request, forbidden, internal_server_error, not_found, redirect, unauthorized,
};

use super::*;

/// A 404, a 403 and a 500 show their page; an error the router knows nothing of fails as a 500.
#[test]
fn failures_show_their_page() {
    assert_eq!(
        PanelError::of(&not_found().into()),
        Some(PanelError::NotFound)
    );
    assert_eq!(
        PanelError::of(&forbidden().into()),
        Some(PanelError::Forbidden)
    );
    assert_eq!(
        PanelError::of(&internal_server_error(topcoat::Error::msg("boom")).into()),
        Some(PanelError::Failed)
    );
    assert_eq!(
        PanelError::of(&topcoat::Error::msg("an app's own error")),
        Some(PanelError::Failed)
    );
}

/// An error with an answer of its own keeps it: the browser follows a redirect, and a 401 or a 400
/// is for its caller to read.
#[test]
fn answered_errors_pass_through() {
    for error in [
        redirect("/admin/login").into(),
        unauthorized().into(),
        bad_request("a malformed form").into(),
    ] {
        assert_eq!(PanelError::of(&error), None, "{error}");
    }
}
