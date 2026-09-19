//! A second opinion on FIXT 1.1 / FIX 5.0 SP2 repeating-group field order.
//!
//! `crates/dict/tests/interop_quickfix_order.rs` does this for FIX 4.4, against
//! `vendor/quickfix/src/C++/fix44/`; this file is its SP2 sibling, following
//! the same 730/730 pattern against `vendor/quickfix/src/C++/fix50sp2/`.
//! `docs/decisions/ADR-0083-…-sp2-oracle-comes-from.md` decision 3 is why the
//! oracle is fetched rather than approximated: comparing the generated table to
//! the XML it was built from proves stability, not correctness — this asks a
//! different program, QuickFIX's own twenty-year-old generator, reading the
//! same `FIX50SP2.xml`.
//!
//! **The headers are read, never copied, translated or committed.** They live
//! in gitignored `vendor/`, exactly as the `.def` files and the FIX 4.4 headers
//! do — `CLAUDE.md` §2 rule 9 and ADR-0001.
//!
//! # What agreement means, and what it does not
//!
//! The first two claims are the FIX 4.4 test's claims, unweakened, and they
//! hold on **every one of the 25 927 groups measured here, with zero
//! exceptions**:
//!
//! * every delimiter agrees, exactly;
//! * QuickFIX's order is an exact subsequence of this crate's, so no two tags
//!   are in opposite relative order and no tag QuickFIX has is missing from
//!   ours.
//!
//! The FIX 4.4 test's third claim — *every tag this crate has and QuickFIX
//! does not is itself a group counter* — **does not hold for SP2, and the gap
//! is QuickFIX's generator, not this crate's table.** Measured 2026-09-19,
//! reproduced against `FIX50SP2.xml` directly with a second, independent
//! recursive expansion (not `build.rs`, not this test's own parser):
//!
//! * `LegSecurityXML(1872)` and 23 fields shaped like it (a `<component>`
//!   wrapping exactly a `LENGTH` + `XMLDATA` + `STRING` triple, referenced
//!   from inside a group) are declared members of their group in the XML —
//!   `SecurityXML(1185)` alone accounts for 24 of the 64 094 occurrences
//!   below — and QuickFIX's own class sets them (`FIELD_SET(*this,
//!   FIX::LegSecurityXML)` is in `IOI.h`) but its generator's
//!   `message_order()` omits every one of them, everywhere in the corpus.
//! * `OrderQty(38)` inside `TradeCaptureReport`'s `NoSides(552)`
//!   (`TrdCapRptSideGrp` → `TradeReportOrderDetail` → `OrderQtyData`, two
//!   plain `<component>` levels down, no group in between) is the same
//!   story with no `XMLDATA` involved at all: a full independent recursive
//!   expansion of the component gives exactly 162 tags, matching this
//!   crate's table byte for byte in length, and QuickFIX's `message_order()`
//!   for that group still does not carry it, though `FIELD_SET(*this,
//!   FIX::OrderQty)` is in `TradeCaptureReport.h`.
//!
//! Both are two-or-more-level `<component>` nesting inside a `<group>`,
//! which FIX 4.4's shallower dictionary barely exercises — `interop_
//! quickfix_order.rs` never hit this because FIX 4.4 has nothing this deep.
//! So the third claim is **split in two** below: tags that stand in for a
//! nested group (the FIX 4.4 case, still asserted exactly) and tags that are
//! plain fields QuickFIX's generator drops (the new SP2 case, counted and
//! pinned, not asserted "impossible" — because it is not).
//!
//! # Two things this oracle cannot see, named rather than hidden
//!
//! `src/C++/fix50sp2/` holds QuickFIX's generated **application** messages
//! only — the eight FIXT admin messages (Logon, Heartbeat, …) are generated
//! under `src/C++/fixt11/`, which ADR-0083 decision 3 deliberately does not
//! fetch (its `Logon.h` carries no `FIX::Group(` at all, so it would assert
//! the loss ADR-0083 decision 4 exists to prevent). So this table's two
//! groups outside the 156 application messages — the header's `NoHops(627)`
//! and Logon's `NoMsgTypes(384)` (`MsgTypeGrp`, decision 4) — have no file to
//! appear in here, same as the header group in the FIX 4.4 test. The second
//! test below names both and asserts there are no others.
#![cfg(feature = "fix50sp2")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fixbolt_codec::Dictionary as _;
use fixbolt_dict::{Fixt11Fix50Sp2Tables as Fixt, fixt11_fix50sp2::GROUP_KEYS};

fn headers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/quickfix/src/C++/fix50sp2")
}

/// `(msg_type, counter) -> (delimiter, order)`, read out of the generated C++.
///
/// Identical logic to `interop_quickfix_order.rs::quickfix_groups` — kept as a
/// second copy rather than shared, because sharing it would mean this file
/// depends on that one compiling, and the two directories have different
/// shapes worth telling apart in a failure message.
fn quickfix_groups() -> BTreeMap<(String, u32), (u32, Vec<u32>)> {
    let dir = headers_dir();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\n\n\
             run scripts/fetch-quickfix-assets.sh — it now fetches src/C++/fix50sp2/ too.\n\
             An existing vendor/ checkout made by an older copy of that script has a\n\
             narrower sparse-checkout and will not have these files.",
            dir.display()
        )
    });

    // `read_dir` yields entries in an order no OS specifies, so this walk is
    // only reproducible if it sorts them first.
    let mut paths: Vec<PathBuf> = entries.map(|e| e.expect("dir entry").path()).collect();
    paths.sort();

    let mut out: BTreeMap<(String, u32), (u32, Vec<u32>)> = BTreeMap::new();
    for path in paths {
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
            // Two headers can declare the same MsgType. QuickFIX keeps a
            // stale copy of a renamed message beside the current one:
            // `ExecutionAcknowledgement.h` beside `ExecutionAck.h`, and
            // `MassQuoteAcknowledgement.h` beside `MassQuoteAck.h`, though
            // only `ExecutionAck` and `MassQuoteAck` are messages in the
            // `FIX50SP2.xml` this table was built from. `[measured
            // 2026-09-19]` 32 `(msg_type, counter)` keys are declared twice
            // that way and 21 of them carry a different `message_order`. So
            // keeping whichever header `read_dir` handed over first made the
            // numbers pinned below a property of the filesystem: two stale
            // headers are two independent coin flips, and CI and a developer
            // box landed on different faces of the same pin. All 32 of those
            // keys are declared by a current header too, so merging this way
            // and ignoring the two stale files outright give the same table;
            // merging is what says so out loud.
            //
            // The rule, and it is a rule rather than a tie-break: **keep the
            // richer order, and assert the other is an exact subsequence of
            // it.** The stale header is an older generation of the same group,
            // so it can differ only by fields the later revision added;
            // asserting that relation is what makes discarding it safe, and it
            // holds on all 21 today. Because the choice is made by the two
            // orders and never by arrival, the result no longer depends on the
            // sort above — the sort keeps everything else about this walk
            // reproducible, and both are wanted.
            match out.entry((mt.clone(), counter)) {
                Entry::Vacant(slot) => {
                    slot.insert((delim, order));
                }
                Entry::Occupied(mut slot) => {
                    let richer = {
                        let (have_delim, have_order) = slot.get();
                        assert_eq!(
                            *have_delim, delim,
                            "({mt}, {counter}) is declared twice with different delimiters"
                        );
                        let richer = order.len() > have_order.len();
                        let (long, short) = if richer {
                            (order.as_slice(), have_order.as_slice())
                        } else {
                            (have_order.as_slice(), order.as_slice())
                        };
                        assert!(
                            is_subsequence(short, long),
                            "({mt}, {counter}) is declared twice and neither order is a \
                             subsequence of the other, so one is not an older generation of \
                             the other and this test may not simply keep the richer.\n\
                             kept:  {have_order:?}\n\
                             other: {order:?}"
                        );
                        richer
                    };
                    if richer {
                        slot.insert((delim, order));
                    }
                }
            }
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
fn quickfix_and_this_crate_agree_on_every_sp2_group() {
    let qf = quickfix_groups();
    // `fix50sp2` is much larger than `fix44` (158 application messages built
    // from a far richer component library, against 93 for FIX 4.4) — a low
    // bound here would pass even if the parser above silently read only a
    // handful of files. 20 000 is comfortably below the measured 25 927 and
    // comfortably above "the parser found almost nothing".
    assert!(
        qf.len() > 20_000,
        "only {} groups read out of the headers — the parser above is wrong, \
         not QuickFIX",
        qf.len()
    );

    // Every counter that is a group of this message type, for the extras check.
    let mut counters: BTreeMap<&[u8], BTreeSet<u32>> = BTreeMap::new();
    for (mt, c) in GROUP_KEYS.iter() {
        counters.entry(mt).or_default().insert(*c);
    }

    // Extras split in two, per the module doc: a nested-group counter (the
    // FIX 4.4 case — asserted, not merely counted) and a plain field
    // QuickFIX's generator drops from `message_order()` for a
    // multi-level-`<component>`-inside-`<group>` chain (the SP2 case —
    // counted and pinned, never asserted impossible).
    let mut checked = 0;
    let mut nested_counter_extras: BTreeSet<u32> = BTreeSet::new();
    let mut dropped_field_extras: BTreeSet<u32> = BTreeSet::new();
    let mut dropped_field_occurrences = 0usize;
    for ((mt, counter), (delim, order)) in &qf {
        let mtb = mt.as_bytes();
        let mine = Fixt::group_members(mtb, *counter);
        assert!(
            !mine.is_empty(),
            "QuickFIX has a group ({mt}, {counter}) and this crate has none"
        );
        assert_eq!(
            Fixt::group_delimiter(mtb, *counter),
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
            if counters.get(mtb).is_some_and(|s| s.contains(t)) {
                nested_counter_extras.insert(*t);
            } else {
                dropped_field_extras.insert(*t);
                dropped_field_occurrences += 1;
            }
        }
        checked += 1;
    }

    println!(
        "agreed on {checked} SP2 groups; {} nested-counter extras; {} fields QuickFIX's \
         generator drops from message_order ({} occurrences)",
        nested_counter_extras.len(),
        dropped_field_extras.len(),
        dropped_field_occurrences
    );
    // `[measured 2026-09-19]` at pin `386ce46e`, on a clean build, after
    // `quickfix_groups` above was made independent of `read_dir` order. The
    // last three moved when it was: at this one pin the old code had four
    // answers, one per face of the two coin flips above — 226/941/64 094 (both
    // current headers, what is pinned here), 230/1 231/64 384, 231/1 307/
    // 64 460 (what CI read) and 231/1 307/64 750 (what a developer box read).
    // Pinned the way the FIX 4.4 test pins 730 — a change in any of the four
    // numbers below means either the pin moved (fetch script's `PINNED_SHA`),
    // the pair build's group table changed, or QuickFIX's generator changed,
    // and either way a human reads why before moving the number.
    assert_eq!(checked, 25_927, "every group in the generated SP2 headers");
    assert_eq!(
        nested_counter_extras.len(),
        226,
        "distinct tags that stand in for a nested group — the FIX 4.4 kind of extra"
    );
    assert_eq!(
        dropped_field_extras.len(),
        941,
        "distinct tags QuickFIX's generator drops from message_order — see the module doc"
    );
    assert_eq!(
        dropped_field_occurrences, 64_094,
        "how many (group, tag) pairs that covers"
    );
}

#[test]
fn the_only_groups_quickfix_has_no_message_for_are_the_header_and_logon_ones() {
    // `src/C++/fix50sp2/` holds the 156 application messages only. The two
    // groups this table has outside them — the header's `NoHops(627)` and
    // Logon's `NoMsgTypes(384)` (ADR-0083 decision 4's `MsgTypeGrp` override)
    // — have no file to appear in here, because Logon is generated under
    // `src/C++/fixt11/`, which decision 3 deliberately does not fetch.
    let qf = quickfix_groups();
    let mine: BTreeSet<(String, u32)> = GROUP_KEYS
        .iter()
        .map(|(m, c)| (String::from_utf8_lossy(m).into_owned(), *c))
        .collect();
    let mut only_mine: Vec<_> = mine
        .difference(&qf.keys().cloned().collect())
        .cloned()
        .collect();
    only_mine.sort();
    assert_eq!(
        only_mine,
        vec![(String::new(), 627), ("A".to_string(), 384)]
    );
    assert_eq!(
        mine.len(),
        25_929,
        "header + Logon on top of the 25 927 agreed groups"
    );
}
