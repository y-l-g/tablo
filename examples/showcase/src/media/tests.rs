use super::*;

#[test]
fn the_kind_follows_the_content_type() {
    assert_eq!(kind_of("image/png"), KIND_IMAGE);
    assert_eq!(kind_of("IMAGE/JPEG"), KIND_IMAGE);
    assert_eq!(kind_of(" text/plain"), KIND_FILE);
    assert_eq!(kind_of("application/pdf"), KIND_FILE);
    assert_eq!(kind_of(""), KIND_FILE);
}
