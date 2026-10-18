//! The mail a panel's auth sends, read back from a `MemoryTransport`.

use std::time::Duration;

use topcoat::mail::{Mail, MemoryTransport, TextBody};

/// The host every test's auth mail links to.
pub const BASE_URL: &str = "https://admin.example.com";

/// The mail `mail` holds once it holds `count`: auth mails after the response.
pub async fn mails(mail: &MemoryTransport, count: usize) -> Vec<Mail> {
    for _ in 0..500 {
        let sent = mail.sent();
        if sent.len() >= count {
            return sent;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expected {count} mails, got {}", mail.sent().len());
}

/// The plain-text body of `mail`.
pub fn text(mail: &Mail) -> &str {
    match mail.text() {
        TextBody::Text(text) => text,
        other => panic!("auth mail is plain text, got {other:?}"),
    }
}

/// The path and query of the link `mail` carries to [`BASE_URL`].
pub fn link(mail: &Mail) -> String {
    let start = text(mail).find(BASE_URL).expect("the mail carries a link");
    let url = text(mail)[start..].split_whitespace().next().unwrap();
    url.strip_prefix(BASE_URL).unwrap().to_string()
}

/// The token `link`'s query carries.
pub fn token(link: &str) -> String {
    let query = link.split_once('?').unwrap().1;
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.into_owned())
        .expect("the link carries a token")
}
