use super::*;

/// The RFC 6238 appendix B secret for SHA-1.
const RFC_SECRET: &[u8] = b"12345678901234567890";

/// RFC 6238 appendix B: the SHA-1 codes, eight digits, at each listed time.
#[test]
fn codes_match_the_rfc_6238_vectors() {
    for (time, code) in [
        (59, 94_287_082),
        (1_111_111_109, 7_081_804),
        (1_111_111_111, 14_050_471),
        (1_234_567_890, 89_005_924),
        (2_000_000_000, 69_279_037),
        (20_000_000_000, 65_353_130),
    ] {
        assert_eq!(hotp(RFC_SECRET, step(time), 8), code, "at {time}");
    }
}

/// RFC 4226 appendix D: the six-digit codes for the first counters.
#[test]
fn codes_match_the_rfc_4226_vectors() {
    for (counter, code) in [755_224, 287_082, 359_152, 969_429, 338_314]
        .into_iter()
        .enumerate()
    {
        assert_eq!(hotp(RFC_SECRET, counter as u64, 6), code);
    }
}

/// A code verifies within one step of now, once, and not as anything else.
#[test]
fn a_code_verifies_within_its_window_once() {
    let now = step(1_111_111_111);
    let code = |step| format!("{:06}", hotp(RFC_SECRET, step, 6));

    assert_eq!(verify(RFC_SECRET, &code(now), now, None), Some(now));
    assert_eq!(verify(RFC_SECRET, &code(now - 1), now, None), Some(now - 1));
    assert_eq!(verify(RFC_SECRET, &code(now + 1), now, None), Some(now + 1));
    assert_eq!(verify(RFC_SECRET, &code(now - 2), now, None), None);
    assert_eq!(verify(RFC_SECRET, &code(now), now, Some(now)), None);
    assert_eq!(verify(RFC_SECRET, &code(now - 1), now, Some(now - 1)), None);
    assert_eq!(verify(RFC_SECRET, " 1234567 ", now, None), None);
    assert_eq!(verify(RFC_SECRET, "abcdef", now, None), None);
}

/// The set-up URI names the secret in base32 and labels the entry with issuer and account.
#[test]
fn the_uri_carries_the_secret_and_the_label() {
    let uri = uri(RFC_SECRET, "Tablo Blog", "ada@example.com");
    assert!(uri.starts_with("otpauth://totp/Tablo%20Blog%3Aada%40example.com?"));
    assert!(uri.contains("secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
    assert!(uri.contains("issuer=Tablo+Blog"));
}
