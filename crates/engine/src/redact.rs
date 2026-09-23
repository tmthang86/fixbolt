//! What must never reach the disk, and how it is found in a message.
//!
//! [ADR-0110](../../../docs/decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md):
//! the message log masks a secret's value bytes on its writer thread, and the
//! journal file never holds a message that carries one. Both ask this module,
//! and [`MASKED`] is the one place the list lives.
//!
//! # How a secret is found
//!
//! **Without a dictionary, and it errs towards masking more.** The scanner
//! splits on SOH. A STRING value cannot hold an SOH, so every real `554=` or
//! `925=` begins right after one and is found. Reading bytes *inside* some
//! other DATA field as a field of their own only masks bytes that were not a
//! secret.
//!
//! A DATA secret is masked over **the larger of** its declared length (the most
//! recent length field for it in the message) and the bytes up to the next SOH,
//! then on to the SOH after that, clamped to the buffer. So a `96=` holding an
//! SOH is masked whole, a lying length masks more rather than less, and the
//! scan always resumes at a field boundary. `96` counts as a secret when **any**
//! `35=` in the record is `A` or `BE`, or when there is none.
//!
//! # What is not promised
//!
//! **Never under-masks — for a well-formed length pair.** A DATA secret whose
//! length field is missing, or comes *after* it (`1402` without `1401`, `96`
//! before `95`), has no declared length to go on and is masked to the next SOH
//! only; if its value holds an SOH, the rest is left as it arrived. That frame
//! is malformed (D3: the length field immediately precedes its DATA), and the
//! gap is recorded in plan *Sửa 2*. What proves the rest: `tests/redact.rs`
//! `a_raw_data_holding_an_soh_is_masked_whole`,
//! `a_declared_length_past_the_end_is_clamped_not_a_panic`,
//! `a_declared_length_that_lands_mid_field_masks_to_the_next_boundary`,
//! `raw_data_of_a_logon_behind_another_msg_type_is_masked`,
//! `a_later_msg_type_does_not_unmask_raw_data`; and
//! `secrets_stay_off_disk.rs`
//! `raw_data_behind_a_non_sign_on_msg_type_in_a_garbage_cut_is_masked_in_the_log`.
//!
//! # What it costs
//!
//! Two pure functions over a borrowed slice: no `Vec`, no `String`, no
//! `format!`, no indexing, no `unwrap`. `benches/alloc.rs` cases `redact-mask`
//! and `redact-scan` read **0** and each asserts its path ran. How long a scan
//! takes is `[unmeasured]` (ADR-0110).

/// The byte every masked value byte becomes. `0x2A`.
pub const MASK: u8 = b'*';

/// The field separator.
const SOH: u8 = 0x01;

/// How a secret's value is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Ends at the next SOH, which it cannot contain.
    String,
    /// May contain SOH; its length is declared by the field `length`.
    Data {
        /// The LENGTH field that declares it, e.g. `95` for `96`.
        length: u32,
    },
}

/// Which messages a secret is masked in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every message.
    Every,
    /// Only in a record holding a `Logon` (`35=A`) or a `UserRequest`
    /// (`35=BE`) **anywhere** — or no `35=` at all, because a garbage frame is
    /// not known to be anything else. Any, not the first: one garbage record
    /// can hold several frames (plan *Sửa 2*).
    SignOn,
}

/// One field that never reaches the disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Secret {
    /// The tag.
    pub tag: u32,
    /// How its value is delimited.
    pub kind: Kind,
    /// Where it counts as a secret.
    pub scope: Scope,
}

/// **The list, in one place.** ADR-0110 decision 1.
///
/// Not here, deliberately: `553` Username (a dispute needs to know who), the
/// LENGTH fields (a length is not a secret), `1400` EncryptPasswordMethod, `91`
/// SecureData, and `96` on anything but a sign-on. The LENGTH→DATA pairs are
/// pinned against `fixbolt_dict`'s tables by
/// `tests/redact.rs::the_length_pairs_match_the_dictionary`.
pub const MASKED: [Secret; 5] = [
    Secret {
        tag: 554,
        kind: Kind::String,
        scope: Scope::Every,
    },
    Secret {
        tag: 925,
        kind: Kind::String,
        scope: Scope::Every,
    },
    Secret {
        tag: 1402,
        kind: Kind::Data { length: 1401 },
        scope: Scope::Every,
    },
    Secret {
        tag: 1404,
        kind: Kind::Data { length: 1403 },
        scope: Scope::Every,
    },
    Secret {
        tag: 96,
        kind: Kind::Data { length: 95 },
        scope: Scope::SignOn,
    },
];

/// Overwrite every value byte of every secret field in `bytes` with [`MASK`],
/// **keeping the length**. Returns how many secret fields were found (an empty
/// value counts).
///
/// `9=`, the LENGTH fields and `10=` are untouched, so the message still frames
/// and deliberately no longer checksums.
pub fn mask(bytes: &mut [u8]) -> usize {
    let mut scan = Scan::new(bytes);
    let mut masked = 0usize;
    while let Some((from, to)) = scan.next(bytes) {
        if let Some(value) = bytes.get_mut(from..to) {
            value.fill(MASK);
        }
        masked = masked.saturating_add(1);
    }
    masked
}

/// Whether `bytes` holds any secret field at all, by the same rules as
/// [`mask`].
#[must_use]
pub fn carries_secret(bytes: &[u8]) -> bool {
    Scan::new(bytes).next(bytes).is_some()
}

/// The scan's position and what it has learned so far.
///
/// Holds no borrow, so [`mask`] can write between two calls to [`Scan::next`].
/// Writing never moves a field boundary the scan has yet to reach: every range
/// it hands out ends at an SOH or at the end of the buffer, and the next call
/// starts past it.
struct Scan {
    /// Where the next field starts.
    at: usize,
    /// Whether [`Scope::SignOn`] secrets count in this message.
    sign_on: bool,
    /// The most recent declared length for each [`MASKED`] entry, parallel to
    /// it. Zero for a STRING, or when none was declared.
    declared: [usize; MASKED.len()],
}

impl Scan {
    fn new(bytes: &[u8]) -> Self {
        let sign_on = sign_on(bytes);
        Self {
            at: 0,
            sign_on,
            declared: [0; MASKED.len()],
        }
    }

    /// The value range `[from, to)` of the next secret field, or `None` when
    /// the buffer is exhausted.
    fn next(&mut self, bytes: &[u8]) -> Option<(usize, usize)> {
        loop {
            let rest = bytes.get(self.at..)?;
            if rest.is_empty() {
                return None;
            }
            let start = self.at;
            let field_end = next_soh(bytes, start);
            self.at = field_end.saturating_add(1);
            let Some(field) = bytes.get(start..field_end) else {
                continue;
            };
            let Some(eq) = field.iter().position(|b| *b == b'=') else {
                continue;
            };
            let Some(tag) = field.get(..eq).and_then(parse_tag) else {
                continue;
            };
            let value = field.get(eq.saturating_add(1)..).unwrap_or(&[]);
            let from = start.saturating_add(eq).saturating_add(1);

            // A LENGTH field: remember what it declares, for the DATA after it.
            for (s, d) in MASKED.iter().zip(self.declared.iter_mut()) {
                if s.kind == (Kind::Data { length: tag }) {
                    *d = parse_len(value);
                }
            }

            let Some((secret, declared)) = MASKED
                .iter()
                .zip(self.declared.iter())
                .find(|(s, _)| s.tag == tag)
            else {
                continue;
            };
            if secret.scope == Scope::SignOn && !self.sign_on {
                continue;
            }
            let to = match secret.kind {
                Kind::String => field_end,
                Kind::Data { .. } => {
                    let by_len = from.saturating_add(*declared).min(bytes.len());
                    if by_len > field_end {
                        // The declared length reaches past an SOH: take it all,
                        // and on to the next boundary so the scan resumes at the
                        // start of a field rather than in the middle of one.
                        next_soh(bytes, by_len)
                    } else {
                        field_end
                    }
                }
            };
            self.at = to.saturating_add(1);
            return Some((from, to));
        }
    }
}

/// The index of the first SOH at or after `from`, or `bytes.len()`.
fn next_soh(bytes: &[u8], from: usize) -> usize {
    bytes
        .get(from..)
        .and_then(|r| r.iter().position(|b| *b == SOH))
        .map_or(bytes.len(), |p| from.saturating_add(p))
}

/// Whether [`Scope::SignOn`] secrets count in `bytes`: **any** `35=A` or
/// `35=BE` in it, or no `35=` at all.
///
/// Any, not the first: a `Cut::Garbage` record can hold several frames, and a
/// Heartbeat's `35=0` in front of a `UserRequest` must not exempt the RawData
/// behind it (plan *Sửa 2*, senior review of PR #98 finding 1). A later `35=`
/// cannot un-mask either. Split on SOH like everything else here, so a `35=A`
/// inside some other DATA value masks more — over-masking, allowed.
fn sign_on(bytes: &[u8]) -> bool {
    let mut typed = false;
    for f in bytes.split(|b| *b == SOH) {
        if let Some(t) = f.strip_prefix(b"35=") {
            if t == b"A" || t == b"BE" {
                return true;
            }
            typed = true;
        }
    }
    !typed
}

/// A tag, all ASCII digits. `None` for anything else, or one that overflows.
fn parse_tag(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |n, b| {
        if b.is_ascii_digit() {
            n.checked_mul(10)?.checked_add(u32::from(b - b'0'))
        } else {
            None
        }
    })
}

/// A declared length. Stops at the first non-digit and saturates, because it
/// arrived from the network: a huge one only masks more, clamped to the buffer.
fn parse_len(digits: &[u8]) -> usize {
    digits
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0usize, |n, b| {
            n.saturating_mul(10).saturating_add(usize::from(b - b'0'))
        })
}
