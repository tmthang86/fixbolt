//! The script loader, against all 59 files.
//!
//! Every number here was counted off the corpus on 2026-08-28 and is quoted in
//! the plan. A loader that silently drops a line still produces a plausible
//! transcript, so the counts are the gate — not "it parsed".
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_conformance::script::{Kind, Step, scenarios};

/// The corpus, or a loud failure. `LoadError` carries the fetch instruction.
fn steps() -> Vec<Step> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .flat_map(|s| s.steps)
        .collect()
}

#[test]
fn the_corpus_has_the_shape_it_had_when_the_plan_was_written() {
    let all = steps();
    let n = |f: fn(&Kind) -> bool| all.iter().filter(|s| f(&s.kind)).count();

    assert_eq!(scenarios().unwrap_or_default().len(), 59, "59 .def files");
    assert_eq!(n(|k| matches!(k, Kind::Send(_))), 289, "I lines");
    assert_eq!(n(|k| matches!(k, Kind::Expect(_))), 250, "E lines");
    assert_eq!(n(|k| matches!(k, Kind::Connect)), 65, "i…CONNECT");
    assert_eq!(
        n(|k| matches!(k, Kind::Disconnect)),
        1,
        "i1,DISCONNECT — there is one"
    );
    assert_eq!(
        n(|k| matches!(k, Kind::ExpectDisconnect)),
        64,
        "e…DISCONNECT"
    );
    assert_eq!(all.len(), 669, "289 + 250 + 66 + 64");
}

#[test]
fn concatenating_the_files_corrupts_the_corpus() {
    // 35 of the 59 .def files do not end in a newline, and most files begin
    // with a `#` comment. So `cat *.def` glues the last line of one file to the
    // first line of the next, and the corpus appears to carry comments on the
    // same line as a directive:
    //
    //     eDISCONNECT# If message is garbled, it should be ignored
    //
    // That is two lines from two files. This test exists because the claim
    // reached this crate's plan as a measured fact and was wrong — the tool was
    // wrong, not the corpus. Reproduced here so nobody re-derives it.
    let dir = fixbolt_conformance::script::definitions_dir();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "def"))
        .collect();
    files.sort();

    let no_newline = files
        .iter()
        .filter(|p| !std::fs::read(p).unwrap_or_default().ends_with(b"\n"))
        .count();
    assert_eq!(no_newline, 35, "files with no trailing newline");

    let glued: Vec<u8> = files
        .iter()
        .flat_map(|p| std::fs::read(p).unwrap_or_default())
        .collect();
    let naive = glued
        .split(|&b| b == b'\n')
        .filter(|l| l.strip_suffix(b"\r").unwrap_or(l) == b"eDISCONNECT")
        .count();
    assert_eq!(naive, 28, "what `cat *.def` makes eDISCONNECT look like");

    // The loader reads files one at a time and sees all 64.
    assert_eq!(
        steps()
            .iter()
            .filter(|s| matches!(s.kind, Kind::ExpectDisconnect))
            .count(),
        64
    );
}

#[test]
fn no_message_line_carries_a_hash() {
    // Follows from the above: there are no same-line comments anywhere, so a
    // loader that stripped `#` from a message line would be corrupting a value
    // to solve a problem that does not exist.
    assert!(
        !steps()
            .iter()
            .filter_map(Step::message)
            .any(|m| m.wire.contains(&b'#'))
    );
}

#[test]
fn the_two_multi_session_files_keep_their_session_numbers() {
    let all = steps();
    let tagged: Vec<&Step> = all.iter().filter(|s| s.session.is_some()).collect();
    assert_eq!(tagged.len(), 16, "3 I1, 2 I2, 3 E1, 3 i1, 2 i2, 1 e1, 2 e2");

    let files: std::collections::BTreeSet<&str> = tagged.iter().map(|s| Step::file(s)).collect();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        vec!["1b_DuplicateIdentity.def", "AlreadyLoggedOn.def"]
    );
}

/// The four message lines that are not a well-formed FIX 4.4 frame, and are not
/// a loader bug. Each is the entire point of the file it lives in.
const NOT_A_NORMAL_FRAME: &[(&str, usize, &str)] = &[
    (
        "1d_InvalidLogonWrongBeginString.def",
        4,
        "8=FIX.3.9 on the Logon",
    ),
    (
        "2i_BeginStringValueUnexpected.def",
        8,
        "8=FIX.4.1 on a TestRequest",
    ),
    (
        "2i_BeginStringValueUnexpected.def",
        18,
        "8=FIX.4.1 again, after a reconnect",
    ),
    (
        "2t_FirstThreeFieldsOutOfOrder.def",
        8,
        "35=0 before 8= — the whole point of the file",
    ),
];

#[test]
fn placeholders_are_gone_and_the_frame_is_complete() {
    let mut odd: Vec<(String, usize)> = Vec::new();
    for s in steps() {
        let Some(m) = s.message() else { continue };
        let (file, line_no) = (&s.file, s.line_no);
        let w = &m.wire;
        // These three hold for every line, including the deliberately broken
        // ones: fixify runs on all of them.
        assert!(
            !w.windows(6).any(|x| x == b"<TIME>"),
            "{file}:{line_no} kept a placeholder"
        );
        assert!(w.ends_with(b"\x01"), "{file}:{line_no} does not end in SOH");
        assert!(
            w.windows(4).any(|x| x == b"\x0110="),
            "{file}:{line_no} has no checksum field"
        );
        if !w.starts_with(b"8=FIX.4.4\x019=") {
            odd.push((file.clone(), line_no));
        }
    }
    // Named, not tolerated. A loader bug that mangled a frame would land here
    // too, and the list would stop matching.
    let expected: Vec<(String, usize)> = NOT_A_NORMAL_FRAME
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), *n))
        .collect();
    odd.sort();
    let mut expected = expected;
    expected.sort();
    assert_eq!(odd, expected, "\nunexpected set of malformed frames");
}

#[test]
fn a_time_placeholder_is_seventeen_bytes_inbound_and_twenty_one_outbound() {
    // Solved from the corpus's own 9= values, not chosen: over every line that
    // carries its own body length and a <TIME>, an I line's placeholder is 17
    // bytes (2d_GarbledMessage and 3c_GarbledMessage, two lines each) and an E
    // line's is 21 (SessionReset lines 18 and 27).
    //
    // An E line is the engine's own output and FIX 4.4 SendingTime carries
    // milliseconds; an I line is what the reflector sends and does not. Getting
    // it wrong costs four bytes per timestamp, which is invisible until
    // something compares a 9=.
    let (mut inbound, mut outbound) = ((0, 0), (0, 0));
    for s in steps() {
        let Some(m) = s.message() else { continue };
        for f in m.wire.split(|&b| b == 0x01) {
            if !(f.starts_with(b"52=") || f.starts_with(b"60=")) {
                continue;
            }
            let slot = match s.kind {
                Kind::Send(_) => &mut inbound,
                Kind::Expect(_) => &mut outbound,
                _ => continue,
            };
            match f.len() - 3 {
                17 => slot.0 += 1,
                21 => slot.1 += 1,
                _ => {}
            }
        }
    }
    assert_eq!(inbound, (343, 0), "every inbound timestamp is 17 bytes");
    // 247 of the outbound ones are the substituted 52=; the 45 at 17 bytes are
    // literal 60= values the corpus authors typed by hand.
    assert_eq!(outbound, (45, 247));
}
