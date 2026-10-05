use std::sync::{Arc, Mutex};

use super::*;
use crate::DeclarationErrorKind;

#[test]
fn unavailable_maps_infra_failures_to_an_opaque_error() {
    // Driver text is for the logs, never the error page.
    let err = unavailable("secret driver gunk: no such table");
    let rendered = err.to_string();
    assert!(
        rendered.contains(DATABASE_UNAVAILABLE),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("gunk"),
        "driver text must not leak, got {rendered}"
    );
    assert!(TabloError::is_infrastructure(&err));
}

/// A record hook that fails at the driver is an infra failure, so
/// the write surfaces the opaque mapping — the property
/// `unavailable_maps_infra_failures_to_an_opaque_error` pins, reached
/// through the driver seam.
#[test]
fn driver_failure_maps_driver_errors_to_an_opaque_error() {
    let err = driver_failure(
        toasty::Error::from_args(format_args!("secret driver gunk: no such table")),
        DATABASE_UNAVAILABLE,
    );
    let rendered = err.to_string();
    assert!(
        rendered.contains(DATABASE_UNAVAILABLE),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("gunk"),
        "driver text must not leak, got {rendered}"
    );
    assert!(TabloError::is_infrastructure(&err));
}

/// An app-authored error is not the driver's,
/// so it keeps its own mapping — a guard's 404 stays a 404 rather than
/// becoming the opaque 500.
#[test]
fn driver_failure_keeps_an_app_error_intact() {
    let guard: topcoat::Error = topcoat::router::error::not_found().into();
    let err = driver_failure(guard, DATABASE_UNAVAILABLE);
    assert!(
        err.is::<topcoat::router::error::NotFoundError>(),
        "an app-authored error must keep its own mapping"
    );
    assert!(!TabloError::is_infrastructure(&err));
}

#[test]
fn only_the_cursor_kinds_are_cursor_errors() {
    let cursor: topcoat::Error = TabloError::Cursor("cursor: bad".to_string()).into();
    let rejected: topcoat::Error = TabloError::CursorRejected("refused".to_string()).into();
    let declaration: topcoat::Error = TabloError::Declaration("bad".to_string()).into();
    assert!(TabloError::is_cursor(&cursor));
    assert!(TabloError::is_cursor(&rejected));
    assert!(!TabloError::is_cursor(&declaration));
    assert!(!TabloError::is_cursor(&unavailable("down")));
    // The classification looks through context layers.
    assert!(TabloError::is_cursor(&cursor.context("loading the page")));
}

/// A `Subscriber` capturing the message `declaration` logs.
struct LoggedMessage(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for LoggedMessage {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        struct Capture(Vec<String>);

        impl tracing::field::Visit for Capture {
            fn record_debug(&mut self, _: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.push(format!("{value:?}"));
            }

            fn record_str(&mut self, _: &tracing::field::Field, value: &str) {
                self.0.push(value.to_string());
            }
        }

        let mut capture = Capture(Vec::new());
        event.record(&mut capture);
        self.0.lock().expect("log capture").extend(capture.0);
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

#[test]
fn declaration_logs_the_message_for_operators() {
    let logged: Arc<Mutex<Vec<String>>> = Arc::default();
    let _guard = tracing::dispatcher::set_default(&tracing::dispatcher::Dispatch::new(
        LoggedMessage(logged.clone()),
    ));
    let expected = DeclarationErrorKind::NoColumns.to_string();
    declaration(expected.clone());
    let logged = logged.lock().expect("log capture").join("\n");
    assert!(
        logged.contains(&expected),
        "the log must carry the message, got {logged}"
    );
}
