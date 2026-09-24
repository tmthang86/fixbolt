//! The exporter's one thread: wake every `tick`, do what is waiting, sleep —
//! ADR-0170 decision 5.
//!
//! Each wake: drain the event stream if the caller handed it over, accept every
//! pending connection (the listener is non-blocking), answer each on a blocking
//! socket bounded by a read timeout and [`REQUEST_CAP`], and sleep again. It
//! never spins and it never wakes an engine: a scrape raises the engine's flag
//! and waits, on **this** thread, for at most `fresh_wait` — an engine asleep
//! in `poll` stays asleep, and the snapshot's age says so.
//!
//! **Nothing here allocates after start-up** (decision 4): every buffer is
//! made in [`Loop::new`], and `benches/alloc.rs` counts the whole process over
//! a window of scrapes to prove it.

use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use fixbolt_engine::observe::{EVENT_CAPACITY, Event, EventKind, MAX_SESSIONS, Observer};

use crate::encode::{self, Counts, EngineView, ExporterView, Numbers, SessionView, View};
use crate::http::{self, REQUEST_CAP, Received, Request, Route, Status};
use crate::series;

/// The most connections one wake answers before it drains events, checks the
/// clock and sleeps again. Each connection is bounded by its own deadline, so a
/// wake is bounded by this many deadlines.
const ANSWERS_PER_WAKE: usize = 64;

/// The caller's handler for events, run on the exporter thread: the name the
/// event's engine was given at `.engine(name, …)`, and the event.
pub(crate) type OnEvent = Box<dyn FnMut(&'static str, &Event) + Send + 'static>;

/// The four durations the builder sets.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timings {
    pub(crate) min_request_interval: Duration,
    pub(crate) fresh_wait: Duration,
    pub(crate) tick: Duration,
    pub(crate) read_timeout: Duration,
}

/// One engine, as the exporter keeps it between scrapes.
pub(crate) struct Watched {
    name: &'static str,
    observer: Observer,
    /// `published()` when the exporter last looked.
    seen: u64,
    /// When the exporter last saw `published()` move.
    moved_at: Option<Instant>,
    /// `published()` when the exporter last asked.
    asked_at: u64,
    age_ms: Option<u64>,
    events_lost: u64,
    numbers: Option<Numbers>,
    sessions: Box<[SessionView; MAX_SESSIONS]>,
    len: usize,
    events: bool,
    counts: Counts,
}

impl Watched {
    pub(crate) fn new(name: &'static str, observer: Observer, events: bool) -> Self {
        Self {
            name,
            observer,
            seen: 0,
            moved_at: None,
            asked_at: 0,
            age_ms: None,
            events_lost: 0,
            numbers: None,
            sessions: Box::new([SessionView::default(); MAX_SESSIONS]),
            len: 0,
            events,
            counts: Counts::default(),
        }
    }

    /// Take the newest snapshot if the engine has published since the last
    /// look — through `latest`, which asks for nothing — and bring the age up
    /// to `now`.
    fn absorb(&mut self, now: Instant) {
        let published = self.observer.published();
        if published != self.seen {
            self.seen = published;
            self.moved_at = Some(now);
            if let Some(snap) = self.observer.latest() {
                self.numbers = Some(Numbers::of(&snap));
                self.len = 0;
                for (slot, s) in self.sessions.iter_mut().zip(snap.sessions()) {
                    *slot = SessionView::of(s);
                    self.len += 1;
                }
            }
        }
        self.events_lost = self.observer.events_lost();
        self.age_ms = self
            .moved_at
            .map(|t| u64::try_from(now.duration_since(t).as_millis()).unwrap_or(u64::MAX));
    }

    /// Count one event under its `kind`, and under its `reason` if it ended a
    /// session with one.
    fn count(&mut self, event: &Event) {
        let kind = event.kind();
        if let Some(n) = self.counts.kinds.get_mut(series::event_kind_index(&kind)) {
            *n = n.saturating_add(1);
        }
        if let EventKind::Ended(reason) = kind
            && let Some(n) = self.counts.reasons.get_mut(series::reason_index(&reason))
        {
            *n = n.saturating_add(1);
        }
    }
}

impl View for Watched {
    fn view(&self) -> EngineView<'_> {
        EngineView {
            name: self.name,
            published: self.seen,
            events_lost: self.events_lost,
            age_ms: self.age_ms,
            numbers: self.numbers,
            sessions: self.sessions.get(..self.len).unwrap_or(&[]),
            events: self.events.then_some(&self.counts),
        }
    }
}

/// Everything the exporter thread owns.
pub(crate) struct Loop {
    listener: TcpListener,
    engines: Vec<Watched>,
    timings: Timings,
    on_event: Option<OnEvent>,
    event_buf: Vec<Event>,
    body: Vec<u8>,
    request: Box<[u8; REQUEST_CAP]>,
    last_ask: Option<Instant>,
    scrapes: u64,
    bad_requests: u64,
    stop: Arc<AtomicBool>,
}

impl Loop {
    /// Every buffer the loop will ever use, reserved here and never grown.
    pub(crate) fn new(
        listener: TcpListener,
        engines: Vec<Watched>,
        timings: Timings,
        on_event: Option<OnEvent>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let events = on_event.is_some();
        let body = Vec::with_capacity(encode::capacity_for(engines.iter().map(|e| e.name), events));
        Self {
            listener,
            engines,
            timings,
            on_event,
            event_buf: Vec::with_capacity(EVENT_CAPACITY),
            body,
            request: Box::new([0; REQUEST_CAP]),
            last_ask: None,
            scrapes: 0,
            bad_requests: 0,
            stop,
        }
    }

    /// Until [`crate::Exporter::stop`]. One wake per `tick`, and a sleep.
    pub(crate) fn run(mut self) {
        while !self.stop.load(Ordering::Acquire) {
            self.drain_events();
            self.look();
            self.accept_pending();
            std::thread::sleep(self.timings.tick);
        }
    }

    /// Look at every engine's `published()` and take any new snapshot, dated
    /// now. **Every tick, not only on a scrape**: a snapshot another reader
    /// asked for is published whenever that reader asked, and dating it at the
    /// next scrape would report a stale snapshot as fresh (senior review F3,
    /// `a_snapshot_another_reader_asked_for_is_dated_when_it_was_published`).
    /// Dated to within one `tick`. Asks for nothing and allocates nothing.
    fn look(&mut self) {
        let now = Instant::now();
        for e in &mut self.engines {
            e.absorb(now);
        }
    }

    /// Hand every waiting event to the counters and the caller's handler — but
    /// only when the caller handed the stream over. Reading an event takes it,
    /// so an exporter that read uninvited would steal the application's events
    /// (ADR-0170 decision 7).
    fn drain_events(&mut self) {
        let Some(on_event) = self.on_event.as_mut() else {
            return;
        };
        for engine in &mut self.engines {
            self.event_buf.clear();
            engine.observer.events(&mut self.event_buf);
            for event in &self.event_buf {
                engine.count(event);
                on_event(engine.name, event);
            }
        }
        self.event_buf.clear();
    }

    /// Answer the connections waiting in the backlog — at most
    /// [`ANSWERS_PER_WAKE`] of them, and with the event stream drained between
    /// every two, so a flood of connections neither keeps this loop from its
    /// sleep forever nor lets the engine's event ring overflow meanwhile.
    fn accept_pending(&mut self) {
        for _ in 0..ANSWERS_PER_WAKE {
            if self.stop.load(Ordering::Acquire) {
                return;
            }
            match self.listener.accept() {
                Ok((stream, _)) => self.answer(stream),
                // `WouldBlock` is "nobody else is waiting"; anything else is
                // retried on the next tick rather than spun on.
                Err(_) => return,
            }
            self.drain_events();
        }
    }

    fn answer(&mut self, mut s: TcpStream) {
        // **One deadline for the whole connection** — its request and its
        // reply — set here, at accept. Every read and write below is given
        // what is left of it, never a fresh timeout of its own (review F1).
        let deadline = Instant::now() + self.timings.read_timeout;
        // An accepted socket inherits the listener's non-blocking flag on some
        // platforms (the BSDs and macOS), and a non-blocking read here would be
        // a request refused for arriving a microsecond late.
        if s.set_nonblocking(false).is_err() {
            self.bad_requests += 1;
            return;
        }
        let n = match http::read_request(&mut s, &mut self.request, deadline) {
            Received::Complete(n) => n,
            Received::TooLarge => {
                self.refuse(&mut s, Status::BadRequest, deadline);
                return;
            }
            Received::Gone => {
                self.bad_requests += 1;
                return;
            }
        };
        let request = http::parse(self.request.get(..n).unwrap_or(&[]));
        match request {
            Request::Serve {
                route: Route::Metrics,
                head,
            } => {
                self.scrapes += 1;
                self.refresh();
                let exporter = ExporterView {
                    scrapes: self.scrapes,
                    bad_requests: self.bad_requests,
                };
                let result = match encode::encode(&mut self.body, &self.engines, exporter) {
                    Ok(()) => http::respond(&mut s, Status::Ok, true, &self.body, head, deadline),
                    Err(encode::Overflow) => {
                        let st = Status::InternalError;
                        http::respond(&mut s, st, false, st.body(), head, deadline)
                    }
                };
                drop(result);
            }
            Request::Serve {
                route: Route::Healthz,
                head,
            } => {
                self.refresh();
                let healthy = !self.engines.is_empty()
                    && self
                        .engines
                        .iter()
                        .all(|e| e.numbers.is_some_and(|n| n.healthy));
                let st = if healthy {
                    Status::Ok
                } else {
                    Status::Unavailable
                };
                drop(http::respond(&mut s, st, false, st.body(), head, deadline));
            }
            Request::NotFound => self.refuse(&mut s, Status::NotFound, deadline),
            Request::MethodNotAllowed => self.refuse(&mut s, Status::MethodNotAllowed, deadline),
            Request::Bad => self.refuse(&mut s, Status::BadRequest, deadline),
        }
        drop(s.shutdown(Shutdown::Write));
    }

    /// Answer a request this exporter will not serve, count it, and close
    /// without resetting the connection under the answer.
    fn refuse(&mut self, s: &mut TcpStream, st: Status, deadline: Instant) {
        self.bad_requests += 1;
        drop(http::respond(s, st, false, st.body(), false, deadline));
        drop(s.shutdown(Shutdown::Write));
        // Closing with unread bytes makes the kernel send a reset, which can
        // arrive before the answer is read. A short drain lets the answer land;
        // the bytes are thrown away. Bounded twice: four reads of at most
        // 10 ms, and never past the connection's deadline.
        for _ in 0..4 {
            let Some(left) = http::remaining(deadline) else {
                break;
            };
            if s.set_read_timeout(Some(left.min(Duration::from_millis(10))))
                .is_err()
            {
                break;
            }
            match s.read(self.request.as_mut_slice()) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }

    /// Ask for fresh snapshots — at most once per `min_request_interval`,
    /// however many scrapes arrive (ADR-0170 decision 2) — wait for them on
    /// this thread for at most `fresh_wait`, then take whatever is newest.
    fn refresh(&mut self) {
        self.drain_events();
        let now = Instant::now();
        let may_ask = self
            .last_ask
            .is_none_or(|t| now.duration_since(t) >= self.timings.min_request_interval);
        if may_ask {
            self.last_ask = Some(now);
            for e in &mut self.engines {
                e.asked_at = e.observer.published();
                // `ask`, not `request`: raise the flag and copy nothing — the
                // snapshot is read by `latest` once it moves (plan Sửa 1, F5).
                e.observer.ask();
            }
            let deadline = now + self.timings.fresh_wait;
            while self
                .engines
                .iter()
                .any(|e| e.observer.published() == e.asked_at)
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        self.look();
    }
}
