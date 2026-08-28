//! The application the corpus assumes: an echo server that re-orders.
//!
//! `[measured]` 42 of the 250 `E` lines carry `35=D`. The acceptance server
//! sends application messages straight back, and
//! `15_HeaderAndBodyFieldsOrderedDifferently.def` sends the same message twice
//! with its fields in different orders and expects the **same bytes** back both
//! times. So the echo cannot copy the input's layout; it has to re-sort through
//! the dictionary, which is exactly non-negotiable 5.
//!
//! The gate is `9=101` — byte-exact, because tag 9 is not in `fields.fmt`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nanofix_conformance::echo::echo;
use nanofix_conformance::script::{Kind, Step, scenarios};

fn file(name: &str) -> Vec<Step> {
    scenarios()
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .find(|s| s.file == name)
        .unwrap_or_else(|| panic!("{name} not in the corpus"))
        .steps
}

/// Every `(I, E)` pair in a file where both are `35=<mt>`.
fn pairs(name: &str, mt: u8) -> Vec<(Vec<u8>, Vec<u8>)> {
    let steps = file(name);
    let mut out = Vec::new();
    let mut pending: Option<Vec<u8>> = None;
    for s in &steps {
        let Some(m) = Step::message(s) else { continue };
        let needle = [0x01, b'3', b'5', b'=', mt, 0x01];
        let is_mt = m.wire.windows(6).any(|w| w == needle);
        match &s.kind {
            Kind::Send(_) if is_mt => pending = Some(m.wire.clone()),
            Kind::Expect(_) if is_mt => {
                if let Some(i) = pending.take() {
                    out.push((i, m.wire.clone()));
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn the_echo_reproduces_the_expected_bytes_exactly() {
    let ps = pairs("15_HeaderAndBodyFieldsOrderedDifferently.def", b'D');
    assert_eq!(ps.len(), 2, "one ordered pair and one scrambled pair");

    let mut out = [0u8; 1024];
    for (n, (incoming, expected)) in ps.iter().enumerate() {
        let seq = seq_of(expected);
        let r = echo(incoming, &mut out, seq, b"00000000-00:00:00.000")
            .unwrap_or_else(|e| panic!("pair {n}: {e:?}"));
        // Byte for byte up to the trailer, then by the corpus's own rule: the
        // expected `10=0` is a placeholder and tag 10 is matched by shape.
        let actual = &out[r.clone()];
        let cut = |w: &[u8]| {
            w.windows(4)
                .position(|x| x == b"\x0110=")
                .map_or_else(|| w.to_vec(), |i| w[..=i].to_vec())
        };
        assert_eq!(
            String::from_utf8_lossy(&cut(actual)).replace('\x01', "|"),
            String::from_utf8_lossy(&cut(expected)).replace('\x01', "|"),
            "pair {n} did not echo byte for byte up to the trailer"
        );
        assert_eq!(
            nanofix_conformance::compare::compare(expected, actual),
            Ok(()),
            "pair {n} does not satisfy the corpus's own comparison rules"
        );
    }
}

#[test]
fn the_two_inputs_differ_and_the_two_outputs_do_not() {
    // The whole point of the file. If the echo copied the input's order these
    // two would differ, and `9=` would still be 101 in both — which is why the
    // ordering rule needs a positional comparator to catch it.
    let ps = pairs("15_HeaderAndBodyFieldsOrderedDifferently.def", b'D');
    assert_ne!(
        strip_seq(&ps[0].0),
        strip_seq(&ps[1].0),
        "inputs are the same"
    );
    assert_eq!(strip_seq(&ps[0].1), strip_seq(&ps[1].1), "outputs differ");
}

#[test]
fn body_length_is_one_hundred_and_one() {
    // Spelled out because it is the gate the plan names, and because it is the
    // number that only comes out right when 52 carries milliseconds (21 bytes)
    // and 60 is echoed verbatim from the input (17).
    let ps = pairs("15_HeaderAndBodyFieldsOrderedDifferently.def", b'D');
    let mut out = [0u8; 1024];
    let r = echo(&ps[0].0, &mut out, 2, b"00000000-00:00:00.000").expect("echo");
    let s = String::from_utf8_lossy(&out[r]).replace('\x01', "|");
    assert!(s.starts_with("8=FIX.4.4|9=101|35=D|"), "{s}");
    assert!(
        s.contains("|60=00000000-00:00:00|"),
        "60 must not gain millis: {s}"
    );
    assert!(
        s.contains("|52=00000000-00:00:00.000|"),
        "52 must have them: {s}"
    );
}

fn seq_of(wire: &[u8]) -> u32 {
    let i = wire
        .windows(4)
        .position(|w| w == b"\x0134=")
        .expect("34 present")
        + 4;
    let j = wire[i..].iter().position(|&b| b == 0x01).unwrap_or(0);
    String::from_utf8_lossy(&wire[i..i + j])
        .parse()
        .unwrap_or(0)
}

/// Drop `34=` and `9=` so two messages can be compared for field order alone.
fn strip_seq(wire: &[u8]) -> Vec<Vec<u8>> {
    wire.split(|&b| b == 0x01)
        .filter(|f| !f.is_empty() && !f.starts_with(b"34=") && !f.starts_with(b"9="))
        .map(<[u8]>::to_vec)
        .collect()
}

#[test]
fn every_application_echo_in_the_corpus_is_reproduced() {
    // 22 (I, E) pairs of application messages across the 59 files, not one.
    // This is what turns "the echo works on the file the plan named" into "the
    // echo works".
    let mut out = [0u8; 2048];
    let mut checked = 0;
    for s in scenarios().unwrap_or_else(|e| panic!("{e}")) {
        let mut pending: Option<Vec<u8>> = None;
        for st in &s.steps {
            let Some(m) = Step::message(st) else { continue };
            let app = m.wire.windows(6).any(|w| w == b"\x0135=D\x01");
            match &st.kind {
                Kind::Send(_) if app => pending = Some(m.wire.clone()),
                Kind::Expect(_) if app => {
                    let Some(incoming) = pending.take() else {
                        continue;
                    };
                    checked += 1;
                    let r = echo(
                        &incoming,
                        &mut out,
                        seq_of(&m.wire),
                        b"00000000-00:00:00.000",
                    )
                    .unwrap_or_else(|e| panic!("{}:{} {e:?}", st.file, st.line_no));
                    assert_eq!(
                        nanofix_conformance::compare::compare(&m.wire, &out[r]),
                        Ok(()),
                        "{}:{}",
                        st.file,
                        st.line_no
                    );
                }
                _ => {}
            }
        }
    }
    assert_eq!(checked, 22, "application echo pairs in the corpus");
}

#[test]
fn poss_resend_is_echoed_and_orig_sending_time_is_not() {
    // The two lines that decide the drop list, named. Guessing "all header
    // fields" one way or the other gets exactly one of them wrong.
    let mut out = [0u8; 2048];

    let ps = pairs("19b_PossResendMessageThatHasNotBeenSent.def", b'D');
    let (incoming, expected) = ps
        .iter()
        .find(|(i, _)| i.windows(4).any(|w| w == b"\x0197="))
        .expect("a D pair carrying PossResend");
    let r = echo(
        incoming,
        &mut out,
        seq_of(expected),
        b"00000000-00:00:00.000",
    )
    .expect("echo");
    let s = String::from_utf8_lossy(&out[r]).replace('\x01', "|");
    assert!(s.contains("|97=Y|"), "PossResend must survive: {s}");

    let ps = pairs("2m_BodyLengthValueNotCorrect.def", b'D');
    let (incoming, expected) = ps
        .iter()
        .find(|(i, _)| i.windows(5).any(|w| w == b"\x01122="))
        .expect("a D pair carrying OrigSendingTime");
    let r = echo(
        incoming,
        &mut out,
        seq_of(expected),
        b"00000000-00:00:00.000",
    )
    .expect("echo");
    let s = String::from_utf8_lossy(&out[r]).replace('\x01', "|");
    assert!(
        incoming.windows(5).any(|w| w == b"\x01122="),
        "the input carries OrigSendingTime"
    );
    assert!(
        !s.contains("|122="),
        "OrigSendingTime must not be echoed: {s}"
    );
}
