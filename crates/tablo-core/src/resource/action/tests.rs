use topcoat::context::Cx;

use super::Action;
use crate::{
    NoForm, Resource,
    test_support::{User, cx},
};

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = NoForm<User>;
}

struct SendInvite;

impl Action<UserResource> for SendInvite {
    type Input = ();
    const NAME: &'static str = "send-invite";

    async fn run(_: &Cx, _: &[User], _: (), _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

#[test]
fn the_label_defaults_to_the_name_in_sentence_case() {
    assert_eq!(SendInvite::label(&cx()), "Send invite");
}
