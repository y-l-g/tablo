//! URL-safe encoding for Toasty pagination cursors.
//!
//! Encodes pagination cursors as URL-safe hex tokens preserving the exact value variant with
//! depth-capped records.

use toasty_core::stmt::Value;
use topcoat::Result;

use crate::error::TabloError;

/// Version tag byte; bump on an incompatible layout change.
const VERSION: u8 = 1;

const TAG_NULL: u8 = b'n';
const TAG_BOOL: u8 = b'b';
const TAG_I8: u8 = b'1';
const TAG_I16: u8 = b'2';
const TAG_I32: u8 = b'4';
const TAG_I64: u8 = b'8';
const TAG_U8: u8 = b'A';
const TAG_U16: u8 = b'B';
const TAG_U32: u8 = b'C';
const TAG_U64: u8 = b'D';
const TAG_F32: u8 = b'f';
const TAG_F64: u8 = b'g';
const TAG_STRING: u8 = b's';
const TAG_UUID: u8 = b'u';
const TAG_TIMESTAMP: u8 = b't';
const TAG_DATE: u8 = b'd';
const TAG_DATETIME: u8 = b'm';
const TAG_TIME: u8 = b'i';
const TAG_BYTES: u8 = b'x';
const TAG_ZONED: u8 = b'z';
const TAG_RECORD: u8 = b'r';

/// Encodes a cursor value into a URL-safe token (`[0-9a-f]` only).
pub fn encode(value: &Value) -> Result<String> {
    let mut payload = vec![VERSION];
    write_value(value, &mut payload)?;
    Ok(hex_encode(&payload))
}

/// A malformed token carries the list retry that drops it.
fn malformed(message: impl Into<String>) -> topcoat::Error {
    TabloError::Cursor(message.into()).into()
}

/// Reports an unframeable ordering-column value without blaming the request token.
fn unencodable(message: impl Into<String>) -> topcoat::Error {
    TabloError::Declaration(message.into()).into()
}

/// Reports a cursor cut from a different `ORDER BY` with the malformed-token retry contract.
pub(crate) fn rejected(error: &topcoat::Error) -> topcoat::Error {
    TabloError::CursorRejected(error.to_string()).into()
}

/// Decodes a token produced by [`encode`] back into a cursor [`Value`].
///
/// # Errors
///
/// Fails malformed or over-deep tokens as cursor errors so the retry link drops them (#98).
pub fn decode(token: &str) -> Result<Value> {
    let payload = hex_decode(token)?;
    let mut buf = &payload[..];
    let version = take::<1>(&mut buf)?[0];
    if version != VERSION {
        return Err(malformed(format!("cursor: unsupported version {version}")));
    }
    let (value, rest) = read_value_with_depth(buf, 0)?;
    if !rest.is_empty() {
        return Err(malformed("cursor: trailing bytes after value"));
    }
    Ok(value)
}

/// Max nested-record depth accepted on decode.
const MAX_CURSOR_DEPTH: usize = 16;

macro_rules! cursor_tags {
    (
        bytes: $( $btag:ident => $bv:ident($bty:ty) ),* $(,)? ;
        text: $( $ttag:ident => $tv:ident($tty:ty) as $tlabel:literal ),* $(,)? ;
    ) => {
        fn write_tagged(value: &Value, out: &mut Vec<u8>) -> Result<bool> {
            match value {
                $(
                    Value::$bv(v) => {
                        out.push($btag);
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                )*
                $(
                    Value::$tv(v) => {
                        out.push($ttag);
                        write_len_prefixed(v.to_string().as_bytes(), out)?;
                    }
                )*
                _ => return Ok(false),
            }
            Ok(true)
        }

        fn read_tagged(tag: u8, buf: &mut &[u8]) -> Result<Option<Value>> {
            Ok(Some(match tag {
                $(
                    $btag => Value::$bv(<$bty>::from_le_bytes(
                        take::<{ std::mem::size_of::<$bty>() }>(buf)?,
                    )),
                )*
                $(
                    $ttag => {
                        let s = read_len_prefixed(buf)?;
                        let text = std::str::from_utf8(&s).map_err(|e| {
                            malformed(format!("cursor: invalid {}: {e}", $tlabel))
                        })?;
                        Value::$tv(text.parse::<$tty>().map_err(|e| {
                            malformed(format!("cursor: invalid {}: {e}", $tlabel))
                        })?)
                    }
                )*
                _ => return Ok(None),
            }))
        }
    };
}

cursor_tags! {
    bytes: TAG_I8 => I8(i8), TAG_I16 => I16(i16), TAG_I32 => I32(i32), TAG_I64 => I64(i64),
        TAG_U8 => U8(u8), TAG_U16 => U16(u16), TAG_U32 => U32(u32), TAG_U64 => U64(u64),
        TAG_F32 => F32(f32), TAG_F64 => F64(f64);
    text: TAG_TIMESTAMP => Timestamp(jiff::Timestamp) as "timestamp",
        TAG_DATE => Date(jiff::civil::Date) as "date",
        TAG_DATETIME => DateTime(jiff::civil::DateTime) as "datetime",
        TAG_TIME => Time(jiff::civil::Time) as "time",
        TAG_ZONED => Zoned(jiff::Zoned) as "zoned";
}

fn write_value(value: &Value, out: &mut Vec<u8>) -> Result<()> {
    if write_tagged(value, out)? {
        return Ok(());
    }
    match value {
        Value::Null => out.push(TAG_NULL),
        Value::Bool(b) => {
            out.push(TAG_BOOL);
            out.push(u8::from(*b));
        }
        Value::String(v) => {
            out.push(TAG_STRING);
            write_len_prefixed(v.as_bytes(), out)?;
        }
        Value::Uuid(v) => {
            out.push(TAG_UUID);
            out.extend_from_slice(v.as_bytes());
        }
        Value::Bytes(v) => {
            out.push(TAG_BYTES);
            write_len_prefixed(v, out)?;
        }
        Value::Record(record) => {
            out.push(TAG_RECORD);
            out.extend_from_slice(
                &u32::try_from(record.fields.len())
                    .map_err(|e| unencodable(format!("cursor: record too long: {e}")))?
                    .to_le_bytes(),
            );
            for field in &record.fields {
                write_value(field, out)?;
            }
        }
        other => {
            return Err(unencodable(format!("cursor: unsupported value {other:?}")));
        }
    }
    Ok(())
}

fn read_value_with_depth(buf: &[u8], depth: usize) -> Result<(Value, &[u8])> {
    if depth > MAX_CURSOR_DEPTH {
        return Err(malformed("cursor: record nesting too deep"));
    }
    let mut buf = buf;
    let tag = take::<1>(&mut buf)?[0];
    if let Some(value) = read_tagged(tag, &mut buf)? {
        return Ok((value, buf));
    }
    match tag {
        TAG_NULL => Ok((Value::Null, buf)),
        TAG_BOOL => {
            let b = take::<1>(&mut buf)?[0];
            match b {
                0 => Ok((Value::Bool(false), buf)),
                1 => Ok((Value::Bool(true), buf)),
                _ => Err(malformed("cursor: invalid bool byte")),
            }
        }
        TAG_STRING => {
            let s = read_len_prefixed(&mut buf)?;
            Ok((
                Value::String(
                    String::from_utf8(s)
                        .map_err(|e| malformed(format!("cursor: invalid utf-8 string: {e}")))?,
                ),
                buf,
            ))
        }
        TAG_UUID => {
            let bytes = take::<16>(&mut buf)?;
            Ok((
                Value::Uuid(
                    uuid::Uuid::from_slice(&bytes)
                        .map_err(|e| malformed(format!("cursor: invalid uuid: {e}")))?,
                ),
                buf,
            ))
        }
        TAG_BYTES => {
            let s = read_len_prefixed(&mut buf)?;
            Ok((Value::Bytes(s), buf))
        }
        TAG_RECORD => {
            let count = u32::from_le_bytes(take::<4>(&mut buf)?) as usize;
            let mut fields = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                let (field, rest) = read_value_with_depth(buf, depth + 1)?;
                buf = rest;
                fields.push(field);
            }
            Ok((
                Value::Record(toasty_core::stmt::ValueRecord::from_vec(fields)),
                buf,
            ))
        }
        _ => Err(malformed(format!("cursor: unknown tag byte {tag:#04x}"))),
    }
}

/// Writes a `u32` length prefix, erroring when the value exceeds it.
fn write_len_prefixed(bytes: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let len = u32::try_from(bytes.len())
        .map_err(|e| unencodable(format!("cursor: value too long: {e}")))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn read_len_prefixed(buf: &mut &[u8]) -> Result<Vec<u8>> {
    let len = u32::from_le_bytes(take::<4>(buf)?) as usize;
    Ok(take_slice(buf, len)?.to_vec())
}

/// Takes exactly `N` bytes off the front, or fails closed.
fn take<const N: usize>(buf: &mut &[u8]) -> Result<[u8; N]> {
    let Some((head, rest)) = buf.split_first_chunk::<N>() else {
        return Err(malformed("cursor: unexpected end of payload"));
    };
    *buf = rest;
    Ok(*head)
}

/// Takes `n` bytes off the front, failing closed when short.
fn take_slice<'a>(buf: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if buf.len() < n {
        return Err(malformed("cursor: unexpected end of payload"));
    }
    let (head, rest) = buf.split_at(n);
    *buf = rest;
    Ok(head)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode(token: &str) -> Result<Vec<u8>> {
    let chars: Vec<char> = token.chars().collect();
    if !chars.len().is_multiple_of(2) {
        return Err(malformed("cursor: odd-length hex token"));
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let hi = pair[0]
            .to_digit(16)
            .ok_or_else(|| malformed("cursor: invalid hex digit"))?;
        let lo = pair[1]
            .to_digit(16)
            .ok_or_else(|| malformed("cursor: invalid hex digit"))?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
