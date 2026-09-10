//! TLS as a second [`crate::transport::Transport`] implementation, and a name for which mode is
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
/// `[2026-09-10]` **re-exported; it lives in [`crate::transport`] now.**
/// [ADR-0060] decision 3 puts `Transport::tls_mode` on the core trait with a
/// default so the engine can ask any transport without a downcast — and a type
/// named in that trait cannot sit behind the `tls` feature, because the trait
/// does not. The move is that decision's cost and ADR-0060's Consequences say
/// so.
///
/// [ADR-0005]: ../../../docs/decisions/ADR-0005-tls.md
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
pub use crate::transport::TlsMode;

#[cfg(target_os = "linux")]
mod handshake {
    //! A TLS handshake driven **without blocking**, from inside `recv`/`send`.
    //!
    //! The spike (`spikes/ktls`) drives the same rustls state machine with a
    //! `spin_write_all` / `spin_read` pair, which is correct for a program whose
    //! only job is one connection. It is wrong here: this runs on the acceptor
    //! thread inside `PendingSet::turn`, which sweeps every waiting socket, and
    //! a spin on one of them stalls all the others behind it for a whole
    //! handshake.
    //!
    //! So the loop is turned inside out. Each call pumps the state machine as
    //! far as the socket allows and then returns, keeping every buffer and
    //! cursor, and reports [`Step::Pending`]. `PendingSet::turn` calls again on
    //! the next sweep. Nothing here waits.

    use std::io;

    use rustls::server::UnbufferedServerConnection;
    use rustls::unbuffered::{ConnectionState, UnbufferedStatus};

    use crate::transport::{Io, TcpTransport, Transport};

    /// 32 KiB each way, taken once when the socket is admitted.
    ///
    /// A handshake record can reach 16 KiB and a flight can hold several, so
    /// this is two records of headroom rather than a guess. **These are the
    /// allocation ADR-0005 decision 1 carves out**, and the carve-out is
    /// bounded to exactly here: they are dropped at the handover and nothing
    /// after it allocates.
    const HANDSHAKE_BUF: usize = 32 * 1024;

    /// How far the handshake got on this call.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Step {
        /// Not finished, and the socket has nothing more to give right now.
        /// Call again on the next sweep.
        Pending,
        /// The handshake is complete and no TLS data is left queued, which is
        /// what `dangerous_into_kernel_connection` requires.
        Done,
        /// It will not finish. The connection is over.
        Failed(io::ErrorKind),
    }

    /// The acceptor side of a handshake in progress.
    pub struct Handshake {
        conn: UnbufferedServerConnection,
        incoming: Vec<u8>,
        used: usize,
        outgoing: Vec<u8>,
        out_used: usize,
        out_sent: usize,
        early: Vec<u8>,
    }

    impl Handshake {
        /// Begin, with buffers taken now rather than during the handshake.
        pub fn new(conn: UnbufferedServerConnection) -> Self {
            Self {
                conn,
                incoming: vec![0u8; HANDSHAKE_BUF],
                used: 0,
                outgoing: vec![0u8; HANDSHAKE_BUF],
                out_used: 0,
                out_sent: 0,
                early: Vec::new(),
            }
        }

        /// Application plaintext that arrived **before** the handover.
        ///
        /// `[measured 2026-09-09]` this is not a corner case, it is the normal
        /// case, and the plan named it as a trap before the code was written: a
        /// counterparty that sends its `Logon` immediately after `Finished`
        /// puts application records on the wire while this end is still
        /// finishing. rustls reports `ReadTraffic` rather than `WriteTraffic`,
        /// and the very first run of
        /// `tests/tls.rs::a_handshake_completes_without_the_acceptor_ever_blocking`
        /// hit it.
        ///
        /// **These bytes must be given to the session before a single byte is
        /// read from the kernel**, or the first message of the connection is
        /// lost — and lost silently, since a FIX session with no `Logon` simply
        /// times out.
        ///
        /// Letting rustls decrypt them is what keeps the handover sound.
        /// ADR-0018's third condition is about **ciphertext** left in this
        /// engine's buffer, which the kernel would then re-count — see
        /// [`Handshake::leftover`]. Records rustls has properly consumed have
        /// advanced its own sequence number, and that is the sequence
        /// `dangerous_into_kernel_connection` hands down. The spike's
        /// `hand-draining-desyncs-the-kernel` check fails the *other* thing:
        /// bytes taken off the socket behind rustls's back.
        pub fn take_early_data(&mut self) -> Vec<u8> {
            core::mem::take(&mut self.early)
        }

        /// How many ciphertext bytes are sitting in this buffer unprocessed.
        ///
        /// **Must be zero at the handover.** Bytes here are ciphertext the
        /// kernel will never see, and its receive sequence number has already
        /// counted them — ADR-0018's third condition, and the spike's
        /// `hand-draining-desyncs-the-kernel` check is what happens when it is
        /// not: the next record fails with `EBADMSG`.
        pub const fn leftover(&self) -> usize {
            self.used
        }

        /// Give up the connection once [`Step::Done`] has been reported.
        pub fn into_connection(self) -> UnbufferedServerConnection {
            self.conn
        }

        /// Carry on in userspace, keeping the buffers rather than taking new
        /// ones.
        ///
        /// The fallback of ADR-0005 decision 3. Reached only when the kernel
        /// refused the offload, and **the buffers are the point**: this mode
        /// copies once per direction for the life of the session, so allocating
        /// per message on top of that would be a second cost on a path that has
        /// already left the guarantee.
        pub fn into_traffic(self) -> Traffic {
            Traffic {
                conn: self.conn,
                incoming: self.incoming,
                used: self.used,
                outgoing: self.outgoing,
                out_used: self.out_used,
                out_sent: self.out_sent,
                plain: self.early,
                plain_at: 0,
            }
        }

        /// Push whatever is queued towards the socket. Partial writes are the
        /// normal case on a non-blocking socket and are simply remembered.
        fn flush(&mut self, sock: &mut TcpTransport) -> Result<bool, io::ErrorKind> {
            while self.out_sent < self.out_used {
                let Some(pending) = self.outgoing.get(self.out_sent..self.out_used) else {
                    return Err(io::ErrorKind::InvalidData);
                };
                match sock.send(pending) {
                    Io::Ready(n) => self.out_sent += n,
                    Io::Idle => return Ok(false),
                    Io::Closed => return Err(io::ErrorKind::UnexpectedEof),
                    Io::Failed(k) => return Err(k),
                }
            }
            self.out_used = 0;
            self.out_sent = 0;
            Ok(true)
        }

        /// Drive the handshake as far as this socket allows, then return.
        ///
        /// Never blocks and never spins: every path out of the loop is either a
        /// state change or a socket that said `Idle`.
        pub fn pump(&mut self, sock: &mut TcpTransport) -> Step {
            loop {
                // Anything still queued goes first. Until it is gone the peer
                // cannot make progress, so there is no point reading.
                match self.flush(sock) {
                    Ok(true) => {}
                    Ok(false) => return Step::Pending,
                    Err(k) => return Step::Failed(k),
                }

                let Some(input) = self.incoming.get_mut(..self.used) else {
                    return Step::Failed(io::ErrorKind::InvalidData);
                };
                let UnbufferedStatus { discard, state } = self.conn.process_tls_records(input);

                let mut want_read = false;
                let mut done = false;

                match state {
                    Ok(ConnectionState::EncodeTlsData(mut s)) => {
                        let Some(room) = self.outgoing.get_mut(self.out_used..) else {
                            return Step::Failed(io::ErrorKind::InvalidData);
                        };
                        match s.encode(room) {
                            Ok(n) => self.out_used += n,
                            // A flight that does not fit in 32 KiB is not a
                            // handshake this engine is going to complete.
                            Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                        }
                    }
                    Ok(ConnectionState::TransmitTlsData(s)) => s.done(),
                    Ok(ConnectionState::BlockedHandshake) => want_read = true,
                    Ok(ConnectionState::WriteTraffic(_)) => done = true,
                    // The handshake is over and the peer has already sent
                    // application data. Let rustls decrypt it — see
                    // `take_early_data` for why that is the sound choice rather
                    // than the convenient one — and keep going until the
                    // ciphertext buffer is empty.
                    Ok(ConnectionState::ReadTraffic(mut r)) => {
                        while let Some(record) = r.next_record() {
                            match record {
                                Ok(rec) => self.early.extend_from_slice(rec.payload),
                                Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                            }
                        }
                    }
                    Ok(_) | Err(_) => return Step::Failed(io::ErrorKind::InvalidData),
                }

                if discard > 0 {
                    if discard > self.used {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    }
                    self.incoming.copy_within(discard..self.used, 0);
                    self.used -= discard;
                }

                // **`discard == 0` is the second half of the condition and not
                // a tidy-up.** A TLS 1.3 server emits session tickets after it
                // reaches `WriteTraffic`, and
                // `dangerous_into_kernel_connection` refuses a connection with
                // TLS data still queued. Leaving on the first `WriteTraffic`
                // hands the kernel a connection that still owes bytes.
                if done && discard == 0 && self.out_used == 0 {
                    return Step::Done;
                }

                if want_read {
                    let Some(room) = self.incoming.get_mut(self.used..) else {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    };
                    if room.is_empty() {
                        return Step::Failed(io::ErrorKind::InvalidData);
                    }
                    match sock.recv(room) {
                        Io::Ready(n) => self.used += n,
                        Io::Idle => return Step::Pending,
                        Io::Closed => return Step::Failed(io::ErrorKind::UnexpectedEof),
                        Io::Failed(k) => return Step::Failed(k),
                    }
                }
            }
        }
    }

    /// A TLS connection carrying application data **in userspace**.
    ///
    /// **This is the mode that leaves the hot-path guarantee**, and it exists so
    /// that a kernel without kTLS is a slower session rather than no session.
    /// Every byte is copied once on the way in and once on the way out;
    /// `benches/alloc.rs` does not cover this path and ADR-0005 decision 3 says
    /// it must be named rather than discovered.
    pub struct Traffic {
        conn: UnbufferedServerConnection,
        incoming: Vec<u8>,
        used: usize,
        outgoing: Vec<u8>,
        out_used: usize,
        out_sent: usize,
        plain: Vec<u8>,
        plain_at: usize,
    }

    impl Traffic {
        /// Take the bytes rustls decrypted during the handshake.
        ///
        /// They must be delivered before anything read afterwards: a `Logon`
        /// out of order is a sequence gap the counterparty did not cause.
        pub fn adopt_early(&mut self, early: Vec<u8>) {
            if self.plain.is_empty() {
                self.plain = early;
                self.plain_at = 0;
            } else {
                self.plain.extend_from_slice(&early);
            }
        }

        /// Move whatever is queued towards the socket, tolerating short writes.
        fn flush(&mut self, sock: &mut TcpTransport) -> Result<bool, io::ErrorKind> {
            while self.out_sent < self.out_used {
                let Some(pending) = self.outgoing.get(self.out_sent..self.out_used) else {
                    return Err(io::ErrorKind::InvalidData);
                };
                match sock.send(pending) {
                    Io::Ready(n) => self.out_sent += n,
                    Io::Idle => return Ok(false),
                    Io::Closed => return Err(io::ErrorKind::UnexpectedEof),
                    Io::Failed(k) => return Err(k),
                }
            }
            self.out_used = 0;
            self.out_sent = 0;
            Ok(true)
        }

        /// Hand the session decrypted bytes, reading and decrypting only when
        /// what is already decrypted has run out.
        pub fn recv(&mut self, sock: &mut TcpTransport, buf: &mut [u8]) -> Io {
            loop {
                if let Some(rest) = self.plain.get(self.plain_at..)
                    && !rest.is_empty()
                {
                    let n = rest.len().min(buf.len());
                    let (Some(src), Some(dst)) = (rest.get(..n), buf.get_mut(..n)) else {
                        return Io::Failed(io::ErrorKind::InvalidData);
                    };
                    dst.copy_from_slice(src);
                    self.plain_at += n;
                    return Io::Ready(n);
                }
                self.plain.clear();
                self.plain_at = 0;

                // Decrypt whatever ciphertext is already in hand before asking
                // the socket for more.
                let Some(input) = self.incoming.get_mut(..self.used) else {
                    return Io::Failed(io::ErrorKind::InvalidData);
                };
                let UnbufferedStatus { discard, state } = self.conn.process_tls_records(input);
                let mut got = false;
                match state {
                    Ok(ConnectionState::ReadTraffic(mut r)) => {
                        while let Some(record) = r.next_record() {
                            match record {
                                Ok(rec) => {
                                    self.plain.extend_from_slice(rec.payload);
                                    got = true;
                                }
                                Err(_) => return Io::Failed(io::ErrorKind::InvalidData),
                            }
                        }
                    }
                    Ok(ConnectionState::WriteTraffic(_) | ConnectionState::BlockedHandshake) => {}
                    Ok(ConnectionState::Closed) => return Io::Closed,
                    Ok(_) | Err(_) => return Io::Failed(io::ErrorKind::InvalidData),
                }
                if discard > 0 {
                    if discard > self.used {
                        return Io::Failed(io::ErrorKind::InvalidData);
                    }
                    self.incoming.copy_within(discard..self.used, 0);
                    self.used -= discard;
                }
                if got {
                    continue;
                }

                let Some(room) = self.incoming.get_mut(self.used..) else {
                    return Io::Failed(io::ErrorKind::InvalidData);
                };
                if room.is_empty() {
                    // A record larger than the buffer cannot be assembled, and
                    // waiting for room that will never appear is a hang.
                    return Io::Failed(io::ErrorKind::InvalidData);
                }
                match sock.recv(room) {
                    Io::Ready(n) => self.used += n,
                    other => return other,
                }
            }
        }

        /// Encrypt and send. A short write is remembered, not lost.
        pub fn send(&mut self, sock: &mut TcpTransport, buf: &[u8]) -> Io {
            match self.flush(sock) {
                Ok(true) => {}
                // Still draining the last message: the caller keeps its bytes,
                // which is exactly `DESIGN.md` D10's backpressure contract.
                Ok(false) => return Io::Idle,
                Err(k) => return Io::Failed(k),
            }
            if buf.is_empty() {
                return Io::Idle;
            }
            let UnbufferedStatus { state, .. } = self.conn.process_tls_records(&mut []);
            let Ok(ConnectionState::WriteTraffic(mut w)) = state else {
                return Io::Idle;
            };
            let Ok(n) = w.encrypt(buf, &mut self.outgoing) else {
                // Not enough room for this message plus its overhead. The
                // caller may offer less; refusing is right, losing is not.
                return Io::Idle;
            };
            self.out_used = n;
            self.out_sent = 0;
            match self.flush(sock) {
                Ok(_) => Io::Ready(buf.len()),
                Err(k) => Io::Failed(k),
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use handshake::{Handshake, Step, Traffic};

/// Which parts of the kernel handover a serving loop really performs.
///
/// `[2026-09-10]` **A test seam whose shape is a finding rather than a
/// convenience.** [ADR-0060] decision 1 has two halves — a startup probe and a
/// per-connection check — and the first attempt gave them **one boolean between
/// them**. That could not express the case the second half exists for: *the
/// kernel is capable and this handshake still fell back*. Setting the flag to
/// "no kernel" made the startup probe refuse, so the serving loop never bound
/// and the per-connection half was unreachable from any test —
/// `[measured 2026-09-10]` `the serving loop never bound the address`.
///
/// One flag for two independent facts is `STATUS.md` item 54's shape, where
/// three socket endings shared one word. Three named situations cost the same
/// single parameter and say exactly which pretence is in force.
///
/// **A deployment always passes [`TlsProbe::Real`].**
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsProbe {
    /// Ask the kernel, offload for real. **What a deployment passes.**
    Real,
    /// The startup probe reports that this kernel has no TLS ULP.
    ///
    /// Reaches ADR-0060 decision 1's **first** half on a desk whose kernel does
    /// offload — which is every desk this has run on, so without it that
    /// refusal would be code that compiles and has never run.
    PretendKernelCannotOffload,
    /// The startup probe is real; each handshake then lands in userspace.
    ///
    /// Reaches the **second** half: the suite-mismatch case, where the host is
    /// fine and this particular connection is not. **This is the arm a single
    /// boolean could not express.**
    PretendHandshakeFallsBack,
}

/// Can **this kernel** offload TLS at all?
///
/// `[2026-09-10]` **[ADR-0060] decision 1, the startup half.** One `socket`, one
/// `connect`, one `setsockopt`, once, before the listener opens — so a host
/// built without `CONFIG_TLS` is found before a counterparty is affected, rather
/// than through a dropped session that had nothing to do with that counterparty.
///
/// **The socket must be connected, and that is measured rather than assumed.**
/// `scripts/check-ktls-available.sh` learned it first: `TCP_ULP` on an
/// unconnected socket fails for a *different* reason, so a probe that skipped
/// the `connect` would report a refusal on a kernel that offloads perfectly
/// well. A probe wrong in the pessimistic direction is still wrong, and under
/// `TlsRequireKernel=Y` it would refuse to start a correct deployment.
///
/// **What it does not answer:** whether the suite *your counterparty* picks can
/// be offloaded. A capable kernel still lands a mismatched suite in userspace,
/// which is why ADR-0060 decision 1 has a second half.
///
/// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
#[must_use]
pub fn kernel_can_offload() -> bool {
    let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:0") else {
        return false;
    };
    let Ok(addr) = listener.local_addr() else {
        return false;
    };
    let Ok(client) = std::net::TcpStream::connect(addr) else {
        return false;
    };
    let Ok((server, _)) = listener.accept() else {
        return false;
    };
    let answer = ktls_core::setup_ulp(&client).is_ok();
    drop(server);
    answer
}

/// A `rustls::ServerConfig` this engine can actually hand to the kernel.
///
/// `[2026-09-10]` **step 4a of the `tls` plan.** [`crate::serve_tls`] takes a
/// certificate and a key rather than a finished `ServerConfig` and builds it
/// here, because two of the settings are load-bearing and neither is
/// discoverable:
///
/// 1. **`enable_secret_extraction = true`.** Without it the connection is
///    **dropped**, and it is worth being exact about why, because the obvious
///    guess is wrong. It does not quietly serve from userspace `rustls`: the
///    fallback is reachable only from a `setup_ulp` refusal, which is asked
///    while the rustls connection is still alive.
///    `dangerous_into_kernel_connection` **consumes** that connection, so a
///    refusal there leaves nothing to fall back *to* — the comment on
///    `TlsTransport::hand_over` says so, and it is the reason the two questions
///    are asked in that order. `[measured 2026-09-10]` flipping this flag to
///    `false` and running
///    `crates/engine/tests/tls_wire.rs::serve_tls_brings_a_session_up_through_tls`
///    reads `ConnectionReset` at the counterparty, with no FIX-level
///    explanation because there is no session yet to carry one.
/// 2. **TLS 1.3, `AES-128-GCM` only.** kTLS carries far fewer suites than
///    `rustls` will negotiate, so leaving the ends to agree freely means the
///    kernel offload succeeds or fails depending on what the counterparty
///    offered. `spikes/ktls` made the same narrowing for the same reason.
///
/// **This is a deliberate narrowing and it is a cost, not only a safeguard.** A
/// counterparty that cannot do `TLS13_AES_128_GCM_SHA256` cannot connect to this
/// acceptor at all. ADR-0005 open question 2 — which kernel and which suites are
/// the floor — is answered by measurement in step 6, and this is the answer
/// standing in until then.
///
/// # Errors
///
/// [`crate::ServeError::Tls`] if the provider has no TLS 1.3, or the certificate
/// and key do not match.
#[cfg(all(feature = "tls", target_os = "linux"))]
pub fn server_config(
    certs: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: rustls::pki_types::PrivateKeyDer<'static>,
) -> Result<std::sync::Arc<rustls::ServerConfig>, crate::ServeError> {
    let mut provider = rustls::crypto::ring::default_provider();
    provider
        .cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    let mut cfg = rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| crate::ServeError::Tls(format!("{e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| crate::ServeError::Tls(format!("{e}")))?;
    cfg.enable_secret_extraction = true;
    Ok(std::sync::Arc::new(cfg))
}

#[cfg(target_os = "linux")]
mod transport_impl {
    //! [`TlsTransport`]: a socket that handshakes, hands its keys to the kernel,
    //! and then is an ordinary socket again.
    //!
    //! **The whole design is in that last clause.** After the handover the
    //! kernel holds the keys, `read(2)` returns plaintext and `write(2)` takes
    //! it, so [`Transport::recv`] and [`Transport::send`] are the same two
    //! syscalls the plain transport makes. D8's spin loop learns nothing new,
    //! the parser still works in place, and `benches/alloc.rs` still reads zero
    //! — none of which is true of the userspace fallback, which is why
    //! [`TlsMode`] exists and is reported rather than inferred.

    use std::io;

    use ktls_core::Context;
    use rustls::kernel::KernelConnection;
    use rustls::server::ServerConnectionData;

    use super::{Handshake, Step, TlsMode, Traffic};
    use crate::transport::{Io, Source, TcpTransport, Transport};

    /// What the socket is doing right now.
    enum Stage {
        /// Still negotiating. `recv`/`send` pump it and report [`Io::Idle`].
        Handshaking(Box<Handshake>),
        /// The kernel holds the keys. Ordinary reads and writes, plus the one
        /// error path the offload adds.
        Kernel(Box<Context<KernelConnection<ServerConnectionData>>>),
        /// The kernel would not take the offload. **This is the mode that
        /// leaves the hot-path guarantee** — ADR-0005 decision 3 — and it is
        /// named rather than silently entered: `mode()` says so,
        /// [`TlsTransport::fell_back`] says so, and `TlsRequireKernel=Y` will
        /// refuse it outright.
        Userspace(Box<Traffic>),
        /// It ended, and the reason is kept so the engine reports it once.
        Broken(io::ErrorKind),
    }

    /// A TLS acceptor socket.
    pub struct TlsTransport {
        sock: TcpTransport,
        stage: Stage,
        /// Application bytes rustls decrypted before the handover, waiting to be
        /// handed to the session ahead of anything the kernel produces.
        early: Vec<u8>,
        early_at: usize,
        /// Whether this connection asked the kernel and was refused. Read once,
        /// by the engine, to raise the event.
        fell_back: bool,
        /// Whether to ask the kernel at all. Always `true` in a deployment; see
        /// [`TlsTransport::with_offload`].
        offload: bool,
    }

    impl TlsTransport {
        /// Take a freshly accepted socket into a handshake.
        #[must_use]
        pub fn new(sock: TcpTransport, handshake: Handshake) -> Self {
            Self::with_offload(sock, handshake, true)
        }

        /// The same, with the kernel offload disabled.
        ///
        /// **This exists so the userspace fallback can be tested at all.** That
        /// path is reached only when the kernel refuses `setup_ulp`, and this
        /// desk's kernel does not refuse — so without a way to ask for the
        /// refusal, the fallback would be code that compiles and has never run,
        /// which is what `CLAUDE.md` §10 calls a promise rather than evidence.
        /// It is not a deployment knob: `TlsRequireKernel` (step 4's settings
        /// half) is the operator-facing control and it points the other way.
        #[must_use]
        pub fn with_offload(sock: TcpTransport, handshake: Handshake, offload: bool) -> Self {
            Self {
                sock,
                offload,
                stage: Stage::Handshaking(Box::new(handshake)),
                early: Vec::new(),
                early_at: 0,
                fell_back: false,
            }
        }

        /// Whether this connection fell back to userspace.
        ///
        /// **The engine must read this and say so**: a deployment that believes
        /// it is on kTLS and is not has a published latency figure describing a
        /// different code path, which is the whole of ADR-0005 open question 3.
        #[must_use]
        pub const fn fell_back(&self) -> bool {
            self.fell_back
        }

        /// Which of ADR-0005's answers is carrying the bytes.
        ///
        /// [`TlsMode::Plain`] is never returned by this type — it is what a
        /// `TcpTransport` is — and a handshake still in flight reports
        /// `Userspace`, because that is what is true while `rustls` is doing
        /// the work. **Read it after the session is up**, which is the only
        /// point at which the answer is stable and the only point a published
        /// latency figure is about.
        #[must_use]
        pub const fn mode(&self) -> TlsMode {
            match self.stage {
                Stage::Kernel(_) => TlsMode::Kernel,
                Stage::Handshaking(_) | Stage::Broken(_) | Stage::Userspace(_) => {
                    TlsMode::Userspace
                }
            }
        }

        /// Whether the handshake is finished and the keys are in the kernel.
        #[must_use]
        pub const fn is_ready(&self) -> bool {
            matches!(self.stage, Stage::Kernel(_))
        }

        /// Drive the handshake, and hand the keys over the moment it completes.
        ///
        /// Called from both `recv` and `send`, because either can be the first
        /// thing the engine asks of a socket.
        fn advance(&mut self) -> Result<(), io::ErrorKind> {
            let Stage::Handshaking(hs) = &mut self.stage else {
                return match self.stage {
                    Stage::Broken(k) => Err(k),
                    _ => Ok(()),
                };
            };
            match hs.pump(&mut self.sock) {
                Step::Pending => return Ok(()),
                Step::Failed(k) => {
                    self.stage = Stage::Broken(k);
                    return Err(k);
                }
                Step::Done => {}
            }

            // ADR-0018's third condition, asserted rather than assumed. Bytes
            // here are ciphertext the kernel will never see and whose place in
            // the sequence it has already counted, so handing over now
            // desynchronises the receive side and the next record fails with
            // `EBADMSG` — the spike's `hand-draining-desyncs-the-kernel`.
            if hs.leftover() != 0 {
                self.stage = Stage::Broken(io::ErrorKind::InvalidData);
                return Err(io::ErrorKind::InvalidData);
            }
            self.early = hs.take_early_data();
            self.early_at = 0;

            // **`setup_ulp` FIRST, and the order is forced rather than
            // stylistic.** `dangerous_into_kernel_connection` **consumes** the
            // rustls connection, and the fallback ADR-0005 decision 3 requires
            // needs that connection alive. Asking the kernel to attach its TLS
            // ULP costs nothing, needs no keys, and is the step that fails on a
            // kernel without the `tls` module — so it is the question to ask
            // while the answer can still be acted on. Converting first and
            // discovering the refusal afterwards leaves nothing to fall back
            // *to*, which is why the previous commit could only end the
            // connection.
            if !self.offload || ktls_core::setup_ulp(self.sock.socket()).is_err() {
                let Stage::Handshaking(hs) =
                    core::mem::replace(&mut self.stage, Stage::Broken(io::ErrorKind::InvalidData))
                else {
                    return Err(io::ErrorKind::InvalidData);
                };
                // The early bytes go into the traffic driver rather than
                // `self.early`, so one queue orders them and the userspace path
                // cannot deliver them twice.
                let mut traffic = hs.into_traffic();
                traffic.adopt_early(core::mem::take(&mut self.early));
                self.early_at = 0;
                self.fell_back = true;
                self.stage = Stage::Userspace(Box::new(traffic));
                return Ok(());
            }

            let Stage::Handshaking(hs) =
                core::mem::replace(&mut self.stage, Stage::Broken(io::ErrorKind::InvalidData))
            else {
                return Err(io::ErrorKind::InvalidData);
            };
            let Ok((secrets, kconn)) = hs.into_connection().dangerous_into_kernel_connection()
            else {
                return Err(io::ErrorKind::InvalidData);
            };

            let version = kconn.protocol_version();
            if super::push_keys(self.sock.socket(), secrets, version).is_err() {
                // The ULP attached and the keys did not go down — a cipher
                // suite this kernel does not carry (ADR-0005 open question 2).
                // The connection is unusable in either mode now: rustls has
                // been consumed and the socket has a ULP with no keys.
                self.stage = Stage::Broken(io::ErrorKind::Unsupported);
                return Err(io::ErrorKind::Unsupported);
            }
            self.stage = Stage::Kernel(Box::new(Context::new(kconn, None)));
            Ok(())
        }

        /// Serve from the pre-handover bytes first. Returns 0 when they are
        /// spent, which is the only time the kernel is asked.
        fn drain_early(&mut self, buf: &mut [u8]) -> usize {
            let Some(rest) = self.early.get(self.early_at..) else {
                return 0;
            };
            if rest.is_empty() {
                if !self.early.is_empty() {
                    self.early = Vec::new();
                    self.early_at = 0;
                }
                return 0;
            }
            let n = rest.len().min(buf.len());
            let (Some(src), Some(dst)) = (rest.get(..n), buf.get_mut(..n)) else {
                return 0;
            };
            dst.copy_from_slice(src);
            self.early_at += n;
            n
        }
    }

    impl Transport for TlsTransport {
        /// [ADR-0060] decision 3: the same answer as the inherent
        /// [`TlsTransport::mode`], reachable through the trait so a generic engine
        /// can ask it without a downcast.
        ///
        /// [ADR-0060]: ../../../docs/decisions/ADR-0060-a-deployment-that-requires-the-kernel-is-refused-twice.md
        fn tls_mode(&self) -> TlsMode {
            self.mode()
        }

        const POLLABLE: bool = cfg!(unix);

        fn source(&self) -> Option<Source> {
            self.sock.source()
        }

        fn recv(&mut self, buf: &mut [u8]) -> Io {
            // **Before the socket, always.** A `Logon` that arrived during the
            // handshake is already decrypted and in hand; reading the kernel
            // first would deliver it out of order, which on a FIX session is a
            // sequence gap the counterparty did not cause.
            let n = self.drain_early(buf);
            if n > 0 {
                return Io::Ready(n);
            }
            if let Err(k) = self.advance() {
                return Io::Failed(k);
            }
            // The userspace fallback carries its own buffers and its own
            // ordering; `self.early` was handed to it at the fork.
            if let Stage::Userspace(t) = &mut self.stage {
                return t.recv(&mut self.sock, buf);
            }
            let Stage::Kernel(ctx) = &mut self.stage else {
                return Io::Idle;
            };
            // **Read the socket here rather than through `TcpTransport::recv`,
            // and the reason is a hole in `Io` rather than a preference.**
            // `Io::Failed` carries an `io::ErrorKind`, and the error this path
            // must recognise is `EIO` — which has no stable `ErrorKind` at all
            // (`ErrorKind::Uncategorized` is unstable to name). Going through
            // the plain transport would throw away the one piece of information
            // that distinguishes a recoverable control record from a dead
            // socket.
            //
            // ADR-0018's first condition: **every** read error goes through
            // `Context::handle_io_error`. A TLS 1.3 session ticket arrives as a
            // control record the kernel will not decode, surfaces as `EIO`, and
            // an engine that treated it as a failure would drop a session that
            // was perfectly healthy — `[measured 2026-08-31]` the spike saw one
            // ticket and one recovery on every client run.
            match io::Read::read(&mut self.sock.socket(), buf) {
                Ok(0) => Io::Closed,
                Ok(n) => Io::Ready(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Io::Idle,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => Io::Idle,
                Err(e) => {
                    if ctx.handle_io_error(self.sock.socket(), e).is_ok() {
                        Io::Idle
                    } else {
                        Io::Failed(io::ErrorKind::InvalidData)
                    }
                }
            }
        }

        fn send(&mut self, buf: &[u8]) -> Io {
            if let Err(k) = self.advance() {
                return Io::Failed(k);
            }
            // `stage` and `sock` are different fields, so both can be borrowed
            // mutably at once — which is why `Traffic` takes the socket as an
            // argument and does not own one.
            if let Stage::Userspace(t) = &mut self.stage {
                return t.send(&mut self.sock, buf);
            }
            if !matches!(self.stage, Stage::Kernel(_)) {
                // Nothing may go out before the keys are in the kernel: it
                // would leave as plaintext on a socket the peer is reading as
                // TLS.
                return Io::Idle;
            }
            self.sock.send(buf)
        }
    }
}

#[cfg(target_os = "linux")]
pub use transport_impl::TlsTransport;

/// Hand the negotiated keys to the kernel, both directions.
///
/// Split out so the one `unsafe`-adjacent step — three `setsockopt` calls
/// behind `ktls-core` — sits in one place with one name.
///
/// # Errors
///
/// Any of `setup_ulp` or the two `set` calls. A kernel without the `tls` module
/// fails at `setup_ulp`, which is the case ADR-0005 decision 3's fallback is
/// for.
#[cfg(target_os = "linux")]
fn push_keys<S: std::os::fd::AsFd>(
    sock: &S,
    secrets: rustls::ExtractedSecrets,
    version: rustls::ProtocolVersion,
) -> Result<(), ktls_core::Error> {
    let secrets = ktls_core::ExtractedSecrets::try_from(secrets)?;
    let version = ktls_core::ProtocolVersion::from(version);
    ktls_core::TlsCryptoInfoTx::new(version, secrets.tx.1, secrets.tx.0)?.set(sock)?;
    ktls_core::TlsCryptoInfoRx::new(version, secrets.rx.1, secrets.rx.0)?.set(sock)?;
    Ok(())
}
