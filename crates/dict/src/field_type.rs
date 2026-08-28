//! The 23 FIX 4.4 field types, and what each will accept on the wire.
//!
//! Hand-written rather than generated: the *set* of types comes from the XML
//! and is asserted against it, but what "a QTY" looks like is not in the XML at
//! all. Putting the check here rather than in `session` keeps it in one place —
//! `CLAUDE.md` §4, one rule, one place.
//!
//! # What this is not
//!
//! It is a **format** check, answering `SessionRejectReason 6`, *Incorrect data
//! format for value*. It is not a range check and not a business rule: `38=0`
//! is a well-formed QTY and whether an order for nothing makes sense is not the
//! dictionary's question.

/// The FIX field separator. A value containing one cannot have come off the
/// wire as a single field, so no type accepts it.
const SOH: u8 = 0x01;

/// A FIX 4.4 field's data type.
///
/// Exactly the 23 the XML uses — `crates/dict/tests/generated.rs` asserts that,
/// so a 24th type appearing upstream is a build failure rather than a silent
/// `STRING`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    Int,
    Length,
    SeqNum,
    NumInGroup,
    Float,
    Qty,
    Price,
    PriceOffset,
    Amt,
    Percentage,
    Char,
    Boolean,
    String,
    MultipleValueString,
    Currency,
    Country,
    Exchange,
    MonthYear,
    LocalMktDate,
    UtcDateOnly,
    UtcTimeOnly,
    UtcTimestamp,
    Data,
}

impl FieldType {
    /// The XML spelling, so `build.rs` and this enum cannot drift apart.
    #[must_use]
    pub const fn from_xml(name: &str) -> Option<Self> {
        // `match` on `&str` in a `const fn` needs bytes.
        Some(match name.as_bytes() {
            b"INT" => Self::Int,
            b"LENGTH" => Self::Length,
            b"SEQNUM" => Self::SeqNum,
            b"NUMINGROUP" => Self::NumInGroup,
            b"FLOAT" => Self::Float,
            b"QTY" => Self::Qty,
            b"PRICE" => Self::Price,
            b"PRICEOFFSET" => Self::PriceOffset,
            b"AMT" => Self::Amt,
            b"PERCENTAGE" => Self::Percentage,
            b"CHAR" => Self::Char,
            b"BOOLEAN" => Self::Boolean,
            b"STRING" => Self::String,
            b"MULTIPLEVALUESTRING" => Self::MultipleValueString,
            b"CURRENCY" => Self::Currency,
            b"EXCHANGE" => Self::Exchange,
            b"COUNTRY" => Self::Country,
            b"MONTHYEAR" => Self::MonthYear,
            b"LOCALMKTDATE" => Self::LocalMktDate,
            b"UTCDATEONLY" => Self::UtcDateOnly,
            b"UTCTIMEONLY" => Self::UtcTimeOnly,
            b"UTCTIMESTAMP" => Self::UtcTimestamp,
            b"DATA" => Self::Data,
            _ => return None,
        })
    }

    /// The identifier `build.rs` writes into the generated table.
    #[must_use]
    pub const fn as_rust(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Length => "Length",
            Self::SeqNum => "SeqNum",
            Self::NumInGroup => "NumInGroup",
            Self::Float => "Float",
            Self::Qty => "Qty",
            Self::Price => "Price",
            Self::PriceOffset => "PriceOffset",
            Self::Amt => "Amt",
            Self::Percentage => "Percentage",
            Self::Char => "Char",
            Self::Boolean => "Boolean",
            Self::String => "String",
            Self::MultipleValueString => "MultipleValueString",
            Self::Currency => "Currency",
            Self::Country => "Country",
            Self::Exchange => "Exchange",
            Self::MonthYear => "MonthYear",
            Self::LocalMktDate => "LocalMktDate",
            Self::UtcDateOnly => "UtcDateOnly",
            Self::UtcTimeOnly => "UtcTimeOnly",
            Self::UtcTimestamp => "UtcTimestamp",
            Self::Data => "Data",
        }
    }

    /// Whether `value` is a well-formed value of this type.
    ///
    /// No allocation and no `format!` — this runs once per field of a message
    /// under validation. Proven by `crates/codec/benches/alloc.rs`.
    #[must_use]
    pub fn accepts(self, value: &[u8]) -> bool {
        // DATA is delimited by its length field, so any bytes are legal —
        // including none, and including `0x01`. Every other type is refused an
        // empty value here; the session reports that as `373=4`, a different
        // code, before it ever asks about format.
        if self == Self::Data {
            return true;
        }
        if value.is_empty() {
            return false;
        }
        match self {
            Self::Data => true,
            // INT carries a sign but never a point. `371=-1` is a real INT —
            // `14a_BadField.def` sends `-1=HI` and the Reject echoes the tag
            // back in `371`.
            Self::Int => signed_int(value),
            Self::Float
            | Self::Qty
            | Self::Price
            | Self::PriceOffset
            | Self::Amt
            | Self::Percentage => signed_number(value),
            Self::Length | Self::NumInGroup => unsigned_int(value),
            // A sequence number counts from 1; `11c_NewSeqNoLess.def` sends
            // `34=0` and the corpus rejects it.
            Self::SeqNum => unsigned_int(value) && value.iter().any(|&b| b != b'0'),
            Self::Char => value.len() == 1,
            Self::Boolean => value == b"Y" || value == b"N",
            // A value may hold anything but the separator, which cannot reach
            // here anyway — the parser splits on it.
            Self::String | Self::MultipleValueString | Self::Exchange => !value.contains(&SOH),
            Self::Currency => value.len() == 3 && value.iter().all(u8::is_ascii_alphabetic),
            Self::Country => value.len() == 2 && value.iter().all(u8::is_ascii_alphabetic),
            Self::MonthYear => month_year(value),
            Self::LocalMktDate | Self::UtcDateOnly => date(value),
            Self::UtcTimeOnly => time(value),
            Self::UtcTimestamp => {
                value.len() > 9 && value[8] == b'-' && date(&value[..8]) && time(&value[9..])
            }
        }
    }
}

/// Digits, at least one. No sign, no point.
fn unsigned_int(v: &[u8]) -> bool {
    !v.is_empty() && v.iter().all(u8::is_ascii_digit)
}

/// An optional leading `-`, then digits. **No point** — that is the whole
/// difference between INT and FLOAT, and grouping them cost a red test.
fn signed_int(v: &[u8]) -> bool {
    unsigned_int(v.strip_prefix(b"-").unwrap_or(v))
}

/// An optional leading `-`, then digits, then at most one `.` and more digits.
///
/// **A leading `+` is refused**, and that is not a detail:
/// `14f_IncorrectDataFormat.def` sends `38=+200.00` and expects `373=6`. FIX
/// floats carry a minus or nothing.
fn signed_number(v: &[u8]) -> bool {
    let v = v.strip_prefix(b"-").unwrap_or(v);
    if v.is_empty() {
        return false;
    }
    let mut parts = v.splitn(2, |&b| b == b'.');
    let (Some(whole), tail) = (parts.next(), parts.next()) else {
        return false;
    };
    // `.5` and `5.` are both written by real engines.
    if whole.is_empty() && tail.is_none_or(<[u8]>::is_empty) {
        return false;
    }
    whole.iter().all(u8::is_ascii_digit) && tail.is_none_or(|t| t.iter().all(u8::is_ascii_digit))
}

/// `YYYYMM`, and the month is a month.
fn month_year(v: &[u8]) -> bool {
    // FIX 4.4 also allows `YYYYMMDD` and a `YYYYMMww` week form.
    if !(v.len() == 6 || v.len() == 8) || !v[..6].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let m = u32::from(v[4] - b'0') * 10 + u32::from(v[5] - b'0');
    if !(1..=12).contains(&m) {
        return false;
    }
    v.len() == 6 || v[6..].iter().all(u8::is_ascii_digit) || (v[6] == b'w' && v[7].is_ascii_digit())
}

/// `YYYYMMDD`.
fn date(v: &[u8]) -> bool {
    if v.len() != 8 || !v.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let m = u32::from(v[4] - b'0') * 10 + u32::from(v[5] - b'0');
    let d = u32::from(v[6] - b'0') * 10 + u32::from(v[7] - b'0');
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// `HH:MM:SS` or `HH:MM:SS.sss`.
fn time(v: &[u8]) -> bool {
    if v.len() != 8 && v.len() != 12 {
        return false;
    }
    if v[2] != b':' || v[5] != b':' {
        return false;
    }
    let two = |at: usize| u32::from(v[at] - b'0') * 10 + u32::from(v[at + 1] - b'0');
    if !(v[..2].iter().all(u8::is_ascii_digit)
        && v[3..5].iter().all(u8::is_ascii_digit)
        && v[6..8].iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    // 60 is a leap second, the same allowance `session::clock` makes.
    if two(0) > 23 || two(3) > 59 || two(6) > 60 {
        return false;
    }
    v.len() == 8 || (v[8] == b'.' && v[9..].iter().all(u8::is_ascii_digit))
}
