use proptest::prelude::*;

use super::*;

fn sender() -> Mailbox {
    Mailbox::named("Example", "no-reply@example.com").expect("a valid sender")
}

proptest! {
    /// A letter read back from its JSON rebuilds the mail it was written from, `Date` and
    /// `Message-ID` included, so a queue that sends it twice sends one mail.
    #[test]
    fn a_queued_letter_rebuilds_the_same_mail(
        subject in "\\PC{0,40}",
        text in "\\PC{1,200}",
        expires in proptest::option::of(0i64..4_000_000_000),
    ) {
        let expires_at = expires.map(|seconds| Timestamp::from_second(seconds).unwrap());
        let letter = AuthLetter::new(
            AuthLetterKind::PasswordReset,
            &sender(),
            "ada@example.com",
            subject,
            text,
            expires_at,
        )
        .unwrap();
        let read: AuthLetter =
            serde_json::from_str(&serde_json::to_string(&letter).unwrap()).unwrap();
        prop_assert_eq!(&read, &letter);
        let (sent, resent) = (letter.mail().unwrap(), read.mail().unwrap());
        prop_assert_eq!(sent.message_id(), resent.message_id());
        prop_assert_eq!(sent.date(), resent.date());
        prop_assert_eq!(sent.formatted(), resent.formatted());
        prop_assert_eq!(read.expires_at(), expires_at);
    }
}

/// A letter's `Debug` output leaves out the body, which carries the live link or code.
#[test]
fn debug_output_leaves_the_link_out() {
    let letter = AuthLetter::new(
        AuthLetterKind::TwoFactorCode,
        &sender(),
        "ada@example.com",
        "Your code".to_string(),
        "Your code is 123456".to_string(),
        None,
    )
    .unwrap();
    let debug = format!("{letter:?}");
    assert!(!debug.contains("123456"), "{debug}");
}
