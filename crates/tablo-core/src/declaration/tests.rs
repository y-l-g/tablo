use super::*;

#[test]
fn a_route_segment_refuses_what_a_url_cannot_carry_literally() {
    assert_eq!(segment_fault("publish"), None);
    assert_eq!(segment_fault("café-crème"), None);
    assert_eq!(segment_fault(""), Some(SegmentFault::Empty));
    assert_eq!(segment_fault(".."), Some(SegmentFault::Dot));
    assert_eq!(segment_fault("a/b"), Some(SegmentFault::Char('/')));
    assert_eq!(segment_fault("a{b}"), Some(SegmentFault::Char('{')));
    assert_eq!(
        segment_fault("no\u{a0}break"),
        Some(SegmentFault::Char('\u{a0}'))
    );
    assert_eq!(
        segment_fault("é\u{2028}"),
        Some(SegmentFault::Char('\u{2028}'))
    );
}

#[test]
fn a_mount_error_lists_each_mistake_under_its_declaration() {
    struct Users;
    let error = MountError::new(
        "/admin",
        vec![
            DeclarationError::panel(DeclarationErrorKind::MissingDb),
            DeclarationError::of::<Users>(
                Site::Relation("posts".to_string()),
                DeclarationErrorKind::UnregisteredRelation,
            ),
        ],
    );
    let users = std::any::type_name::<Users>();
    assert_eq!(
        error.to_string(),
        format!(
            "panel '/admin' cannot mount:\n  - the router holds no Db: install it with \
             `.app_context(db)` before mounting the panel\n  - `{users}` relation `posts`: \
             the related resource is not registered on this panel: declare it with \
             `Panel::resource`"
        )
    );
}
