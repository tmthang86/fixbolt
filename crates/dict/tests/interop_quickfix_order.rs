//! A second opinion on repeating-group field order.
//!
//! `crates/codec/tests/group_roundtrip.rs` proves parse-then-encode is
//! byte-stable, but it generates its messages from the same `group_order` table
//! the encoder reads, so it cannot notice if that table is wrong. This one asks
//! a different program.
//!
//! QuickFIX ships **generated C++** for FIX 4.4 — `src/C++/fix44/*.h`, one file
//! per message — and each group appears there as
//!
//! ```text
//! NoMDEntries() : FIX::Group(268,279,FIX::message_order(279,285,269,...,0)) {}
//! ```
//!
//! counter, delimiter, order. Written by QuickFIX's generator, from the same
//! XML, twenty years before this one. Reading the order out of it is not the
//! same as reading the XML again.
//!
//! **The headers are read, never copied, translated or committed.** They live in
//! gitignored `vendor/`, exactly as the `.def` files do — `CLAUDE.md` §2 rule 9
//! and ADR-0001. This test extends "data and a test oracle" to one more set of
//! files under the same terms.
//!
//! # What agreement means, and what it does not
//!
//! QuickFIX's `message_order` and this crate's `group_members` do not mean quite
//! the same thing: QuickFIX lists a nested group's counter only sometimes, while
//! this crate always does — the writer walks the list and emits the nested group
//! when it reaches the counter, so the counter has to be in it. So the claim
//! checked here is **subsequence**, not equality:
//!
//! * every delimiter agrees, exactly;
//! * QuickFIX's order is an exact subsequence of this crate's, so no two tags
//!   are in opposite relative order and no tag is missing;
//! * every tag this crate has and QuickFIX does not is itself a group counter.
//!
//! `[measured 2026-08-28]` 730 / 730 on all three.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fixbolt_codec::Dictionary;
use fixbolt_dict::{Fix44, GROUP_KEYS};

fn headers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/quickfix/src/C++/fix44")
}

/// `(msg_type, counter) -> (delimiter, order)`, read out of the generated C++.
fn quickfix_groups() -> BTreeMap<(String, u32), (u32, Vec<u32>)> {
    let dir = headers_dir();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\n\
             run scripts/fetch-quickfix-assets.sh — it now fetches src/C++/fix44/ too.\n\
             An existing vendor/ checkout made by an older copy of that script has a\n\
             narrower sparse-checkout and will not have these files.",
            dir.display()
        )
    });

    let mut out = BTreeMap::new();
    for e in entries {
        let path = e.expect("dir entry").path();
        if path.extension().is_none_or(|x| x != "h") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read header");
        // Message.h and MessageCracker.h are infrastructure, not messages.
        let Some(mt) = between(&text, "MsgType(\"", "\")") else {
            continue;
        };
        for spec in text.split("FIX::Group(").skip(1) {
            let Some(args) = spec.split(')').next() else {
                continue;
            };
            // "268,279,FIX::message_order(279,285,...,0"
            let mut it = args.split(',');
            let (Some(c), Some(d)) = (it.next(), it.next()) else {
                continue;
            };
            let (Ok(counter), Ok(delim)) = (c.trim().parse::<u32>(), d.trim().parse::<u32>())
            else {
                continue;
            };
            let order: Vec<u32> = it
                .filter_map(|t| {
                    t.trim()
                        .trim_start_matches("FIX::message_order(")
                        .parse()
                        .ok()
                })
                .filter(|t| *t != 0)
                .collect();
            if order.is_empty() {
                continue;
            }
            out.entry((mt.clone(), counter)).or_insert((delim, order));
        }
    }
    out
}

fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let i = s.find(open)? + open.len();
    let j = s[i..].find(close)? + i;
    Some(s[i..j].to_string())
}

/// Is `needle` an exact subsequence of `hay`?
fn is_subsequence(needle: &[u32], hay: &[u32]) -> bool {
    let mut it = hay.iter();
    needle.iter().all(|x| it.any(|y| y == x))
}

#[test]
fn quickfix_and_this_crate_agree_on_every_group() {
    let qf = quickfix_groups();
    assert!(
        qf.len() > 700,
        "only {} groups read out of the headers — the parser above is wrong, \
         not QuickFIX",
        qf.len()
    );

    // Every counter that is a group of this message type, for the extras check.
    let mut counters: BTreeMap<&[u8], BTreeSet<u32>> = BTreeMap::new();
    for (mt, c) in GROUP_KEYS.iter() {
        counters.entry(mt).or_default().insert(*c);
    }

    let mut checked = 0;
    let mut extras: BTreeSet<u32> = BTreeSet::new();
    for ((mt, counter), (delim, order)) in &qf {
        let mtb = mt.as_bytes();
        let mine = Fix44::group_members(mtb, *counter);
        assert!(
            !mine.is_empty(),
            "QuickFIX has a group ({mt}, {counter}) and this crate has none"
        );
        assert_eq!(
            Fix44::group_delimiter(mtb, *counter),
            Some(*delim),
            "({mt}, {counter}) delimiter"
        );
        assert!(
            is_subsequence(order, mine),
            "({mt}, {counter}): QuickFIX's order is not a subsequence of this crate's.\n\
             quickfix: {order:?}\n\
             mine:     {mine:?}"
        );
        for t in mine.iter().filter(|t| !order.contains(t)) {
            assert!(
                counters.get(mtb).is_some_and(|s| s.contains(t)),
                "({mt}, {counter}): this crate lists {t}, QuickFIX does not, and {t} is \
                 not a group counter — so it is an invented field, not a nesting hook"
            );
            extras.insert(*t);
        }
        checked += 1;
    }

    println!(
        "agreed on {checked} groups; {} nested-counter extras",
        extras.len()
    );
    assert_eq!(checked, 730, "every group in the generated headers");
}

#[test]
fn the_only_group_quickfix_has_no_message_for_is_the_header_one() {
    // QuickFIX generates one file per message and the header is not a message,
    // so NoHops(627) has no header file to appear in. 730 + 1 = 731.
    let qf = quickfix_groups();
    let mine: BTreeSet<(String, u32)> = GROUP_KEYS
        .iter()
        .map(|(m, c)| (String::from_utf8_lossy(m).into_owned(), *c))
        .collect();
    let only_mine: Vec<_> = mine
        .difference(&qf.keys().cloned().collect())
        .cloned()
        .collect();
    assert_eq!(only_mine, vec![(String::new(), 627)]);
    assert_eq!(mine.len(), 731);
}
