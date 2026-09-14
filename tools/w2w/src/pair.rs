//! Pairing a request's hardware RX stamp with its reply's hardware TX stamp —
//! step A3b of `docs/plans/2026-09-04-the-second-linux-desk.md`.
//!
//! **Pure**: no socket, no clock, no allocation but the caller's `Vec`. What
//! `mod wire` in `main.rs` records, this turns into wire-in → wire-out figures
//! at the acceptor, and into the two counts that say how far to trust them.
//!
//! # What goes in
//!
//! * [`Inbound`] — one IPv4/TCP frame the AF_PACKET tap saw arriving on the
//!   engine's port, with the `scm_timestamping` triple the kernel attached.
//! * [`Sent`] — one entry from the engine socket's error queue: the triple and
//!   the `SOF_TIMESTAMPING_OPT_ID | OPT_ID_TCP` key, which is the 0-based byte
//!   offset of the **last byte** of one `send` on the engine's stream, counted
//!   from the `write_seq` at `setsockopt` time (`net/ipv4/tcp.c`
//!   `tcp_tx_timestamp`: `tskey = seq + len - 1`; `net/core/skbuff.c`:
//!   `ee_data = tskey - sk_tskey`; `net/core/sock.c`: `sk_tskey = write_seq`).
//!   The option is set before the engine owns the socket, so offset 0 is the
//!   engine's first byte.
//!
//! # How a reply is found — by the TCP stream, not by counting
//!
//! One request in flight (back-to-back): the client sends request `k+1` only
//! after it has read reply `k` whole, so the `ack` on request `k+1` is the
//! engine's byte count through reply `k`. Relative to the handshake's `ack`
//! that is [`Request::acked`]. Reply `k` is therefore the error-queue entry
//! whose key lies in `[acked(k), acked(k+1))` — the **last** such entry, which
//! is the last byte of the last write before the client answered. The final
//! request has no successor and takes the **first** entry at or past its
//! `acked`.
//!
//! This is "paired in order" as the plan says, with the order read off the
//! byte stream rather than off a count. The difference matters exactly when a
//! stamp is lost: the plan's `igb` trap (one pending TX stamp; a second is
//! dropped and **nothing** reaches the error queue) would shift every later
//! pair by one under a count, and here it leaves one request with no entry in
//! its range — [`Tally::tx_missing`] — and every other pair where it was.
//!
//! # What is never done
//!
//! **A software stamp never enters the hardware column.** [`hw`] is the only
//! reader of a triple and it reads `ts[2]` alone; `ts[0]` (software) and
//! `ts[1]` (deprecated) are carried so the tests can put a value there and see
//! it ignored. A `ts[2]` of zero is counted — [`Tally::rx_missing`],
//! [`Tally::tx_missing`] — and the request produces no figure. Nothing is
//! interpolated from a neighbour.
//!
//! Relative offsets are `u32` and wrap at 4 GiB of one direction's stream;
//! a run is megabytes.

use std::ops::Range;

/// TCP header flag bits, as they sit in byte 13 of the header.
pub const TCP_SYN: u8 = 0x02;
/// See [`TCP_SYN`].
pub const TCP_ACK: u8 = 0x10;

/// One inbound IPv4/TCP frame on the engine's port, as the tap recorded it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inbound {
    /// `scm_timestamping.ts[0..3]` in ns: software, deprecated, raw hardware.
    pub ts: [u64; 3],
    /// Source port — which connection.
    pub sport: u16,
    /// TCP flags byte.
    pub flags: u8,
    /// Acknowledgement number, absolute.
    pub ack: u32,
    /// TCP payload length.
    pub payload: u16,
}

/// One request: the frame that carried it, reduced to what pairing needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// The triple of the frame that completed the request.
    pub ts: [u64; 3],
    /// Engine bytes the client had acknowledged when it sent this request.
    pub acked: u32,
}

/// One TX stamp from the engine socket's error queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sent {
    /// `scm_timestamping.ts[0..3]` in ns.
    pub ts: [u64; 3],
    /// `ee_data`: 0-based offset of the last byte of that write.
    pub key: u32,
}

/// What [`pair`] counted over the window it was given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Requests in the window.
    pub requests: usize,
    /// Requests whose frame carried no hardware stamp (`ts[2] == 0`).
    pub rx_missing: usize,
    /// Requests whose reply had no error-queue entry, or one with `ts[2] == 0`.
    pub tx_missing: usize,
    /// Both stamps present and the reply's is **earlier** than the request's.
    /// Not a latency; counted, never folded into the figures.
    pub reversed: usize,
}

/// The hardware stamp of a triple, and the only place a triple is read.
///
/// `ts[2]` or nothing. **Never `ts[0]`.**
#[must_use]
pub const fn hw(ts: &[u64; 3]) -> Option<u64> {
    if ts[2] == 0 { None } else { Some(ts[2]) }
}

/// The IPv4/TCP fields a tap frame needs, from a `SOCK_DGRAM` packet socket's
/// buffer (network header at offset 0). `None` for anything that is not an
/// unfragmented IPv4 TCP segment to `dport` whose headers fit in `bytes`.
#[must_use]
pub fn parse_ipv4_tcp(bytes: &[u8], dport: u16, ts: [u64; 3]) -> Option<Inbound> {
    let b0 = *bytes.first()?;
    if b0 >> 4 != 4 || bytes.get(9) != Some(&6) {
        return None;
    }
    let ihl = usize::from(b0 & 0x0f) * 4;
    let tot = usize::from(u16::from_be_bytes([*bytes.get(2)?, *bytes.get(3)?]));
    let frag = u16::from_be_bytes([*bytes.get(6)?, *bytes.get(7)?]) & 0x1fff;
    if ihl < 20 || frag != 0 {
        return None;
    }
    let tcp = bytes.get(ihl..)?;
    let be16 =
        |i: usize| -> Option<u16> { Some(u16::from_be_bytes([*tcp.get(i)?, *tcp.get(i + 1)?])) };
    if be16(2)? != dport {
        return None;
    }
    let ack = u32::from_be_bytes([*tcp.get(8)?, *tcp.get(9)?, *tcp.get(10)?, *tcp.get(11)?]);
    let doff = usize::from(*tcp.get(12)? >> 4) * 4;
    let flags = *tcp.get(13)?;
    let payload = u16::try_from(tot.checked_sub(ihl + doff)?).ok()?;
    Some(Inbound {
        ts,
        sport: be16(0)?,
        flags,
        ack,
        payload,
    })
}

/// The requests of the connection from `peer`, in stream order.
///
/// The base is the `ack` of the first frame from `peer` that has `ACK` and not
/// `SYN` — the handshake's last packet, whose `ack` is the engine's ISN + 1,
/// the same `write_seq` the TX key counts from. Frames with no payload are not
/// requests. Consecutive payload frames with the same `acked` are one request
/// split across segments (or a retransmit), and the last one's stamp stands:
/// the engine could not answer before it arrived.
///
/// `None` if the handshake was not seen, which leaves nothing to count from.
pub fn requests(frames: &[Inbound], peer: u16, out: &mut Vec<Request>) -> Option<()> {
    let from = || frames.iter().filter(move |f| f.sport == peer);
    let base = from()
        .find(|f| f.flags & TCP_ACK != 0 && f.flags & TCP_SYN == 0)?
        .ack;
    for f in from().filter(|f| f.payload > 0) {
        let acked = f.ack.wrapping_sub(base);
        match out.last_mut() {
            Some(last) if last.acked == acked => last.ts = f.ts,
            _ => out.push(Request { ts: f.ts, acked }),
        }
    }
    Some(())
}

/// Pair each request in `window` with its reply and push every wire-in →
/// wire-out figure that both hardware stamps allow into `wire`, in ns.
///
/// `sent` must be sorted by key. `reqs` outside `window` are read only to
/// bound the last request inside it. See the module note for the rule.
pub fn pair(reqs: &[Request], window: Range<usize>, sent: &[Sent], wire: &mut Vec<u64>) -> Tally {
    let mut t = Tally::default();
    let mut j = 0;
    for k in window {
        let Some(r) = reqs.get(k) else { break };
        t.requests += 1;
        let hi = reqs.get(k + 1).map(|n| n.acked);
        while sent.get(j).is_some_and(|s| s.key < r.acked) {
            j += 1;
        }
        let reply = match hi {
            Some(hi) => sent[j..].iter().take_while(|s| s.key < hi).last(),
            None => sent.get(j),
        };
        let rx = hw(&r.ts);
        let tx = reply.and_then(|s| hw(&s.ts));
        if rx.is_none() {
            t.rx_missing += 1;
        }
        if tx.is_none() {
            t.tx_missing += 1;
        }
        if let (Some(a), Some(b)) = (rx, tx) {
            match b.checked_sub(a) {
                Some(d) => wire.push(d),
                None => t.reversed += 1,
            }
        }
    }
    t
}

#[cfg(test)]
fn frame(sport: u16, flags: u8, ack: u32, payload: u16, ts: [u64; 3]) -> Inbound {
    Inbound {
        ts,
        sport,
        flags,
        ack,
        payload,
    }
}

/// Three back-to-back round trips on one connection, with a second
/// connection's frames interleaved and a reply written in two pieces.
#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
fn pairs_in_order_one_in_flight() {
    const PSH_ACK: u8 = 0x18;
    let isn = 0xffff_ff00_u32; // the engine's ISN + 1 wraps inside the run
    let frames = [
        frame(40000, TCP_SYN, 0, 0, [0; 3]),
        frame(40000, TCP_ACK, isn, 0, [0; 3]),
        // Another connection on the same port: not this peer.
        frame(40001, TCP_ACK, 7, 0, [0; 3]),
        frame(40000, PSH_ACK, isn, 90, [0, 0, 1_000]),
        frame(40001, PSH_ACK, 7, 90, [0, 0, 1_500]),
        // Reply 0 was 50 bytes, reply 1 was 60: the client acked each whole.
        frame(40000, PSH_ACK, isn.wrapping_add(50), 90, [0, 0, 2_000]),
        frame(40000, PSH_ACK, isn.wrapping_add(110), 90, [0, 0, 3_000]),
        // A pure ACK after the last reply: not a request.
        frame(40000, TCP_ACK, isn.wrapping_add(160), 0, [0, 0, 3_900]),
    ];
    let mut reqs = Vec::new();
    requests(&frames, 40000, &mut reqs).unwrap();
    assert_eq!(
        reqs.iter().map(|r| r.acked).collect::<Vec<_>>(),
        [0, 50, 110],
        "acked must count from the handshake's ack, per peer"
    );
    let sent = [
        Sent {
            ts: [0, 0, 1_100],
            key: 49,
        },
        // Reply 1 in two writes: the last byte's stamp is the wire-out.
        Sent {
            ts: [0, 0, 2_100],
            key: 79,
        },
        Sent {
            ts: [0, 0, 2_250],
            key: 109,
        },
        Sent {
            ts: [0, 0, 3_400],
            key: 159,
        },
    ];
    let mut wire = Vec::new();
    let t = pair(&reqs, 0..3, &sent, &mut wire);
    assert_eq!(wire, [100, 250, 400], "each request against its own reply");
    assert_eq!(
        t,
        Tally {
            requests: 3,
            ..Tally::default()
        }
    );
    // A window that leaves the first request out still bounds by the stream.
    let mut tail = Vec::new();
    let _ = pair(&reqs, 1..3, &sent, &mut tail);
    assert_eq!(tail, [250, 400]);
}

/// Five round trips, 50-byte replies. Request 1's frame has no hardware stamp;
/// reply 2 never reached the error queue (the `igb` one-pending trap); reply 3
/// reached it with `ts[2] == 0`. Requests 0 and 4 must pair with their own
/// replies, and nothing may stand in for the three missing figures.
#[cfg(test)]
#[test]
fn a_missing_hw_stamp_is_counted_not_interpolated() {
    let reqs: Vec<Request> = [1_000, 0, 3_000, 4_000, 5_000]
        .iter()
        .zip(0u32..)
        .map(|(&hw, k)| Request {
            ts: [0, 0, hw],
            acked: 50 * k,
        })
        .collect();
    let sent = [
        Sent {
            ts: [0, 0, 1_100],
            key: 49,
        },
        Sent {
            ts: [0, 0, 2_200],
            key: 99,
        },
        // key 149 absent.
        Sent {
            ts: [0, 0, 0],
            key: 199,
        },
        Sent {
            ts: [0, 0, 5_500],
            key: 249,
        },
    ];
    let mut wire = Vec::new();
    let t = pair(&reqs, 0..5, &sent, &mut wire);
    assert_eq!(
        wire,
        [100, 500],
        "only requests 0 and 4 have both stamps; a shifted pairing would put reply 3 or 4 \
         against request 2 or 3"
    );
    assert_eq!(
        t,
        Tally {
            requests: 5,
            rx_missing: 1,
            tx_missing: 2,
            reversed: 0,
        }
    );
}

/// Every triple carries a software stamp in `ts[0]`. Where `ts[2]` is zero the
/// software value must not be used — not as the RX side, not as the TX side.
#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
fn never_mixes_software_into_the_hardware_column() {
    let reqs = [
        // hardware RX, software-only TX
        Request {
            ts: [10_000, 0, 1_000],
            acked: 0,
        },
        // software-only RX, hardware TX
        Request {
            ts: [20_000, 0, 0],
            acked: 50,
        },
        // both hardware
        Request {
            ts: [30_000, 0, 3_000],
            acked: 100,
        },
    ];
    let sent = [
        Sent {
            ts: [10_050, 0, 0],
            key: 49,
        },
        Sent {
            ts: [20_050, 0, 2_100],
            key: 99,
        },
        Sent {
            ts: [30_070, 0, 3_300],
            key: 149,
        },
    ];
    let mut wire = Vec::new();
    let t = pair(&reqs, 0..3, &sent, &mut wire);
    assert_eq!(wire, [300], "one figure, from ts[2] on both sides");
    assert_eq!((t.rx_missing, t.tx_missing), (1, 1));
    assert_eq!(hw(&[123, 456, 0]), None, "ts[0] and ts[1] are not hardware");
    // The tap's parser carries the triple untouched.
    let mut ip = [0u8; 40];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&40u16.to_be_bytes());
    ip[9] = 6;
    ip[20..22].copy_from_slice(&40000u16.to_be_bytes());
    ip[22..24].copy_from_slice(&9000u16.to_be_bytes());
    ip[32] = 0x50;
    ip[33] = TCP_ACK;
    let f = parse_ipv4_tcp(&ip, 9000, [77, 0, 0]).unwrap();
    assert_eq!((f.ts, f.payload, f.sport), ([77, 0, 0], 0, 40000));
    assert_eq!(hw(&f.ts), None);
}
