//! A Prometheus exporter for fixbolt engines.
//!
//! An [`Exporter`] holds an [`Observer`] for each engine it watches — never an
//! `Admin`, never the `Handles` — so it can look and cannot change, stop, or
//! speak for an engine ([ADR-0170] decision 1). It runs one thread, named
//! `fixbolt-metrics`, which wakes every `tick`, answers whatever scrapes are
//! waiting, and sleeps. It serves:
//!
//! - `GET`/`HEAD /metrics` — the Prometheus text format 0.0.4, every series in
//!   [`series::ALL`];
//! - `GET`/`HEAD /healthz` — `200` when every engine's `Snapshot::healthy()`,
//!   `503` otherwise.
//!
//! ```no_run
//! use fixbolt_engine::observe::Handles;
//! use fixbolt_metrics::Exporter;
//!
//! let handles = Handles::new();
//! // Spawn the exporter BEFORE the engine thread pins itself to a core: its
//! // thread inherits the affinity of the thread that spawns it.
//! let exporter = Exporter::builder("127.0.0.1:9464".parse().unwrap())
//!     .engine("acceptor", handles.observer())
//!     .spawn()
//!     .unwrap();
//! // … hand `handles` to `fixbolt_engine::serve`, which adopts them …
//! exporter.stop();
//! ```
//!
//! # What it costs the engine
//!
//! A snapshot is built on the engine thread only when asked, and the exporter
//! asks **at most once per `min_request_interval`** (default 100 ms) however
//! many scrapers there are (decision 2). It waits for the answer on its own
//! thread, for at most `fresh_wait` (default 50 ms), and never wakes an engine
//! that is asleep in `poll` — `fixbolt_snapshot_age_seconds` grows instead
//! (decision 3).
//!
//! # What it costs the process
//!
//! **Nothing per scrape.** Every buffer is reserved when the exporter starts,
//! for the worst case — every engine at `MAX_SESSIONS` sessions, every value
//! at its longest — and never grown (decision 4). `benches/alloc.rs` counts
//! allocations on every thread over a window of scrapes and asserts zero.
//!
//! # Events are opt-in
//!
//! Reading an event removes it from the engine's stream. By default the
//! exporter reads none and exports only `fixbolt_events_lost_total`;
//! [`Builder::with_events`] makes it the stream's only reader (decision 7).
//!
//! # Security
//!
//! No TLS, no authentication, no keep-alive. Bind it to loopback or to a
//! private interface.
//!
//! [ADR-0170]: ../../../docs/decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md
#![forbid(unsafe_code)]

use std::fmt;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use fixbolt_engine::observe::{Event, Observer};

mod encode;
mod http;
pub mod series;
mod thread;

/// How often, at most, the exporter asks an engine for a snapshot.
pub const DEFAULT_MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(100);

/// How long a scrape that asked waits for the engine to answer.
pub const DEFAULT_FRESH_WAIT: Duration = Duration::from_millis(50);

/// How long the exporter thread sleeps between wakes.
pub const DEFAULT_TICK: Duration = Duration::from_millis(100);

/// How long a client has to send its request, and to take the answer.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(1);

/// The shortest `tick` and `read_timeout` the builder accepts; shorter is
/// raised to this. A zero `tick` would be a spinning thread.
pub const MIN_DURATION: Duration = Duration::from_millis(1);

/// Why an exporter did not start.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExportError {
    /// The listening socket could not be bound or set up.
    Bind(std::io::Error),
    /// The exporter thread could not be started.
    Spawn(std::io::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind(e) => write!(f, "the metrics listener could not be bound: {e}"),
            Self::Spawn(e) => write!(f, "the metrics thread could not be started: {e}"),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind(e) | Self::Spawn(e) => Some(e),
        }
    }
}

/// How an [`Exporter`] is set up. From [`Exporter::builder`].
pub struct Builder {
    addr: SocketAddr,
    engines: Vec<(&'static str, Observer)>,
    min_request_interval: Duration,
    fresh_wait: Duration,
    tick: Duration,
    read_timeout: Duration,
    on_event: Option<thread::OnEvent>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("addr", &self.addr)
            .field("engines", &self.engines.len())
            .field("min_request_interval", &self.min_request_interval)
            .field("fresh_wait", &self.fresh_wait)
            .field("tick", &self.tick)
            .field("read_timeout", &self.read_timeout)
            .field("with_events", &self.on_event.is_some())
            .finish()
    }
}

impl Builder {
    /// Watch one more engine, exported under the label `engine="<name>"`.
    /// Call once per engine; a sharded deployment is several engines.
    #[must_use]
    pub fn engine(mut self, name: &'static str, observer: Observer) -> Self {
        self.engines.push((name, observer));
        self
    }

    /// Ask an engine for a snapshot at most this often, whatever the scrape
    /// rate. [`DEFAULT_MIN_REQUEST_INTERVAL`].
    #[must_use]
    pub const fn min_request_interval(mut self, d: Duration) -> Self {
        self.min_request_interval = d;
        self
    }

    /// Wait at most this long, on the exporter thread, for an engine that was
    /// asked to publish. [`DEFAULT_FRESH_WAIT`].
    #[must_use]
    pub const fn fresh_wait(mut self, d: Duration) -> Self {
        self.fresh_wait = d;
        self
    }

    /// Sleep this long between wakes. [`DEFAULT_TICK`]; at least
    /// [`MIN_DURATION`].
    #[must_use]
    pub fn tick(mut self, d: Duration) -> Self {
        self.tick = d.max(MIN_DURATION);
        self
    }

    /// Give a client this long, **from the moment it is accepted**, to send its
    /// request and to take the answer: one deadline per connection, not a
    /// timeout per read, so a client that trickles bytes is dropped on time.
    /// [`DEFAULT_READ_TIMEOUT`]; at least [`MIN_DURATION`].
    #[must_use]
    pub fn read_timeout(mut self, d: Duration) -> Self {
        self.read_timeout = d.max(MIN_DURATION);
        self
    }

    /// Make the exporter the **only** reader of every watched engine's event
    /// stream: it counts `fixbolt_events_total{kind}` and
    /// `fixbolt_session_ends_total{reason}`, and hands each event to
    /// `handler`, on the exporter thread, in order — with the `name` its engine
    /// was given at [`Self::engine`], the same string as that engine's `engine`
    /// label. Needed as soon as there are two engines: a `ConnId` starts again
    /// at 0 in each, so an event's `id()` alone cannot say whose it is (plan
    /// Sửa 1, F2).
    ///
    /// Without this the exporter reads no event — reading one removes it, and
    /// the stream belongs to whoever else reads it.
    #[must_use]
    pub fn with_events<F>(mut self, handler: F) -> Self
    where
        F: FnMut(&'static str, &Event) + Send + 'static,
    {
        self.on_event = Some(Box::new(handler));
        self
    }

    /// Bind, reserve every buffer, and start the `fixbolt-metrics` thread.
    ///
    /// The thread inherits the CPU affinity of the thread that calls this:
    /// **call it before the engine thread pins itself**, or the exporter shares
    /// the engine's core.
    ///
    /// # Errors
    ///
    /// [`ExportError::Bind`] when the address cannot be listened on;
    /// [`ExportError::Spawn`] when the thread cannot be started.
    pub fn spawn(self) -> Result<Exporter, ExportError> {
        let listener = TcpListener::bind(self.addr).map_err(ExportError::Bind)?;
        listener.set_nonblocking(true).map_err(ExportError::Bind)?;
        let local_addr = listener.local_addr().map_err(ExportError::Bind)?;
        let events = self.on_event.is_some();
        let engines = self
            .engines
            .into_iter()
            .map(|(name, observer)| thread::Watched::new(name, observer, events))
            .collect();
        let stop = Arc::new(AtomicBool::new(false));
        let timings = thread::Timings {
            min_request_interval: self.min_request_interval,
            fresh_wait: self.fresh_wait,
            tick: self.tick,
            read_timeout: self.read_timeout,
        };
        let lp = thread::Loop::new(listener, engines, timings, self.on_event, Arc::clone(&stop));
        let handle = std::thread::Builder::new()
            .name("fixbolt-metrics".into())
            .spawn(move || lp.run())
            .map_err(ExportError::Spawn)?;
        Ok(Exporter {
            local_addr,
            stop,
            thread: Some(handle),
        })
    }
}

/// A running exporter. Stops, and joins its thread, on [`Self::stop`] or on
/// drop.
#[derive(Debug)]
pub struct Exporter {
    local_addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Exporter {
    /// Start describing an exporter that will listen on `addr`. Port 0 picks a
    /// free one; [`Self::local_addr`] says which.
    #[must_use]
    pub const fn builder(addr: SocketAddr) -> Builder {
        Builder {
            addr,
            engines: Vec::new(),
            min_request_interval: DEFAULT_MIN_REQUEST_INTERVAL,
            fresh_wait: DEFAULT_FRESH_WAIT,
            tick: DEFAULT_TICK,
            read_timeout: DEFAULT_READ_TIMEOUT,
            on_event: None,
        }
    }

    /// Where it is listening.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop the thread and wait for it: at most one `tick` when it is asleep,
    /// or, when it is answering a connection, that connection's `read_timeout`
    /// (its deadline, counted from accept) plus one `fresh_wait`. No further
    /// connection is taken once stop is asked. The listener is closed when this
    /// returns.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            // A panic on the exporter thread has nothing left to report to.
            drop(t.join());
        }
    }
}

impl Drop for Exporter {
    fn drop(&mut self) {
        self.halt();
    }
}
