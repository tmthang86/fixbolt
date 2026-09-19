//! The 29 field types the two dictionaries use, and what each will accept on
//! the wire.
//!
//! Twenty-three names come from `FIX44.xml`; `FIX50SP2.xml` adds ten more, six
//! of which are new variants and four of which are a second XML spelling for a
//! variant that already exists (ADR-0083 decision 1).
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

// `[measured 2026-09-08]` 22 `clippy::indexing_slicing` sites in this file on
// the day `indexing_slicing = "deny"` went into the workspace lints. Debt, not
// permission: `scripts/check-indexing-debt.sh` counts these with `--force-warn`,
// which overrides this line, and its ceiling only ever goes down. STATUS.md
// item 55.
#![allow(clippy::indexing_slicing)]

/// The FIX field separator. A value containing one cannot have come off the
/// wire as a single field, so no type accepts it.
const SOH: u8 = 0x01;

/// A FIX field's data type.
///
/// Exactly the type names the two dictionaries use — `FIX44.xml`'s 23 and
/// `FIX50SP2.xml`'s 32, which overlap in 22 — and
/// `crates/dict/tests/field_types.rs` asserts the count, so a thirtieth type
/// appearing upstream is a build failure rather than a silent `STRING`.
///
/// Four XML names map onto a variant that already existed because the
/// specification defines them as the same wire format: `XID` and `XIDREF` are
/// [`FieldType::String`], `MULTIPLESTRINGVALUE` is
/// [`FieldType::MultipleValueString`], `XMLDATA` is [`FieldType::Data`].
/// ADR-0083 decision 1 records why, and [`FieldType::from_xml`] records the
/// spellings beside each other.
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
    // The six `FIX50SP2.xml` adds. ADR-0083 decision 1.
    Language,
    TagNum,
    MultipleCharValue,
    LocalMktTime,
    TzTimeOnly,
    TzTimestamp,
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
            // `FIX50SP2.xml`'s ten. `XmlData(213)` is spelled `DATA` in
            // `FIXT11.xml` and `XMLDATA` here, which is why the pair build
            // compares *variants* and needs no exception for it (ADR-0083
            // decision 2).
            b"XID" | b"XIDREF" => Self::String,
            b"MULTIPLESTRINGVALUE" => Self::MultipleValueString,
            b"XMLDATA" => Self::Data,
            b"LANGUAGE" => Self::Language,
            b"TAGNUM" => Self::TagNum,
            b"MULTIPLECHARVALUE" => Self::MultipleCharValue,
            b"LOCALMKTTIME" => Self::LocalMktTime,
            b"TZTIMEONLY" => Self::TzTimeOnly,
            b"TZTIMESTAMP" => Self::TzTimestamp,
            // An eleventh unknown name still stops the build.
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
            Self::Language => "Language",
            Self::TagNum => "TagNum",
            Self::MultipleCharValue => "MultipleCharValue",
            Self::LocalMktTime => "LocalMktTime",
            Self::TzTimeOnly => "TzTimeOnly",
            Self::TzTimestamp => "TzTimestamp",
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
            // `[unproven]` FIX 4.4 words all three as counts, so a negative is
            // refused here. QuickFIX types them through its plain `IntConvertor`
            // and would accept one; nothing in the corpus sends a negative, so
            // no oracle settles it. See `reference/fix44-dictionary-traps.md`.
            Self::Length | Self::NumInGroup | Self::SeqNum => unsigned_int(value),
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
            Self::UtcTimestamp => utc_timestamp(value),
            // ISO 639-1, two letters. Case is not checked, for the reason
            // `Country` does not check it.
            Self::Language => value.len() == 2 && value.iter().all(u8::is_ascii_alphabetic),
            // *"int field representing a tag number. Value must be positive and
            // may not contain leading zeros."* Stricter than QuickFIX's
            // `IntConvertor`, which takes `-1` and `007`.
            Self::TagNum => unsigned_int(value) && value.first() != Some(&b'0'),
            // *"one or more space delimited single character values"* — `18=2 A
            // F`. `split` yields at least one token and an empty value was
            // refused above, so "none empty" falls out of the length check.
            Self::MultipleCharValue => value.split(|&b| b == b' ').all(|t| t.len() == 1),
            // *"Format is HH:MM:SS"* — no fraction is offered, so the shared
            // time reader is pinned to width 8.
            Self::LocalMktTime => value.len() == 8 && time(value),
            // *"HH:MM[:SS][Z | [ + | - hh[:mm]]]"*. No fraction, per the text.
            Self::TzTimeOnly => match strip_zone(value) {
                head if head.len() == 8 => time(head),
                head => hour_minute(head),
            },
            // The `UtcTimestamp` reader unchanged — ADR-0058's one rule for
            // widths stays one rule — and only the zone suffix is new.
            Self::TzTimestamp => utc_timestamp(strip_zone(value)),
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
    // `HH:MM:SS`, or a `.` and one to twelve fractional digits — **the same
    // rule `session::clock::parse_utc` applies, and the two readers agreeing is
    // the point.** A timestamp reaches here as `&value[9..]`, so 8 bytes is a
    // bare `HH:MM:SS` and 21 is twelve fractional digits.
    //
    // `[measured 2026-09-09]` they did not agree, and only a real counterparty
    // could see it. Half A of `timestamp-micros` widened `parse_utc` and left
    // this alone, so `scripts/interop.sh` at `TimestampPrecision=6` logged on
    // and then answered every message after the Logon with
    // `35=3 ... 371=52 373=6` — a `Reject` per Heartbeat, per SequenceReset,
    // per Logout. `crates/dict/tests/field_types.rs::a_microsecond_timestamp_is_a_timestamp`.
    //
    // **ADR-0058 replaced both four-entry tables with a rule** so that a width
    // cannot be added to one reader and forgotten in the other again;
    // `crates/session/tests/timestamp_widths.rs::both_readers_agree_on_every_width`
    // asks both the same question at every length from 0 to 34. `9 == 8 + 1` is
    // a `.` with no digits and is refused by both — ADR-0058 decision 2.
    if !(v.len() == 8 || (10..=21).contains(&v.len())) {
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

/// `YYYYMMDD-HH:MM:SS` with the optional fraction ADR-0058's rule allows.
///
/// One reader, called from two arms: [`FieldType::UtcTimestamp`] and — after
/// its zone suffix has been taken off — [`FieldType::TzTimestamp`]. ADR-0083
/// decision 1 is explicit that the date-time part of a `TZTIMESTAMP` is this
/// reader **unchanged**, so a width added here reaches both.
fn utc_timestamp(v: &[u8]) -> bool {
    match v.split_at_checked(8) {
        Some((d, [b'-', t @ ..])) => date(d) && time(t),
        _ => false,
    }
}

/// Removes a trailing FIX time-zone suffix: `Z`, `+hh`, `-hh`, `+hh:mm` or
/// `-hh:mm`.
///
/// A value with no suffix, or with one that is not well formed, comes back
/// unchanged — and is then refused by the time reader it is handed to, which is
/// where the `373=6` is decided. Anchored on the end and on a fixed width
/// rather than scanning for a sign: `20260919-06:15:27` ends in a `-` eight
/// bytes in, and a scan would take `06:15:27` for a zone.
fn strip_zone(v: &[u8]) -> &[u8] {
    if let Some(head) = v.strip_suffix(b"Z") {
        return head;
    }
    if let Some((head, [sign, h1, h2, b':', m1, m2])) = split_tail(v, 6)
        && (*sign == b'+' || *sign == b'-')
        && zone_hour(*h1, *h2)
        && two_digit(*m1, *m2).is_some_and(|m| m <= 59)
    {
        return head;
    }
    if let Some((head, [sign, h1, h2])) = split_tail(v, 3)
        && (*sign == b'+' || *sign == b'-')
        && zone_hour(*h1, *h2)
    {
        return head;
    }
    v
}

/// `v` split so the second half is exactly `n` bytes, or `None` if it is
/// shorter than that.
fn split_tail(v: &[u8], n: usize) -> Option<(&[u8], &[u8])> {
    v.len().checked_sub(n).and_then(|at| v.split_at_checked(at))
}

/// Two ASCII digits as a number.
fn two_digit(a: u8, b: u8) -> Option<u32> {
    (a.is_ascii_digit() && b.is_ascii_digit())
        .then(|| u32::from(a - b'0') * 10 + u32::from(b - b'0'))
}

/// The `hh` of a zone offset. **01 to 12**, which is the specification's text
/// for `TZTimeOnly` and `TZTimestamp` — not the 00..=14 the world uses.
/// ADR-0083 decision 1 says exactly this range, so it is written exactly this
/// range and the ADR carries the cost.
fn zone_hour(a: u8, b: u8) -> bool {
    two_digit(a, b).is_some_and(|h| (1..=12).contains(&h))
}

/// `HH:MM`, the short form `TZTimeOnly` allows and no other type does.
fn hour_minute(v: &[u8]) -> bool {
    matches!(
        v,
        [h1, h2, b':', m1, m2]
            if two_digit(*h1, *h2).is_some_and(|h| h <= 23)
                && two_digit(*m1, *m2).is_some_and(|m| m <= 59)
    )
}
