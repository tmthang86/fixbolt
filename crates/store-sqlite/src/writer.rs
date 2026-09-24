//! The writer thread, `fixbolt-sqlite`: everything the engine thread pushed,
//! committed to the database in batches. **The only thread that touches
//! SQLite after `open`.** ADR-0180 decisions 6–9.
//!
//! # The records on the ring
//!
//! The engine thread pushes what `FileJournal` `Async` pushes — `seq ‖ len ‖
//! payload`, little-endian — so both journals cost the engine thread the same:
//!
//! | Record | `seq` | `len` | payload |
//! |---|---|---|---|
//! | a message | its number | its length | its bytes |
//! | inbound mark (`mark_in`) | the number | 0 | — |
//! | activity mark (`mark_active`) | 0 | 8 | `u64` milliseconds |
//! | outbound mark (`mark_out`) | 0 | 4 | `u32` number |
//! | stop | one byte, [`STOP`] | | |

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;

use fixbolt_engine::journal::{Releaser, TicketState, WriterTicket};
use fixbolt_engine::redact::carries_secret;
use fixbolt_engine::ring::{Consumer, Idle};
use rusqlite::{Connection, Statement};

/// The stop record: one byte, which no other record can be — every other one
/// is at least [`HEADER`] bytes (ADR-0150 decision 2).
pub(crate) const STOP: u8 = 0xFF;

/// `seq` (4) and `len` (4) in front of every record but [`STOP`].
pub(crate) const HEADER: usize = 8;

/// `seq == 0` with this many payload bytes is an activity mark.
pub(crate) const ACTIVITY_LEN: usize = 8;

/// `seq == 0` with this many payload bytes is an outbound mark.
pub(crate) const OUTBOUND_LEN: usize = 4;

/// The writer's buffer for a journal whose slot holds `len` bytes: the
/// largest record the ring can carry, header included (ADR-0150 decision 1).
pub(crate) const fn buf_len(len: usize) -> usize {
    let most = if len > ACTIVITY_LEN {
        len
    } else {
        ACTIVITY_LEN
    };
    HEADER + most
}

/// What the writer tells the rest of the process, each one atomic: records
/// committed, the highest outbound number committed (`0` = none, a number no
/// session spends), and records a failed batch lost.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    pub(crate) committed: AtomicU64,
    pub(crate) highest_out: AtomicU64,
    pub(crate) failed: AtomicU64,
}

impl Counters {
    pub(crate) fn starting_at(highest_out: Option<u32>) -> Self {
        Self {
            committed: AtomicU64::new(0),
            highest_out: AtomicU64::new(u64::from(highest_out.unwrap_or(0))),
            failed: AtomicU64::new(0),
        }
    }
}

/// The marks the database holds, as the writer last committed them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Marks {
    pub(crate) highest_in: Option<u32>,
    pub(crate) highest_out: Option<u32>,
    pub(crate) last_active: Option<u64>,
}

/// Everything the writer thread owns, moved into it by `open`.
pub(crate) struct Writer {
    pub(crate) conn: Connection,
    pub(crate) from_engine: Consumer,
    pub(crate) ticket: WriterTicket,
    pub(crate) releaser: Releaser,
    pub(crate) counters: Arc<Counters>,
    pub(crate) marks: Marks,
    pub(crate) batch_max: u64,
    pub(crate) buf_len: usize,
}

/// What the writer does when a pop found the ring empty and no batch is
/// open — **ADR-0181's rule for a journal**, in one place so a unit test can
/// drive it a step at a time.
///
/// Told *retired, stop when dry*, the writer **pops once more** and stops
/// only if that pop is empty too: read the state, then find the ring empty,
/// never the other way round — a record pushed between an empty pop and the
/// state read would otherwise be lost.
#[derive(Debug, Default)]
pub(crate) struct DryRule {
    last_look: bool,
}

/// [`DryRule`]'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dry {
    /// Nothing more can come: stop.
    Stop,
    /// Told to stop when dry: pop once more before stopping.
    PopAgain,
    /// Nothing to do: wait by the engine's idle rule.
    Wait,
}

impl DryRule {
    pub(crate) fn on_empty(&mut self, ticket: &WriterTicket) -> Dry {
        if self.last_look {
            return Dry::Stop;
        }
        if ticket.state() == TicketState::RetiredStopWhenDry {
            self.last_look = true;
            return Dry::PopAgain;
        }
        Dry::Wait
    }
}

/// One transaction's worth of records.
#[derive(Debug, Default)]
struct Batch {
    records: u64,
    /// `BEGIN` succeeded: a `COMMIT` or `ROLLBACK` is owed.
    in_tx: bool,
    /// A statement failed: the batch is rolled back and counted.
    failed: bool,
    max_in: Option<u32>,
    max_out: Option<u32>,
    active: Option<u64>,
}

fn max(a: Option<u32>, b: u32) -> Option<u32> {
    Some(a.map_or(b, |a| a.max(b)))
}

impl Batch {
    fn add(&mut self, conn: &Connection, insert: &mut Statement<'_>, rec: &[u8]) {
        if self.records == 0 {
            match conn.execute_batch("BEGIN") {
                Ok(()) => self.in_tx = true,
                Err(_) => self.failed = true,
            }
        }
        self.records += 1;
        let (Some(head), Some(payload)) = (rec.get(..HEADER), rec.get(HEADER..)) else {
            return;
        };
        let mut word = [0u8; 4];
        word.copy_from_slice(head.get(..4).unwrap_or(&[0; 4]));
        let seq = u32::from_le_bytes(word);
        match (seq, payload.len()) {
            (0, ACTIVITY_LEN) => {
                let mut ms = [0u8; ACTIVITY_LEN];
                ms.copy_from_slice(payload);
                self.active = Some(u64::from_le_bytes(ms));
            }
            (0, OUTBOUND_LEN) => {
                word.copy_from_slice(payload);
                self.max_out = max(self.max_out, u32::from_le_bytes(word));
            }
            (0, _) => {}
            (seq, 0) => self.max_in = max(self.max_in, seq),
            (seq, _) => {
                // **A message carrying a secret is not inserted**, not even
                // masked: replayed after a restart, `554=********` would be a
                // wrong password on the wire. Its number is spent all the
                // same, so it raises `highest_out` and a restart gap-fills
                // it. ADR-0110 decision 4, ADR-0180 decision 7.
                self.max_out = max(self.max_out, seq);
                if !self.failed
                    && !carries_secret(payload)
                    && insert.execute((seq, payload)).is_err()
                {
                    self.failed = true;
                }
            }
        }
    }

    /// Commit, or roll back and count: **a failed batch is counted, not
    /// retried** (ADR-0180 decision 8).
    fn end(&mut self, conn: &Connection, marks: &mut Marks, counters: &Counters) {
        if self.records == 0 {
            return;
        }
        let next = Marks {
            highest_in: self
                .max_in
                .map_or(marks.highest_in, |b| max(marks.highest_in, b)),
            highest_out: self
                .max_out
                .map_or(marks.highest_out, |b| max(marks.highest_out, b)),
            last_active: self.active.or(marks.last_active),
        };
        let committed = self.in_tx
            && !self.failed
            && conn
                .execute(
                    "UPDATE session SET highest_in = ?1, highest_out = ?2, last_active_ms = ?3 \
                     WHERE id = 1",
                    (
                        next.highest_in,
                        next.highest_out,
                        next.last_active.and_then(|ms| i64::try_from(ms).ok()),
                    ),
                )
                .is_ok()
            && conn.execute_batch("COMMIT").is_ok();
        if committed {
            *marks = next;
            counters.committed.fetch_add(self.records, Ordering::AcqRel);
            counters
                .highest_out
                .store(u64::from(next.highest_out.unwrap_or(0)), Ordering::Release);
        } else {
            if self.in_tx {
                // Already rolled back by SQLite on some errors, in which case
                // this one fails and there is nothing left to undo.
                let _ = conn.execute_batch("ROLLBACK");
            }
            counters.failed.fetch_add(self.records, Ordering::AcqRel);
        }
        *self = Self::default();
    }
}

/// The writer thread's body. Says on `ready` whether it could start —
/// **after** allocating its buffer and preparing its statement, so `open`
/// returns with every writer allocation already made (ADR-0150 decision 1).
///
/// Stops on [`STOP`], or by [`DryRule`]; then commits what is open,
/// checkpoints the WAL into the database (`TRUNCATE`), **closes the
/// connection** — which lets go of SQLite's lock — then says so through its
/// [`Releaser`], then finishes its [`WriterTicket`]: ADR-0155 decision 2's
/// order, ADR-0180 decision 9.
pub(crate) fn run(w: Writer, ready: &SyncSender<std::io::Result<()>>) {
    let Writer {
        conn,
        mut from_engine,
        ticket,
        releaser,
        counters,
        mut marks,
        batch_max,
        buf_len,
    } = w;
    'work: {
        let mut buf = vec![0u8; buf_len];
        let mut insert =
            match conn.prepare("INSERT OR REPLACE INTO messages (seq, body) VALUES (?1, ?2)") {
                Ok(stmt) => stmt,
                Err(e) => {
                    let _ = ready.send(Err(std::io::Error::other(format!(
                        "sqlite store: prepare the insert: {e}"
                    ))));
                    break 'work;
                }
            };
        let _ = ready.send(Ok(()));
        let batch_max = batch_max.max(1);
        let mut batch = Batch::default();
        let mut idle = Idle::new();
        let mut dry = DryRule::default();
        loop {
            match from_engine.pop(&mut buf) {
                // **Not the stop signal**: a record longer than the buffer
                // was dropped. Unreachable from `SqliteJournal`, whose buffer
                // holds its largest record; skipped if reached.
                Some(0) => idle.reset(),
                Some(1) if buf.first() == Some(&STOP) => break,
                Some(n) => {
                    idle.reset();
                    if let Some(rec) = buf.get(..n) {
                        batch.add(&conn, &mut insert, rec);
                    }
                    if batch.records >= batch_max {
                        batch.end(&conn, &mut marks, &counters);
                    }
                }
                // The ring ran dry: one transaction per drain.
                None if batch.records > 0 => batch.end(&conn, &mut marks, &counters),
                None => match dry.on_empty(&ticket) {
                    Dry::Stop => break,
                    Dry::PopAgain => {}
                    Dry::Wait => idle.wait(),
                },
            }
        }
        batch.end(&conn, &mut marks, &counters);
        drop(insert);
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    }
    // Closing the connection is what releases SQLite's lock; only then may a
    // recovery hear "released".
    drop(conn);
    releaser.release();
    ticket.finish();
}

#[cfg(test)]
mod tests {
    use super::{Dry, DryRule};
    use fixbolt_engine::journal::WriterTicket;

    /// **The race the rule exists for, one step at a time** (plan row 3,
    /// R5b): the writer's pop finds the ring empty; the engine pushes a last
    /// record and only then says *stop when dry*; the writer reads that. It
    /// must pop once more — and so write the record — before it stops.
    #[test]
    fn a_record_pushed_between_an_empty_pop_and_stop_when_dry_is_written() {
        let (mut to_writer, mut from_engine) = fixbolt_engine::ring::pair(64);
        let ticket = WriterTicket::new();
        let mut dry = DryRule::default();
        let mut buf = [0u8; 16];

        assert_eq!(
            from_engine.pop(&mut buf),
            None,
            "the writer finds the ring empty"
        );
        assert!(
            to_writer.push(&[b"last"]),
            "the engine pushes its last record"
        );
        assert!(
            ticket.retire(false),
            "and retires, telling it: stop when dry"
        );

        assert_eq!(
            dry.on_empty(&ticket),
            Dry::PopAgain,
            "told stop-when-dry after an empty pop, the writer must look again"
        );
        assert_eq!(
            from_engine.pop(&mut buf),
            Some(4),
            "and that look finds the last record"
        );
        assert_eq!(from_engine.pop(&mut buf), None);
        assert_eq!(dry.on_empty(&ticket), Dry::Stop, "then, dry, it stops");
        ticket.finish();
    }

    /// Not retired, an empty ring means wait — however often it is asked.
    #[test]
    fn a_running_writer_waits_when_dry() {
        let ticket = WriterTicket::new();
        let mut dry = DryRule::default();
        for _ in 0..3 {
            assert_eq!(dry.on_empty(&ticket), Dry::Wait);
        }
    }
}
