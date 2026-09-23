//! `fixbolt_engine::redact`, one trap per test.
//!
//! The traps are the plan's *Bẫy đã lường trước* table
//! (`docs/plans/2026-09-23-p3-redact-secrets.md`); the end-to-end gate is
//! `secrets_stay_off_disk.rs`. ADR-0110 decisions 1, 2 and 5.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Not a library crate's source: `scripts/check-indexing-debt.sh` counts nothing
// outside `crates/*/src`.
#![allow(clippy::indexing_slicing)]

use fixbolt_engine::redact::{Kind, MASK, MASKED, Scope, carries_secret, mask};

/// `|` → SOH, so a fixture reads like the corpus.
fn wire(s: &str) -> Vec<u8> {
    s.bytes()
        .map(|b| if b == b'|' { 0x01 } else { b })
        .collect()
}

/// SOH → `|`, for a readable failure.
fn show(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace('\u{1}', "|")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

fn masked(s: &str) -> (Vec<u8>, usize) {
    let mut b = wire(s);
    let n = mask(&mut b);
    (b, n)
}

const LOGON: &str = "8=FIX.4.4|9=120|35=A|34=1|49=TW44|52=20260923-10:32:07.000|56=ISLD|\
                     95=14|96=raw1|raw2-tail|98=0|108=30|553=alice|554=hunter2|10=000|";

#[test]
fn a_password_is_masked_and_the_username_is_not() {
    let (b, n) = masked(LOGON);
    assert!(n >= 1, "at least the password was found");
    assert!(
        contains(&b, &wire("|554=*******|")),
        "every value byte of 554 is masked: {}",
        show(&b)
    );
    assert!(
        contains(&b, &wire("|553=alice|")),
        "553 Username is identity, not a secret: {}",
        show(&b)
    );
    assert!(!contains(&b, b"hunter2"), "{}", show(&b));
}

#[test]
fn masking_keeps_the_length() {
    let before = wire(LOGON);
    let (after, _) = masked(LOGON);
    assert_eq!(after.len(), before.len(), "the length is kept");
    // And nothing outside a secret moved: every differing byte is now `*`.
    for (a, b) in before.iter().zip(after.iter()) {
        assert!(a == b || *b == MASK, "{} → {}", show(&before), show(&after));
    }
    assert!(
        contains(&after, &wire("|9=120|")) && contains(&after, &wire("|95=14|")),
        "9= and the LENGTH field are left exactly as received: {}",
        show(&after)
    );
    assert!(
        contains(&after, &wire("|10=000|")),
        "10= is left exactly as received: {}",
        show(&after)
    );
}

#[test]
fn a_raw_data_holding_an_soh_is_masked_whole() {
    // `96=` is 14 bytes: `raw1|raw2-tail`, with an SOH inside.
    let (b, _) = masked(LOGON);
    assert!(
        contains(&b, &wire("|95=14|96=**************|98=0|")),
        "all 14 declared bytes, the SOH among them, are masked: {}",
        show(&b)
    );
    assert!(
        !contains(&b, b"raw2-tail"),
        "the half after the SOH must not survive: {}",
        show(&b)
    );
}

#[test]
fn raw_data_on_news_is_left_alone() {
    let (b, n) = masked("8=FIX.4.4|9=40|35=B|148=Headline|95=5|96=hello|10=000|");
    assert_eq!(n, 0, "nothing on a News message is a secret: {}", show(&b));
    assert!(
        contains(&b, &wire("|96=hello|")),
        "RawData on News is content, not a secret: {}",
        show(&b)
    );
    assert!(!carries_secret(&wire(
        "8=FIX.4.4|9=40|35=B|148=Headline|95=5|96=hello|10=000|"
    )));
}

#[test]
fn raw_data_on_a_user_request_is_masked() {
    let (b, _) = masked("8=FIX.4.4|9=40|35=BE|95=4|96=abcd|923=R|924=1|553=u|10=000|");
    assert!(contains(&b, &wire("|96=****|")), "{}", show(&b));
}

#[test]
fn raw_data_on_a_frame_with_no_msg_type_is_masked() {
    // A garbage frame is not known to be anything else, so it is treated as a
    // sign-on. Over-masking is allowed; under-masking is not.
    let (b, n) = masked("8=FIX.4.4|9=20|34=1|95=6|96=secret|10=000|");
    assert_eq!(n, 1, "{}", show(&b));
    assert!(contains(&b, &wire("|96=******|")), "{}", show(&b));
}

#[test]
fn only_the_whole_tag_554_is_masked() {
    let (b, n) = masked("8=FIX.4.4|9=30|35=D|1554=keep1|5540=keep2|55=keep3|554=gone|10=000|");
    assert_eq!(n, 1, "only 554 itself: {}", show(&b));
    for kept in ["|1554=keep1|", "|5540=keep2|", "|55=keep3|"] {
        assert!(contains(&b, &wire(kept)), "{kept} survives: {}", show(&b));
    }
    assert!(contains(&b, &wire("|554=****|")), "{}", show(&b));
}

#[test]
fn a_new_password_and_the_encrypted_ones_are_masked_everywhere() {
    let (b, n) = masked("8=FIX.4.4|9=60|35=D|925=newpw|1401=3|1402=a|b|1403=2|1404=zz|10=000|");
    assert_eq!(n, 3, "{}", show(&b));
    assert!(contains(&b, &wire("|925=*****|")), "{}", show(&b));
    assert!(
        contains(&b, &wire("|1401=3|1402=***|1403=2|1404=**|")),
        "each DATA secret over its own declared length: {}",
        show(&b)
    );
}

#[test]
fn an_empty_or_unterminated_password_is_handled() {
    let (b, n) = masked("8=FIX.4.4|35=A|554=|");
    assert_eq!(
        n,
        1,
        "an empty password is still a secret field: {}",
        show(&b)
    );
    assert_eq!(b, wire("8=FIX.4.4|35=A|554=|"), "and nothing else moves");

    let (b, n) = masked("8=FIX.4.4|35=A|554=abc");
    assert_eq!(n, 1, "{}", show(&b));
    assert_eq!(
        b,
        wire("8=FIX.4.4|35=A|554=***"),
        "a password with no closing SOH is masked to the end of the buffer"
    );

    let (b, n) = masked("554=abc|");
    assert_eq!(n, 1, "a field at offset 0 is a field: {}", show(&b));
    assert_eq!(b, wire("554=***|"));
}

#[test]
fn a_declared_length_past_the_end_is_clamped_not_a_panic() {
    let (b, n) = masked("8=FIX.4.4|35=A|95=99999999999999999999999|96=abc|554=pw|");
    assert_eq!(
        n,
        1,
        "the lying length swallowed the rest as one field: {}",
        show(&b)
    );
    assert_eq!(
        b,
        wire("8=FIX.4.4|35=A|95=99999999999999999999999|96=***********"),
        "everything after `96=` is masked, the SOHs included, and nothing panics"
    );
    assert!(!contains(&b, b"pw"));
}

#[test]
fn a_declared_length_that_lands_mid_field_masks_to_the_next_boundary() {
    // 95=5 over `ab|55` would stop inside `554=pw`; the mask runs on to the next
    // SOH so the scan never resumes inside a field and reads `4=pw` as tag 4.
    let (b, _) = masked("8=FIX.4.4|35=A|95=5|96=ab|554=pw|58=x|");
    assert!(!contains(&b, b"pw"), "{}", show(&b));
    assert!(contains(&b, &wire("|58=x|")), "{}", show(&b));
}

#[test]
fn a_declared_length_shorter_than_the_value_still_masks_to_the_soh() {
    let (b, _) = masked("8=FIX.4.4|35=A|95=2|96=abcdef|98=0|");
    assert!(
        contains(&b, &wire("|96=******|98=0|")),
        "the larger of the declared length and the run to the SOH: {}",
        show(&b)
    );
}

#[test]
fn every_prefix_of_a_logon_is_scanned_without_a_panic() {
    let whole = wire(LOGON);
    for end in 0..=whole.len() {
        let mut b = whole[..end].to_vec();
        let n = mask(&mut b);
        assert_eq!(b.len(), end, "the length is kept at prefix {end}");
        assert!(n <= 2, "at most 96 and 554 at prefix {end}");
        let _ = carries_secret(&whole[..end]);
    }
    // And every suffix, which starts mid-field.
    for start in 0..=whole.len() {
        let mut b = whole[start..].to_vec();
        let _ = mask(&mut b);
        let _ = carries_secret(&whole[start..]);
    }
}

#[test]
fn carries_secret_agrees_with_mask() {
    for s in [
        LOGON,
        "8=FIX.4.4|35=D|55=X|10=000|",
        "8=FIX.4.4|35=B|95=3|96=abc|10=000|",
        "8=FIX.4.4|35=D|925=x|10=000|",
        "",
    ] {
        let (_, n) = masked(s);
        assert_eq!(carries_secret(&wire(s)), n > 0, "{s}");
    }
}

/// **The LENGTH→DATA pairs, pinned against the generated FIX 4.4 table.** A
/// dictionary change that moves a pair is a red test, never a silent leak.
#[test]
fn the_length_pairs_match_the_dictionary() {
    for s in MASKED {
        match s.kind {
            Kind::String => assert_eq!(
                fixbolt_dict::field_type(s.tag),
                Some(fixbolt_dict::FieldType::String),
                "{} is a STRING in FIX 4.4",
                s.tag
            ),
            Kind::Data { length } => {
                if fixbolt_dict::is_defined_tag(s.tag) {
                    assert_eq!(
                        fixbolt_dict::field_type(s.tag),
                        Some(fixbolt_dict::FieldType::Data),
                        "{} is DATA in FIX 4.4",
                        s.tag
                    );
                    assert_eq!(
                        fixbolt_dict::data_length_tag(s.tag),
                        Some(length),
                        "{}'s length is declared by {length} in FIX 4.4",
                        s.tag
                    );
                } else {
                    // 1402 and 1404 arrived with FIX 5.0; the `fix50sp2`
                    // test below pins them. Asserted, so FIX 4.4 growing them
                    // is noticed here.
                    assert!(
                        [1402, 1404].contains(&s.tag),
                        "{} is not in FIX 4.4 and is not one of the two that \
                         are expected to be absent",
                        s.tag
                    );
                }
            }
        }
    }
    assert_eq!(fixbolt_dict::data_length_tag(96), Some(95));
    assert!(
        MASKED
            .iter()
            .any(|s| s.tag == 96 && s.scope == Scope::SignOn),
        "96 is masked on sign-on messages only"
    );
}

/// The same pin, against the FIXT 1.1 / FIX 5.0 SP2 table, where `1402` and
/// `1404` exist.
#[cfg(feature = "fix50sp2")]
#[test]
fn the_length_pairs_match_the_fix50sp2_dictionary() {
    use fixbolt_dict::fixt11_fix50sp2 as d;
    for s in MASKED {
        match s.kind {
            Kind::String => assert_eq!(
                d::field_type(s.tag),
                Some(fixbolt_dict::FieldType::String),
                "{} is a STRING in FIX 5.0 SP2",
                s.tag
            ),
            Kind::Data { length } => {
                assert_eq!(
                    d::field_type(s.tag),
                    Some(fixbolt_dict::FieldType::Data),
                    "{} is DATA in FIX 5.0 SP2",
                    s.tag
                );
                assert_eq!(
                    d::data_length_tag(s.tag),
                    Some(length),
                    "{}'s length is declared by {length} in FIX 5.0 SP2",
                    s.tag
                );
            }
        }
    }
    assert_eq!(d::data_length_tag(1402), Some(1401));
    assert_eq!(d::data_length_tag(1404), Some(1403));
}
