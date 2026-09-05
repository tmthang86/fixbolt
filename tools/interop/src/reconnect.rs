//! `--role reconnect` — the engine's own reconnect loop, in front of a
//! `libquickfix` acceptor that dies and comes back.
//!
//! `STATUS.md` item 38, and the plan is
//! [reconnect-interop](../../../docs/plans/2026-09-05-reconnect-interop.md).
//!
//! # Why this is a third role and not an eighth step
//!
//! `--role initiator` drives [`fixbolt_session::Session`] directly over a
//! blocking socket and says so in its own rustdoc: *not under test: the
//! engine's polling loop*. It owns its reads, its writes and its timing. So a
//! reconnect step added there would exercise **the tool's** loop, and
//! [`fixbolt_engine::connect_and_serve`] — the thing
//! [ADR-0043](../../../docs/decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)
//! built and the thing item 38 is about — would still have been read by nobody
//! outside this repository.
//!
//! This role calls that function and nothing else. Everything below it —
//! backoff, the `Recovery` seam, `add_resumed`, the engine's turn — is the
//! product.
//!
//! # It scores nothing
//!
//! The judge is the C++ acceptor's transcript, read by `scripts/interop.sh`.
//! This process prints two kinds of line and no verdict:
//!
//! ```text
//! interop-reconnect: dialing 127.0.0.1:15646
//! interop-reconnect: delivered 34=2 35=B
//! ```
//!
//! A `delivered` line is stronger than *"the bytes arrived"*: it is written
//! from inside [`Handler::on_message`], so it says the message reached an
//! application through a session that accepted its sequence number. If
//! `next_in` had restarted, the acceptor's `34=5` would have opened a gap and
//! been asked for again rather than delivered
//! ([a-message-on-the-wire-is-not-a-message-delivered]).
//!
//! **A fixbolt process scoring its own reconnect is the mirrored corpus's
//! mistake again** — ADR-0042 decision 1 — so the assertions live in the
//! script and read `A1.log` and `A2.log`, which this process cannot write to.
//!
//! [a-message-on-the-wire-is-not-a-message-delivered]: ../../../docs/reference/a-message-on-the-wire-is-not-a-message-delivered.md

use std::path::{Path, PathBuf};

use fixbolt::{Answer, GroupData, GroupEntryData, Handler, Incoming, Peer, Reply};
use fixbolt_engine::journal::{Durability, FileJournal, Store};
use fixbolt_engine::msglog::NoLog;
use fixbolt_engine::reconnect::Policy;
use fixbolt_engine::recovery::{NoRecovery, Recovery, Resumed};
use fixbolt_session::Config;

/// The journal this role keeps on disk.
///
/// 64 slots of 1 KiB. The scenario replays nothing — it is `34=` continuity
/// under test, not the resend store — so the ring is sized to be obviously
/// enough rather than to be interesting.
type Disk = FileJournal<64, 1024>;

/// Speaks once when the session comes up, prints what comes back, and answers
/// nothing else.
///
/// # Why it originates at all
///
/// `[đo 2026-09-05]` **because a journal that holds no application message
/// cannot say what `34=` comes next.** `Journal::put` is called only on the
/// application path (`crates/session/src/lib.rs:1871`, `:2481`), so a session
/// that sends nothing but administrative messages leaves `highest()` at `None`,
/// [`OnDisk::recover`] answers `next_out = 1`, and the counterparty says so in
/// its own words: `MsgSeqNum too low, expecting 2 but received 1`. That was the
/// first run of this role, and the plan's Sửa 2 records it.
///
/// So this handler walks through [ADR-0048]'s door — the one opened the week
/// this file was written — and sends **one** `35=B` per logon. That puts `34=2`
/// in the journal, which makes `next_out = 3` the right answer rather than a
/// lucky one.
///
/// **The `35=B` is built by [`Reply`], not by hand.** Non-negotiable 5: field
/// order comes from the generated tables. `FIX44.xml` requires `148` Headline
/// **and** the `33` NoLinesOfText group with a `58` in each entry — a News
/// carrying only `148=` is refused by the counterparty's dictionary and never
/// reaches its application, which `tools/interop/src/desk.rs` already paid for.
///
/// [ADR-0048]: ../../../docs/decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md
#[derive(Default)]
pub struct Watch {
    logons: u32,
}

impl Handler for Watch {
    fn on_logon(&mut self, _who: Peer<'_>, nth: u32, reply: Reply<'_>) -> Answer {
        if nth > 0 {
            return reply.silent();
        }
        self.logons += 1;
        // A headline that names which logon this is, so a transcript shows the
        // second session is a second session and not a replayed first one.
        let headline: &[u8] = if self.logons == 1 {
            b"fixbolt initiator is up"
        } else {
            b"fixbolt initiator is back"
        };
        let text: [(u32, &[u8]); 1] = [(58, headline)];
        let entries = [GroupEntryData {
            fields: &text,
            groups: &[],
        }];
        reply
            .message(b"B")
            .field(148, headline)
            .group(33)
            .send_with_groups(&[GroupData {
                counter: 33,
                entries: &entries,
            }])
    }

    /// Print the inbound `34=` and `35=`, and say nothing back.
    ///
    /// **The number is read off the message, not taken from a parameter.**
    /// `[đo 2026-09-05]` the first version of this printed
    /// `Application::on_message`'s `seq`, which is the number the session would
    /// spend on a **reply** — so two News at `34=2` and `34=3` both printed
    /// `delivered 34=2`. The plan's Sửa 1. [`Incoming::seq`] is the number that
    /// arrived.
    ///
    /// **`println!` on the engine thread is deliberate and is not a hot-path
    /// claim.** Non-negotiable 1 is proven by `benches/alloc.rs` about the
    /// library crates; this is a gate whose entire output is these lines, and it
    /// moves a handful of messages in a couple of seconds.
    fn on_message(&mut self, msg: &Incoming<'_>, reply: Reply<'_>) -> Answer {
        println!(
            "interop-reconnect: delivered 34={} 35={}",
            String::from_utf8_lossy(msg.seq().unwrap_or(b"?")),
            String::from_utf8_lossy(msg.msg_type())
        );
        reply.silent()
    }
}

/// One `FileJournal` per counterparty, on a path this role was given.
///
/// `[đo 2026-09-05]` **This used to derive all three numbers by hand, and two of
/// the three were wrong in different ways** — `next_out` from `journal.highest()`
/// (short by every administrative message since the last application one), and
/// `on_disk.rs`'s copy of the same block dropped the `+ 1` on `next_in`. Both are
/// now [`Resumed::from_journal`], which is why that function exists: arithmetic
/// written twice was got wrong twice. ADR-0053.
struct OnDisk {
    path: PathBuf,
    how: Durability,
}

impl OnDisk {
    /// Open the path once, so a bad one is an error here rather than a process
    /// that exits from inside a trait method three seconds later.
    fn probe(path: &Path, how: Durability) -> std::io::Result<Self> {
        drop(FileJournal::<64, 1024>::open(path, how)?);
        Ok(Self {
            path: path.to_path_buf(),
            how,
        })
    }

    /// Open the journal, or say why and stop.
    ///
    /// **`exit` rather than `unwrap`**: non-negotiable 7 denies `unwrap`,
    /// `expect` and `panic!` workspace-wide, and neither [`Recovery`] method
    /// can return an error. [`Self::probe`] has already opened this exact path
    /// once, so reaching this arm means the filesystem changed under a running
    /// gate — which is not a result, and a gate that carries on from it would
    /// report a reconnect failure that was really a disk failure.
    fn open(&self) -> Disk {
        match FileJournal::open(&self.path, self.how) {
            Ok(j) => j,
            Err(e) => {
                println!(
                    "interop-reconnect: FAIL journal {}: {e}",
                    self.path.display()
                );
                std::process::exit(1)
            }
        }
    }
}

impl Recovery<Disk> for OnDisk {
    fn fresh(&mut self, _cfg: &Config) -> Disk {
        self.open()
    }

    fn recover(&mut self, _cfg: &Config) -> Option<Resumed<Disk>> {
        // **`from_journal`, not three lines of arithmetic.** `[đo 2026-09-05]`
        // the three lines that used to be here derived `next_out` from
        // `journal.highest()`, which is the highest message held for a
        // *replay* — short by every administrative message since the last
        // application one. A real `libquickfix` answered *"MsgSeqNum too low,
        // expecting 4 but received 3"*. ADR-0053.
        //
        // `None` here is the ordinary answer on the first attempt: nothing was
        // left behind, the engine asks `fresh`, and the session starts at
        // `34=1`.
        let resumed = Resumed::from_journal(self.open())?;
        println!(
            "interop-reconnect: resuming next_out={} next_in={}",
            resumed.next_out, resumed.next_in
        );
        Some(resumed)
    }
}

/// Dial, and keep dialling. Returns only when the process is killed or the
/// engine cannot be built.
pub fn run(args: &[String]) -> std::process::ExitCode {
    let addr = crate::arg(args, "--connect").unwrap_or_else(|| "127.0.0.1:15646".to_owned());
    let sender = crate::arg(args, "--sender").unwrap_or_else(|| "FIXBOLT".to_owned());
    let target = crate::arg(args, "--target").unwrap_or_else(|| "QFACC".to_owned());
    let first_ms = ms(args, "--first-ms", 200);
    let ceiling_ms = ms(args, "--ceiling-ms", 2_000);

    // **`30` by default, and `--heart-bt-int` exists because that default used
    // to be the reason a scenario passed.**
    //
    // `[đo 2026-09-05]` 30 was chosen so no Heartbeat falls inside the few
    // seconds this scenario runs, keeping the `34=` on the transcript the ones
    // the protocol put there (the reconnect plan's trap 4). It also meant the
    // `SIGKILL` scenario was green **because no administrative message was sent
    // after the last application one** — the exact condition item 48 was about.
    // A gate that passes because of how its fixture is configured is not
    // reporting on the engine.
    //
    // So the script now runs a second `SIGKILL` round at `--heart-bt-int 1`
    // with a deliberate pause before the kill, which puts at least one
    // Heartbeat between the `35=B` and the death. That round is what says the
    // fix is about *every* administrative message rather than about `35=5`.
    // ADR-0053.
    let beat = u32::try_from(ms(args, "--heart-bt-int", 30)).unwrap_or(30);
    let cfg =
        Config::initiator(b"FIX.4.4", sender.as_bytes(), target.as_bytes()).with_heart_bt_int(beat);
    println!("interop-reconnect: HeartBtInt={beat}");

    let policy = match Policy::new(first_ms, ceiling_ms) {
        Ok(p) => p,
        Err(e) => {
            println!("interop-reconnect: FAIL policy {first_ms}/{ceiling_ms}: {e:?}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!("interop-reconnect: {sender} -> {target}, FIX.4.4");
    println!("interop-reconnect: backoff {first_ms} ms doubling to {ceiling_ms} ms");

    // **The reversal switch.** `--no-recovery` is `NoRecovery`, which
    // `GUIDE.md` §8c warns restarts the numbering on every reconnect — the
    // easiest mistake a deployment can make here. Running the gate with it must
    // turn the `next_out` assertion red; if it does not, the assertion is not
    // reading what it claims to. It needs a different journal type, because
    // `NoRecovery` is `Recovery<J>` only for `J: Default` and a `FileJournal`
    // has no honest `Default` — it needs a path (ADR-0039).
    if args.iter().any(|a| a == "--no-recovery") {
        println!("interop-reconnect: NoRecovery — every reconnect starts at 34=1");
        println!("interop-reconnect: dialing {addr}");
        return match fixbolt_engine::connect_and_serve::<_, Store, NoRecovery, NoLog>(
            &addr,
            cfg,
            fixbolt::app(Watch::default()),
            policy,
            NoRecovery,
            NoLog,
        ) {
            Ok(s) => {
                println!("interop-reconnect: stopped: {s:?}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                println!("interop-reconnect: FAIL serve: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    let Some(path) = crate::arg(args, "--journal") else {
        println!("interop-reconnect: FAIL --role reconnect needs --journal <path>");
        return std::process::ExitCode::FAILURE;
    };
    // `Durability::Async` as the plan says. The journal is re-opened by
    // `recover` on every attempt while the process keeps running, so what is
    // being taken on trust is that the previous `FileJournal` has been dropped
    // — and its writer thread joined — before the next one opens the same path.
    // If that turns out to be false the `next_out` assertion is what sees it,
    // and the plan's trap 5 says what to do about it.
    let recovery = match OnDisk::probe(Path::new(&path), Durability::Async) {
        Ok(r) => r,
        Err(e) => {
            println!("interop-reconnect: FAIL journal {path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!("interop-reconnect: journal {path}");
    println!("interop-reconnect: dialing {addr}");

    match fixbolt_engine::connect_and_serve::<_, Disk, OnDisk, NoLog>(
        &addr,
        cfg,
        fixbolt::app(Watch::default()),
        policy,
        recovery,
        NoLog,
    ) {
        Ok(s) => {
            println!("interop-reconnect: stopped: {s:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("interop-reconnect: FAIL serve: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// A millisecond argument, or `default` if it is absent or unreadable.
fn ms(args: &[String], name: &str, default: u64) -> u64 {
    crate::arg(args, name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
