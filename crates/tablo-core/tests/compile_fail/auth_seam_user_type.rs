use tablo_core::{Auth, Authenticator, PanelUser, PasswordAuth, PasswordReset, PasswordSignUp};
use topcoat::context::Cx;

struct Staff;

impl PanelUser for Staff {
    fn user_id(&self) -> String {
        String::new()
    }

    fn display_name(&self) -> &str {
        ""
    }

    fn can_access_panel(&self, _panel: &str) -> bool {
        true
    }
}

struct StaffAuth;

impl Authenticator for StaffAuth {
    type User = Staff;

    async fn verify(&self, _: &Cx, _: &str, _: &str) -> topcoat::Result<Option<Staff>> {
        Ok(None)
    }

    async fn find_by_id(&self, _: &Cx, _: &str) -> topcoat::Result<Option<Staff>> {
        Ok(None)
    }
}

fn main() {
    // Each seam registers, edits or resets an `AdminUser`; the authenticator loads `Staff`.
    let _ = Auth::custom(StaffAuth).registration(PasswordSignUp::open());
    let _ = Auth::custom(StaffAuth).profile(PasswordAuth);
    let _ = Auth::custom(StaffAuth).password_reset(PasswordReset::new(PasswordAuth));
}
