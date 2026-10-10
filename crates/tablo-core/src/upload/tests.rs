use super::{DirUploader, Uploader, is_windows_reserved_name, sanitize_filename};

#[test]
fn a_filename_reduces_to_its_basename_and_drops_reserved_names() {
    assert_eq!(sanitize_filename("upload.jpg"), "upload.jpg");
    assert_eq!(sanitize_filename("../../../etc/cron.d/x"), "x");
    assert_eq!(sanitize_filename("/abs/path"), "path");
    assert_eq!(sanitize_filename("C:\\fakepath\\x"), "x");
    assert_eq!(sanitize_filename(""), "");
    assert_eq!(sanitize_filename("."), "");
    assert_eq!(sanitize_filename(".."), "");
    assert_eq!(sanitize_filename("../.."), "");
    assert_eq!(sanitize_filename("..."), "...");
    assert_eq!(sanitize_filename("con"), "");
    assert_eq!(sanitize_filename("NUL"), "");
    assert_eq!(sanitize_filename("Com1.txt"), "");
    assert_eq!(sanitize_filename("lpt9"), "");
    assert_eq!(sanitize_filename("console.txt"), "console.txt");
    assert_eq!(sanitize_filename("companion"), "companion");
    assert_eq!(sanitize_filename("...."), "....");
    // The cap preserves the tail without splitting a multibyte char.
    let multibyte = format!("{}{}", "é".repeat(200), "a".repeat(200));
    let capped = sanitize_filename(&multibyte);
    assert!(
        capped.len() <= 255,
        "cap must bound bytes, got {}",
        capped.len()
    );
    assert!(
        capped.ends_with('a'),
        "tail must be preserved, got {capped:?}"
    );
}

proptest::proptest! {
    /// Whatever the client sends, the stored name is one path segment that cannot climb, name a
    /// device or carry a control character, within the 255-byte cap.
    #[test]
    fn a_sanitized_filename_is_one_safe_segment(raw in "\\PC{0,400}|[./\\\\a-zA-Z\\x00-\\x1f]{0,40}") {
        let out = sanitize_filename(&raw);
        proptest::prop_assert!(!out.contains(['/', '\\']), "{out:?}");
        proptest::prop_assert!(out.len() <= 255, "{} bytes", out.len());
        proptest::prop_assert!(!out.chars().any(char::is_control), "{out:?}");
        proptest::prop_assert!(out != "." && out != "..", "{out:?}");
        proptest::prop_assert!(out.is_empty() || !is_windows_reserved_name(&out), "{out:?}");
    }
}

#[tokio::test]
async fn a_dir_uploader_holds_what_it_stored_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("tablo-dir-uploader-{}", uuid::Uuid::new_v4()));
    let uploader = DirUploader::new("/uploads/", &dir);
    let path = uploader.store("my cover é.png", b"png").await.unwrap();
    let segment = path.strip_prefix("/uploads/").expect("under the prefix");
    assert!(
        segment.ends_with("-my%20cover%20%C3%A9.png"),
        "one encoded segment, got {path}"
    );
    let name = percent_encoding::percent_decode_str(segment)
        .decode_utf8()
        .unwrap();
    assert_eq!(std::fs::read(dir.join(&*name)).unwrap(), b"png");
    assert!(uploader.holds(&path).await);

    std::fs::write(dir.join("other.png"), b"x").unwrap();
    for refused in [
        "/uploads/missing.png",
        "/uploads/",
        "/elsewhere/other.png",
        "/uploads/%2E%2E",
        "/uploads/..%2Fother.png",
        "/uploads/sub/other.png",
    ] {
        assert!(!uploader.holds(refused).await, "{refused}");
    }
    assert!(
        !uploader.holds("/uploads/other%2Epng").await,
        "a spelling store never returns"
    );
    assert!(uploader.holds("/uploads/other.png").await);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_dir_uploader_keeps_a_long_name_within_the_filesystem_limit() {
    let dir = std::env::temp_dir().join(format!("tablo-dir-uploader-{}", uuid::Uuid::new_v4()));
    let uploader = DirUploader::new("/uploads", &dir);
    let long = format!("{}.png", "é".repeat(200));
    let path = uploader.store(&long, b"png").await.unwrap();
    let segment = path.strip_prefix("/uploads/").unwrap();
    let name = percent_encoding::percent_decode_str(segment)
        .decode_utf8()
        .unwrap();
    assert!(name.len() <= 255, "{} bytes", name.len());
    assert!(name.ends_with("éé.png"), "the tail is kept: {name}");
    assert!(uploader.holds(&path).await);

    let bare = uploader.store("..", b"x").await.unwrap();
    assert!(
        uploader.holds(&bare).await,
        "a refused name stores as its UUID"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
