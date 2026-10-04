use toasty_core::stmt::ValueRecord;

use super::*;

fn round_trip(value: Value) {
    let token = encode(&value).expect("encode");
    assert!(
        token.bytes().all(|b| b.is_ascii_hexdigit()),
        "token must be hex-safe, got {token}"
    );
    assert_eq!(
        decode(&token).expect("decode"),
        value,
        "round-trip {value:?}"
    );
}

#[test]
fn round_trips_scalar_variants_exactly() {
    round_trip(Value::Null);
    round_trip(Value::Bool(true));
    round_trip(Value::Bool(false));
    round_trip(Value::I8(-8));
    round_trip(Value::I16(-16));
    round_trip(Value::I32(-32));
    round_trip(Value::I64(i64::MIN));
    round_trip(Value::U8(255));
    round_trip(Value::U16(u16::MAX));
    round_trip(Value::U32(u32::MAX));
    round_trip(Value::U64(u64::MAX));
    round_trip(Value::F32(1.5));
    round_trip(Value::F64(f64::MIN));
    round_trip(Value::String("Ada Lovelace".to_string()));
    round_trip(Value::String("with spaces & symbols ?#=/".to_string()));
    round_trip(Value::Uuid(uuid::Uuid::nil()));
    round_trip(Value::Uuid(uuid::Uuid::new_v4()));
    round_trip(Value::Timestamp(
        "2024-01-15T09:30:00Z".parse().expect("timestamp"),
    ));
    round_trip(Value::Date("2024-01-15".parse().expect("date")));
    round_trip(Value::DateTime(
        "2024-01-15T09:30:00".parse().expect("datetime"),
    ));
    round_trip(Value::Time("09:30:00".parse().expect("time")));
    round_trip(Value::Zoned(
        jiff::civil::date(2024, 1, 15)
            .at(9, 30, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("zoned"),
    ));
}

#[test]
fn round_trips_record_cursor_shape() {
    // The engine's multi-column cursor: [sort value, primary key]
    let cursor = Value::Record(ValueRecord::from_vec(vec![
        Value::String("Alan Turing".to_string()),
        Value::Uuid(uuid::Uuid::new_v4()),
    ]));
    round_trip(cursor);
}

/// The wire format is a contract with tokens already in browsers' URLs, so
/// its exact bytes are pinned here instead of only round-tripped: a
/// symmetric tag swap or payload-width change round-trips cleanly and still
/// breaks every URL in flight. `VERSION` is the escape hatch — bump it for a
/// deliberate layout change and update this token with it; a token that
/// stops matching this test without a version bump is a bug.
#[test]
fn the_wire_format_is_pinned() {
    assert_eq!(VERSION, 1);

    // The engine's multi-column cursor, [sort value, primary key], sized to
    // cross the record, i64, string and uuid tags.
    let value = Value::Record(ValueRecord::from_vec(vec![
        Value::I64(42),
        Value::String("Ada Lovelace".to_string()),
        Value::Uuid(uuid::Uuid::nil()),
    ]));
    let token = encode(&value).expect("encode");
    assert_eq!(
        token,
        concat!(
            "017203000000382a00000000000000730c000000416461204c6f76656c616365",
            "7500000000000000000000000000000000",
        )
    );
    assert_eq!(decode(&token).expect("decode"), value);
}

#[test]
fn rejects_malformed_tokens() {
    assert!(decode("").is_err(), "empty token must fail");
    assert!(decode("zz").is_err(), "non-hex must fail");
    assert!(decode("abc").is_err(), "odd length must fail");
    // Valid hex, wrong version byte
    let token = hex_encode(&[9, TAG_I64]);
    assert!(decode(&token).is_err(), "wrong version must fail");
    // Truncated payload
    let full = encode(&Value::I64(42)).expect("encode");
    let truncated = hex_decode(&full).expect("hex");
    let token = hex_encode(&truncated[..truncated.len() - 2]);
    assert!(decode(&token).is_err(), "truncation must fail");
}

#[test]
fn round_trips_bytes_blobs() {
    // SQL drivers hand the primary-key back as a byte blob (e.g. a UUID
    // stored in a BLOB column) — the cursor must survive unchanged.
    round_trip(Value::Bytes(vec![1, 160, 84, 22, 172, 65, 127]));
}

#[test]
fn rejects_unsupported_variants() {
    assert!(encode(&Value::List(vec![Value::I64(1)])).is_err());
}

#[test]
fn rejects_overly_deep_record_nesting() {
    let mut value = Value::I64(1);
    for _ in 0..(MAX_CURSOR_DEPTH + 2) {
        value = Value::Record(ValueRecord::from_vec(vec![value]));
    }
    let token = encode(&value).expect("deep encode must succeed");
    assert!(
        decode(&token).is_err(),
        "decode must cap record nesting at {MAX_CURSOR_DEPTH}"
    );
}
