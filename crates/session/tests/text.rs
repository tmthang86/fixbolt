//! The `58=` texts, checked against the corpus rather than believed.
//!
//! `Comparator.rb` compares values byte for byte, so these strings are part of
//! the gate: an engine that says "Required tag is missing" instead of "Required
//! tag missing" fails a test about sequence numbers and the message will not say
//! why.
//!
//! The table lives in `src/text.rs` and this file re-derives it from the `.def`
//! files at run time. Hard-coding it twice would prove nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use nanofix_conformance::script::{Kind, Step, scenarios};
use nanofix_session::text::SessionText;

/// Every `(58=text)` in the corpus, with its `373=` code and `35=` type.
fn corpus() -> BTreeMap<String, (Option<u32>, String, usize)> {
    let mut out: BTreeMap<String, (Option<u32>, String, usize)> = BTreeMap::new();
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        for step in s.steps {
            if !matches!(step.kind, Kind::Expect(_)) {
                continue;
            }
            let Some(m) = Step::message(&step) else {
                continue;
            };
            let mut f: BTreeMap<&str, &str> = BTreeMap::new();
            for raw in m.wire.split(|&b| b == 0x01).filter(|x| !x.is_empty()) {
                let Ok(s) = core::str::from_utf8(raw) else {
                    continue;
                };
                if let Some((k, v)) = s.split_once('=') {
                    f.entry(k).or_insert(v);
                }
            }
            let Some(text) = f.get("58") else { continue };
            let entry = out.entry((*text).to_string()).or_insert((
                f.get("373").and_then(|c| c.parse().ok()),
                (*f.get("35").unwrap_or(&"?")).to_string(),
                0,
            ));
            entry.2 += 1;
        }
    }
    out
}

#[test]
fn the_corpus_has_the_seventeen_texts_the_table_claims() {
    let c = corpus();
    assert_eq!(c.len(), 17, "distinct 58= texts on E lines");
    assert_eq!(
        c.values().map(|v| v.2).sum::<usize>(),
        44,
        "58= fields in total"
    );
    assert_eq!(
        c.values().filter(|v| v.0.is_some()).count(),
        12,
        "texts that come with a 373="
    );
    // The five without a 373= are not Rejects at all, which is why this enum is
    // not called RejectText.
    let no_code: Vec<(&str, &str)> = c
        .iter()
        .filter(|(_, v)| v.0.is_none())
        .map(|(k, v)| (k.as_str(), v.1.as_str()))
        .collect();
    assert_eq!(
        no_code,
        vec![
            ("Incorrect BeginString", "5"),
            ("MsgSeqNum too low, expecting 3 but received 1", "5"),
            ("MsgSeqNum too low, expecting 5 but received 2", "5"),
            ("No Products found for this Class Symbol", "d"),
            ("Unsupported Message Type", "j"),
        ]
    );
}

#[test]
fn every_text_renders_byte_for_byte() {
    let c = corpus();
    let mut buf = [0u8; 128];
    let mut seen = 0;
    for (text, (code, _, _)) in &c {
        let variant = SessionText::ALL
            .iter()
            .copied()
            .chain(embedded())
            .find(|v| {
                let n = v.render(&mut buf).unwrap_or(0);
                buf.get(..n) == Some(text.as_bytes())
            })
            .unwrap_or_else(|| panic!("no table entry renders {text:?}"));
        assert_eq!(
            variant.session_reject_reason(),
            *code,
            "wrong 373= for {text:?}"
        );
        seen += 1;
    }
    assert_eq!(seen, 17);
}

/// The two variants that carry numbers, with the corpus's own numbers.
fn embedded() -> impl Iterator<Item = SessionText> {
    [
        SessionText::MsgSeqNumTooLow {
            expecting: 3,
            received: 1,
        },
        SessionText::MsgSeqNumTooLow {
            expecting: 5,
            received: 2,
        },
    ]
    .into_iter()
}

#[test]
fn the_table_has_nothing_the_corpus_does_not() {
    let c = corpus();
    let mut buf = [0u8; 128];
    for v in SessionText::ALL {
        let n = v.render(&mut buf).unwrap_or(0);
        let s = String::from_utf8_lossy(&buf[..n]).into_owned();
        assert!(c.contains_key(&s), "table has {s:?}, the corpus does not");
    }
    assert_eq!(
        SessionText::ALL.len(),
        15,
        "17 texts, 2 of them one variant"
    );
}

#[test]
fn rendering_never_writes_past_the_buffer() {
    // A buffer too short must refuse rather than truncate: half a reason is
    // worse than none.
    let mut small = [0u8; 8];
    for v in SessionText::ALL {
        let before = small;
        if v.render(&mut small).is_none() {
            assert_eq!(small, before, "refused and still wrote");
        }
    }
    // 46 = "Value is incorrect (out of range) for this tag".
    let mut exact = [0u8; SessionText::MAX_FIXED_LEN];
    assert_eq!(SessionText::MAX_FIXED_LEN, 46);
    assert!(
        SessionText::ALL
            .iter()
            .all(|v| v.render(&mut exact).is_some()),
        "MAX_FIXED_LEN must fit every fixed text"
    );
    let mut one_short = [0u8; SessionText::MAX_FIXED_LEN - 1];
    assert!(
        SessionText::ALL
            .iter()
            .any(|v| v.render(&mut one_short).is_none()),
        "and it must be tight — one byte less has to fail somewhere"
    );

    // The numbered variant at its widest: two u32s at ten digits each.
    let mut wide = [0u8; SessionText::MAX_LEN];
    assert_eq!(SessionText::MAX_LEN, 63);
    assert!(
        SessionText::MsgSeqNumTooLow {
            expecting: u32::MAX,
            received: u32::MAX,
        }
        .render(&mut wide)
        .is_some()
    );
}

#[test]
fn the_numbers_are_rendered_not_formatted() {
    let mut buf = [0u8; 128];
    let n = SessionText::MsgSeqNumTooLow {
        expecting: 5,
        received: 2,
    }
    .render(&mut buf)
    .expect("renders");
    assert_eq!(&buf[..n], b"MsgSeqNum too low, expecting 5 but received 2");

    // Multi-digit, which the corpus never exercises and a real session will.
    let n = SessionText::MsgSeqNumTooLow {
        expecting: 1234,
        received: 7,
    }
    .render(&mut buf)
    .expect("renders");
    assert_eq!(
        &buf[..n],
        b"MsgSeqNum too low, expecting 1234 but received 7"
    );
}
