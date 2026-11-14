//! The mail a panel's auth sends, read back from a `MemoryTransport`.

use topcoat::mail::Mail;

/// The host every test's auth mail links to.
pub const BASE_URL: &str = "https://admin.example.com";

/// The plain-text body of `mail`.
pub fn text(mail: &Mail) -> &str {
    mail.text().expect("auth mail has a plain-text body")
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

/// The six-digit code `mail` carries.
pub fn code(mail: &Mail) -> String {
    text(mail)
        .split(|c: char| !c.is_ascii_digit())
        .find(|run| run.len() == 6)
        .expect("the mail carries a six-digit code")
        .to_string()
}
