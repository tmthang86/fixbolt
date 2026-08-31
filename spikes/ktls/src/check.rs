//! A tally that prints one greppable line per assertion.
//!
//! Every line a script may key on is a fixed token in column two. The prose
//! after the em dash is for a person and is never matched on.

use std::fmt::Display;

#[derive(Default)]
pub struct Checks {
    pub pass: usize,
    pub fail: usize,
}

/// One assertion, recorded on a worker thread and merged into the tally later.
pub struct Record {
    pub token: &'static str,
    pub ok: bool,
    pub detail: String,
}

impl Checks {
    pub fn assert(&mut self, token: &'static str, ok: bool, detail: impl Display) {
        if ok {
            self.pass += 1;
            println!("PASS {token} — {detail}");
        } else {
            self.fail += 1;
            println!("FAIL {token} — {detail}");
        }
    }

    /// An observation that is reported but does not gate. Findings go here when
    /// the spike measures something it has no prior expectation for.
    pub fn note(&self, token: &str, detail: impl Display) {
        println!("NOTE {token} — {detail}");
    }

    pub fn merge(&mut self, records: Vec<Record>) {
        for r in records {
            self.assert(r.token, r.ok, r.detail);
        }
    }

    pub fn summary(&self) {
        println!("SPIKE pass {} fail {}", self.pass, self.fail);
    }
}

/// The worker-thread side of the same thing: collect, do not print, so that the
/// two threads' output cannot interleave into an unreadable transcript.
#[derive(Default)]
pub struct Recorder(pub Vec<Record>);

impl Recorder {
    pub fn assert(&mut self, token: &'static str, ok: bool, detail: impl Display) {
        self.0.push(Record {
            token,
            ok,
            detail: detail.to_string(),
        });
    }
}
