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
    /// device or carry a control or bidirectional-override character, within the 255-byte cap.
    #[test]
    fn a_sanitized_filename_is_one_safe_segment(raw in "\\PC{0,400}|[./\\\\a-zA-Z\\x00-\\x1f\\u{202a}-\\u{202e}\\u{2066}-\\u{2069}]{0,40}") {
        let out = sanitize_filename(&raw);
        proptest::prop_assert!(!out.contains(['/', '\\']), "{out:?}");
        proptest::prop_assert!(out.len() <= 255, "{} bytes", out.len());
        proptest::prop_assert!(!out.chars().any(char::is_control), "{out:?}");
        proptest::prop_assert!(!out.chars().any(super::is_bidi_control), "{out:?}");
        proptest::prop_assert!(out != "." && out != "..", "{out:?}");
        proptest::prop_assert!(out.is_empty() || !is_windows_reserved_name(&out), "{out:?}");
    }
}

#[tokio::test]
async fn a_dir_uploader_holds_the_names_it_writes() {
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
    assert!(
        !uploader.holds("/uploads/other.png").await,
        "a file beside them, named by nobody, is not held"
    );
    let shaped = format!("{}-other.png", uuid::Uuid::new_v4());
    std::fs::write(dir.join(&shaped), b"x").unwrap();
    assert!(
        uploader.holds(&format!("/uploads/{shaped}")).await,
        "the name store writes is the check, not an index of what it wrote"
    );
    for unwritable in [
        uuid::Uuid::new_v4().simple().to_string(),
        uuid::Uuid::new_v4().to_string().to_uppercase(),
        format!("{}-", uuid::Uuid::new_v4()),
    ] {
        std::fs::write(dir.join(&unwritable), b"x").unwrap();
        assert!(
            !uploader.holds(&format!("/uploads/{unwritable}")).await,
            "a name store would not write is not held: {unwritable}"
        );
    }
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

/// A fresh directory of this test's own.
fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tablo-pending-{}", uuid::Uuid::new_v4()))
}

/// The file a `DirUploader` URL names in `dir`.
fn file(dir: &std::path::Path, url: &str) -> std::path::PathBuf {
    dir.join(url.strip_prefix("/uploads/").unwrap())
}

#[tokio::test]
async fn a_sweep_deletes_the_pending_files_and_keeps_the_kept_ones() {
    let dir = temp_dir();
    let uploader =
        DirUploader::new("/uploads", &dir).expire_pending(std::time::Duration::from_secs(3600));
    let kept = uploader.store("kept.png", b"KEPT").await.unwrap();
    let left = uploader.store("left.png", b"LEFT").await.unwrap();
    uploader.keep(&kept).await.unwrap();

    super::pending::sweep(&dir, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(file(&dir, &kept).is_file(), "a kept file stays");
    assert!(!file(&dir, &left).exists(), "a pending file is swept");
}

#[tokio::test]
async fn a_sweep_keeps_a_pending_file_younger_than_its_age() {
    let dir = temp_dir();
    let uploader =
        DirUploader::new("/uploads", &dir).expire_pending(std::time::Duration::from_secs(3600));
    let fresh = uploader.store("fresh.png", b"FRESH").await.unwrap();
    super::pending::sweep(&dir, std::time::Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(file(&dir, &fresh).is_file());
}

#[tokio::test]
async fn without_expiry_nothing_is_pending() {
    let dir = temp_dir();
    let uploader = DirUploader::new("/uploads", &dir);
    let stored = uploader.store("a.png", b"A").await.unwrap();
    super::pending::sweep(&dir, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(file(&dir, &stored).is_file());
}

#[tokio::test]
async fn a_keep_after_the_sweep_took_the_file_fails() {
    let dir = temp_dir();
    let uploader =
        DirUploader::new("/uploads", &dir).expire_pending(std::time::Duration::from_secs(3600));
    let swept = uploader.store("swept.png", b"SWEPT").await.unwrap();
    super::pending::sweep(&dir, std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(
        uploader.keep(&swept).await.is_err(),
        "a write must not commit a path the sweep deleted"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_sweep_finishes_a_claim_a_stopped_sweep_left() {
    let dir = temp_dir();
    let uploader =
        DirUploader::new("/uploads", &dir).expire_pending(std::time::Duration::from_secs(3600));
    let stored = uploader.store("half.png", b"HALF").await.unwrap();
    let name = stored.strip_prefix("/uploads/").unwrap();
    std::fs::rename(
        dir.join(".pending").join(name),
        dir.join(".pending").join(format!(".claimed-{name}")),
    )
    .unwrap();
    assert!(
        uploader.keep(&stored).await.is_err(),
        "a claimed file is not kept"
    );
    super::pending::sweep(&dir, std::time::Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(!file(&dir, &stored).exists());
    assert_eq!(std::fs::read_dir(dir.join(".pending")).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
