//! The three FIXT `.def` corpora, loaded through the same loader the 59 FIX
//! 4.4 definitions use.
//!
//! `[measured 2026-09-19]`, ADR-0080 *Context*, "The oracle:" paragraph: the
//! three directories under `vendor/quickfix/test/definitions/server/`
//! (`fix50`, `fix50sp1`, `fix50sp2`) hold 60 files each and differ only in
//! the counterparty `SenderCompID` (`49=`) and the `DefaultApplVerID(1137)`
//! value a Logon carries.
//!
//! # "Exactly one file per dir missing `1137`" means a missing *Logon*
//!
//! `1d_InvalidLogonNoDefaultApplVerID.def`'s one `I` line is a Logon (`35=A`)
//! with no `1137` — the counter-example the plan row means. Every corpus also
//! has `1e_NotLogonMessage.def`, whose one `I` line is `35=0` (Heartbeat, not
//! Logon): it carries no `1137` either, but it is not a counter-example to
//! "every Logon carries 1137", because it has no Logon to carry one on. This
//! test scopes the check to Logon lines so `1e` does not double the count.
//!
//! # A second, narrower exception, found while writing this test
//!
//! `[measured 2026-09-19]` `1c_InvalidSenderCompID.def`'s one Logon line
//! carries `49=WT`, not the corpus's declared CompID — by design, that is the
//! whole point of the file: an acceptor must refuse a Logon whose
//! `SenderCompID` is wrong, so the corpus writes one. It is the *only* file
//! per corpus whose Logon's `49=` disagrees with the corpus's declared
//! CompID; `2k_CompIDDoesNotMatchProfile.def` also carries a deliberately
//! wrong `49=` (`WT`), but on a later `NewOrderSingle` line, not on its
//! Logon, so it does not count here. Verified by scanning all three corpora's
//! `I` Logon lines before writing the assertion below, not assumed from the
//! plan row (which does not mention a CompID check at all — this fact was
//! added by the brief for B3 and needed checking against the real corpus
//! before it could be asserted as written).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{Kind, Scenario, fixt_corpora, load_corpus};

/// The one file per corpus whose Logon has no `1137` — see the module doc.
const MISSING_1137_FILE: &str = "1d_InvalidLogonNoDefaultApplVerID.def";

/// The one file per corpus whose Logon carries a `SenderCompID` other than
/// the corpus's declared one — see the module doc.
const WRONG_COMPID_LOGON_FILE: &str = "1c_InvalidSenderCompID.def";

/// The value of `tag=` in an SOH-joined field list, or `None` if the field is
/// absent.
fn field<'a>(wire: &'a [u8], tag: &str) -> Option<&'a [u8]> {
    let prefix = format!("{tag}=");
    wire.split(|b| *b == 0x01).find_map(|f| {
        f.starts_with(prefix.as_bytes())
            .then(|| f.get(prefix.len()..))
            .flatten()
    })
}

fn is_logon(wire: &[u8]) -> bool {
    field(wire, "35") == Some(b"A")
}

/// Every `I` Logon line's wire bytes, across every scenario in `corpus`,
/// paired with the file it came from.
fn logon_lines(corpus: &[Scenario]) -> Vec<(&str, &[u8])> {
    corpus
        .iter()
        .flat_map(|s| {
            s.steps.iter().filter_map(move |step| match &step.kind {
                Kind::Send(m) if is_logon(&m.wire) => Some((s.file.as_str(), m.wire.as_slice())),
                _ => None,
            })
        })
        .collect()
}

#[test]
fn all_180_files_parse_and_each_corpus_yields_60_scenarios() {
    let mut total = 0;
    for corpus in fixt_corpora() {
        let scenarios = load_corpus(&corpus).unwrap_or_else(|e| panic!("{}: {e}", corpus.dir));
        assert_eq!(
            scenarios.len(),
            60,
            "{}: expected 60 scenarios, found {}",
            corpus.dir,
            scenarios.len()
        );
        total += scenarios.len();
    }
    assert_eq!(total, 180, "3 corpora x 60 files");
    println!("fixt_corpus: 180 / 180 files parsed, 60 / 60 scenarios per corpus");
}

#[test]
fn every_logon_carries_1137_except_one_file_per_corpus() {
    let mut total_missing = 0;
    for corpus in fixt_corpora() {
        let scenarios = load_corpus(&corpus).unwrap_or_else(|e| panic!("{}: {e}", corpus.dir));
        let missing: Vec<&str> = logon_lines(&scenarios)
            .into_iter()
            .filter(|(_, wire)| field(wire, "1137").is_none())
            .map(|(file, _)| file)
            .collect();
        assert_eq!(
            missing.len(),
            1,
            "{}: expected exactly one Logon missing 1137, found {missing:?}",
            corpus.dir
        );
        total_missing += missing.len();
    }
    println!("fixt_corpus: {total_missing} / 3 corpora have exactly one Logon missing 1137");
}

#[test]
fn the_file_missing_1137_is_1d_invalid_logon_no_default_appl_ver_id() {
    for corpus in fixt_corpora() {
        let scenarios = load_corpus(&corpus).unwrap_or_else(|e| panic!("{}: {e}", corpus.dir));
        let missing: Vec<&str> = logon_lines(&scenarios)
            .into_iter()
            .filter(|(_, wire)| field(wire, "1137").is_none())
            .map(|(file, _)| file)
            .collect();
        assert_eq!(
            missing,
            vec![MISSING_1137_FILE],
            "{}: the file missing 1137 should be {MISSING_1137_FILE}",
            corpus.dir
        );
    }
    println!("fixt_corpus: the missing-1137 file is {MISSING_1137_FILE} in all 3 corpora");
}

#[test]
fn logon_1137_and_compid_match_the_corpus_declaration() {
    let mut checked_1137 = 0;
    let mut checked_compid = 0;
    for corpus in fixt_corpora() {
        let scenarios = load_corpus(&corpus).unwrap_or_else(|e| panic!("{}: {e}", corpus.dir));
        for (file, wire) in logon_lines(&scenarios) {
            if let Some(v) = field(wire, "1137") {
                assert_eq!(
                    v,
                    corpus.default_appl_ver_id.as_bytes(),
                    "{}/{file}: Logon 1137 should equal the corpus's declared value",
                    corpus.dir
                );
                checked_1137 += 1;
            }
            if let Some(v) = field(wire, "49") {
                if file == WRONG_COMPID_LOGON_FILE {
                    // The one deliberate exception — see the module doc.
                    assert_ne!(
                        v,
                        corpus.comp_id.as_bytes(),
                        "{}/{file}: expected this file's Logon to carry the wrong CompID by design",
                        corpus.dir
                    );
                } else {
                    assert_eq!(
                        v,
                        corpus.comp_id.as_bytes(),
                        "{}/{file}: Logon 49 should equal the corpus's declared CompID",
                        corpus.dir
                    );
                }
                checked_compid += 1;
            }
        }
    }
    println!(
        "fixt_corpus: 1137 checked on {checked_1137} Logons, CompID checked on {checked_compid} Logons, 1 declared exception per corpus"
    );
}
