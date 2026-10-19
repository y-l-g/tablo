//! Which strings can be a literal URL segment, such as an action's `NAME`.

use std::fmt;

/// Why a path segment cannot be a literal URL segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentFault {
    /// The segment is empty.
    Empty,
    /// The segment is `.` or `..`.
    Dot,
    /// The segment holds a character a route segment cannot.
    Char(char),
}

impl fmt::Display for SegmentFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("is empty"),
            Self::Dot => f.write_str("is '.' or '..'"),
            Self::Char(c) => write!(
                f,
                "contains {c:?}: quotes, backslashes, control characters, whitespace, URL \
                 punctuation and the route pattern characters '{{', '}}', '(' and ')' are refused"
            ),
        }
    }
}

/// Why `segment` cannot be a literal URL segment, if it cannot.
///
/// A `const fn`, so an action's `NAME` is checked when the panel's code compiles.
pub(crate) const fn segment_fault(segment: &str) -> Option<SegmentFault> {
    let bytes = segment.as_bytes();
    if bytes.is_empty() {
        return Some(SegmentFault::Empty);
    }
    if matches!(bytes, b"." | b"..") {
        return Some(SegmentFault::Dot);
    }
    let mut at = 0;
    while at < bytes.len() {
        let (c, width) = decode_char(bytes, at);
        if c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '"' | '\\' | '/' | '?' | '#' | '%' | '&' | '=' | '{' | '}' | '(' | ')'
            )
        {
            return Some(SegmentFault::Char(c));
        }
        at += width;
    }
    None
}

/// The character starting at byte `at` of valid UTF-8, and its width.
const fn decode_char(bytes: &[u8], at: usize) -> (char, usize) {
    let lead = bytes[at] as u32;
    let (code, width) = if lead < 0x80 {
        (lead, 1)
    } else if lead < 0xE0 {
        ((lead & 0x1F) << 6 | tail(bytes, at + 1), 2)
    } else if lead < 0xF0 {
        (
            (lead & 0x0F) << 12 | tail(bytes, at + 1) << 6 | tail(bytes, at + 2),
            3,
        )
    } else {
        (
            (lead & 0x07) << 18
                | tail(bytes, at + 1) << 12
                | tail(bytes, at + 2) << 6
                | tail(bytes, at + 3),
            4,
        )
    };
    match char::from_u32(code) {
        Some(c) => (c, width),
        None => panic!("a str holds valid UTF-8"),
    }
}

/// The six payload bits of the continuation byte at `at`.
const fn tail(bytes: &[u8], at: usize) -> u32 {
    bytes[at] as u32 & 0x3F
}
