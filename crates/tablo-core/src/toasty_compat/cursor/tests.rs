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

/// The scalar values a sort column or a key carries.
fn scalar() -> impl proptest::strategy::Strategy<Value = Value> {
    use proptest::prelude::*;

    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i8>().prop_map(Value::I8),
        any::<i16>().prop_map(Value::I16),
        any::<i32>().prop_map(Value::I32),
        any::<i64>().prop_map(Value::I64),
        any::<u8>().prop_map(Value::U8),
        any::<u16>().prop_map(Value::U16),
        any::<u32>().prop_map(Value::U32),
        any::<u64>().prop_map(Value::U64),
        // NaN is not equal to itself; a sort value is never one.
        any::<f32>()
            .prop_filter("not NaN", |f| !f.is_nan())
            .prop_map(Value::F32),
        any::<f64>()
            .prop_filter("not NaN", |f| !f.is_nan())
            .prop_map(Value::F64),
        any::<String>().prop_map(Value::String),
        any::<u128>().prop_map(|n| Value::Uuid(uuid::Uuid::from_u128(n))),
    ]
}

proptest::proptest! {
    /// A cursor over any scalar, or the engine's record of them, decodes to exactly what was
    /// encoded, through a URL-safe token.
    #[test]
    fn any_cursor_value_round_trips(
        value in scalar(),
        record in proptest::collection::vec(scalar(), 0..4),
    ) {
        round_trip(value);
        round_trip(Value::Record(ValueRecord::from_vec(record)));
    }

    /// A tampered token is refused, never a panic: the cursor arrives in the URL.
    #[test]
    fn any_token_decodes_or_is_refused(
        token in "\\PC{0,64}",
        bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..64),
    ) {
        let _ = decode(&token);
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let _ = decode(&hex);
    }
}

/// The temporal values the engine's cursors carry, whose spellings the wire format must keep.
#[test]
fn temporal_values_round_trip() {
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
