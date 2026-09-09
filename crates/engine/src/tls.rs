//! TLS as a second [`Transport`] implementation, and a name for which mode is
//! actually carrying the bytes.
//!
//! [ADR-0005] decided the shape: the handshake runs in userspace through
//! `rustls`, the steady state on Linux is **kTLS** — the kernel holds the keys
//! and `read(2)`/`write(2)` carry plaintext, so [`crate::transport::Transport`]
//! is unchanged and D8's spin loop does not learn a new trick — and anywhere
//! else it is userspace `rustls`, which **leaves the hot-path guarantee** and
//! says so.
//!
//! [ADR-0018] answered the question that decided whether any of this was
//! possible: `ktls-core` can be driven from a plain non-blocking socket with no
//! async runtime, under four conditions.
//! `scripts/check-ktls-on-a-plain-socket.sh` re-proves them on the kernel of the
//! day — `[measured 2026-09-09]` 15 pass / 0 fail on `7.0.0-31-generic`.
//!
//! # The handshake lives in here, not in a pre-session stage
//!
//! `[decided 2026-09-09, this plan's Sửa 1]` The plan's draft added a
//! `Handshaking` stage to [`crate::presession::PendingSet`]. There is nowhere
//! for it to stand: `PendingSet<T, R, PRE>` holds one transport **type** from
//! `admit` to `take`, so a stage that changes the socket's type cannot be
//! expressed. The shape that does work is simpler and needs no change to
//! `presession.rs` at all — the handshake runs **inside** `recv`/`send`, which
//! report [`Io::Idle`] until it completes, and `PendingSet::turn` already
//! drives a transport that is not ready yet and already carries the
//! `Limits::logon_ms` deadline.
//!
//! [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
//! [ADR-0018]: ../../../docs/decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md
//! [`Io::Idle`]: crate::transport::Io

/// Which of [ADR-0005]'s three answers is carrying this connection's bytes.
///
/// **This type exists because a session that silently falls back to userspace
/// publishes a latency number that is about a different code path** — ADR-0005
/// open question 3. It is reported rather than inferred: `w2w` prints it beside
/// every figure, and a `hft` deployment can refuse anything but
/// [`TlsMode::Kernel`].
///
/// [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlsMode {
    /// No TLS at all — a plain `TcpTransport`. The default, and what every
    /// number published before this module existed was measured on.
    Plain,
    /// The keys are in the kernel: `read(2)` and `write(2)` carry plaintext and
    /// the engine thread does exactly what it does without TLS.
    Kernel,
    /// `rustls` in userspace, on the data path. **This leaves the hot-path
    /// guarantee**: it copies once per direction and allocates. ADR-0005
    /// decision 3 requires it be named rather than discovered.
    Userspace,
}

impl TlsMode {
    /// Whether this mode keeps the no-allocation, no-copy guarantee the `hft`
    /// numbers are measured under.
    ///
    /// [`TlsMode::Userspace`] is the only one that does not, and
    /// `TlsRequireKernel=Y` is how a deployment refuses it at startup rather
    /// than discovering it in a latency histogram.
    #[must_use]
    pub const fn keeps_the_hot_path(self) -> bool {
        matches!(self, Self::Plain | Self::Kernel)
    }
}
