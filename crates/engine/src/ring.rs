//! A single-producer, single-consumer byte queue, for [`crate::dispatch::RingDispatch`].
//!
//! Records, not bytes: every `push` is one whole message and every `pop`
//! returns one whole message or nothing. A partial read would put the burden of
//! reassembly on the application thread, which is the thread this exists to
//! keep simple.
//!
//! # Why it is built out of `AtomicU8` and not a `memcpy`
//!
//! The obvious ring copies with `copy_from_slice` into a `Box<[u8]>` shared
//! between two threads, and that needs `UnsafeCell` and `unsafe impl Sync`.
//! `CLAUDE.md` non-negotiable 8 says `unsafe` needs a plan and a comment naming
//! what proves it sound, and the engine plan authorises neither `unsafe` here
//! nor a dependency that would supply the ring. So the buffer is a slice of
//! `AtomicU8`: **safe Rust, no dependency, and a byte-at-a-time copy.**
//!
//! That copy is the cost, it is real, and `benches/dispatch.rs` publishes it
//! rather than hiding it. It is also paid on the path that already accepted a
//! thread hop — an application willing to stall for milliseconds is not the one
//! counting nanoseconds. If the number ever matters, the replacement is an ADR
//! and a `unsafe` ring with a Miri run behind it, not a quiet edit here.
//!
//! # Ordering
//!
//! The producer writes the bytes with `Relaxed` and then publishes the new tail
//! with `Release`; the consumer reads the tail with `Acquire` and then the
//! bytes with `Relaxed`. That pair is what makes the bytes visible. The
//! consumer publishes its head the same way, and the producer acquires it to
//! find the free space.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// The length prefix in front of every record.
const HEADER: usize = 4;

/// The shared buffer. Never held by more than one producer and one consumer —
/// [`pair`] is the only way to make one and neither end is `Clone`.
struct Shared {
    buf: Box<[AtomicU8]>,
    /// Bytes written, ever. Index is this masked; it only ever goes up.
    tail: AtomicUsize,
    /// Bytes read, ever.
    head: AtomicUsize,
    mask: usize,
}

impl Shared {
    fn get(&self, at: usize) -> u8 {
        self.buf[at & self.mask].load(Ordering::Relaxed)
    }

    fn put(&self, at: usize, byte: u8) {
        self.buf[at & self.mask].store(byte, Ordering::Relaxed);
    }
}

/// The writing end. One thread, and the type system does not enforce that —
/// [`pair`] handing out exactly one is what does.
pub struct Producer {
    shared: Arc<Shared>,
}

/// The reading end.
pub struct Consumer {
    shared: Arc<Shared>,
}

/// Two ends of one queue, holding `capacity` bytes.
///
/// `capacity` is rounded **up** to a power of two, so indexing is a mask rather
/// than a division. A queue of zero is rounded up to one byte and then holds no
/// record at all, which is a caller error rather than a panic: every `push`
/// simply returns `false`.
///
/// The buffer is allocated here, once, and never again — the engine thread must
/// not allocate (non-negotiable 1).
/// What to pass [`pair`] unless you have measured a reason not to. 4 MiB.
///
/// [ADR-0011](../../../docs/decisions/ADR-0011-a-full-ring-disconnects.md)
/// decision 3, and it exists because the previous working figure did not buy
/// what the ring was chosen to buy. `[measured 2026-08-30]`
/// `crates/engine/benches/ring_full.rs`, Linux 6.18 x86_64, 4 vCPU container:
/// at `1 << 16` a stalled application fills the ring after **352 messages**, in
/// **56.7 µs**. ADR-0002 justified the ring on the argument that the session
/// layer should not stall when the application does, and priced its hop against
/// an application that "can pause for milliseconds" — **one millisecond
/// overruns 65 536 bytes about eighteen times over.**
///
/// At the same measured rate 4 MiB is roughly **3.6 ms** of slack. Two costs,
/// both real: 4 MiB resident per ring, which multiplies if a deployment ever
/// gives each connection its own; and the fill rate on a faster machine is
/// faster, so 3.6 ms is an upper estimate rather than a promise. ADR-0011's own
/// open question 3 is whether it is enough, and nobody has yet measured a real
/// application's worst pause.
///
/// **The benchmarks deliberately do not use this.** `benches/dispatch.rs` and
/// `benches/ring_full.rs` stay at `1 << 16`, because `DESIGN.md` §6's recorded
/// baselines were measured at that capacity and moving it would break a
/// comparison rather than improve one.
pub const DEFAULT_CAPACITY: usize = 1 << 22;

/// A ring, as a producer and a consumer that share one buffer.
#[must_use]
pub fn pair(capacity: usize) -> (Producer, Consumer) {
    let cap = capacity.next_power_of_two().max(1);
    let mut buf = Vec::with_capacity(cap);
    buf.resize_with(cap, || AtomicU8::new(0));
    let shared = Arc::new(Shared {
        buf: buf.into_boxed_slice(),
        tail: AtomicUsize::new(0),
        head: AtomicUsize::new(0),
        mask: cap - 1,
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared },
    )
}

impl Producer {
    /// How many bytes could still be written, ignoring the header.
    #[must_use]
    pub fn free(&self) -> usize {
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        self.shared.buf.len() - (tail.wrapping_sub(head))
    }

    /// One record, made of `parts` laid end to end.
    ///
    /// `false` means it did not fit and **nothing was written** — a record is
    /// all or nothing, because half a FIX message on the other thread is worse
    /// than no message at all. The caller decides what a refusal means;
    /// `RingDispatch` counts it.
    pub fn push(&mut self, parts: &[&[u8]]) -> bool {
        let len: usize = parts.iter().map(|p| p.len()).sum();
        let Ok(len32) = u32::try_from(len) else {
            return false;
        };
        if HEADER + len > self.free() {
            return false;
        }
        let mut at = self.shared.tail.load(Ordering::Relaxed);
        for byte in len32.to_le_bytes() {
            self.shared.put(at, byte);
            at += 1;
        }
        for part in parts {
            for byte in *part {
                self.shared.put(at, *byte);
                at += 1;
            }
        }
        // Release: everything above must be visible before the tail that
        // announces it.
        self.shared.tail.store(at, Ordering::Release);
        true
    }
}

impl Consumer {
    /// The next record, copied into `out`. `None` when the queue is empty.
    ///
    /// A record longer than `out` is **dropped**, and the queue advances past
    /// it: leaving it in place would wedge the queue forever on a message the
    /// consumer can never take. It is reported as `Some(0)` so a caller that
    /// cares can tell it apart from an empty queue.
    pub fn pop(&mut self, out: &mut [u8]) -> Option<usize> {
        let tail = self.shared.tail.load(Ordering::Acquire);
        let mut at = self.shared.head.load(Ordering::Relaxed);
        if tail.wrapping_sub(at) < HEADER {
            return None;
        }
        let mut header = [0u8; HEADER];
        for slot in &mut header {
            *slot = self.shared.get(at);
            at += 1;
        }
        let len = u32::from_le_bytes(header) as usize;
        if tail.wrapping_sub(at) < len {
            // A half-written record cannot happen — `push` publishes the tail
            // only after the whole record is in — so this is a corrupt queue
            // rather than a race. Treat it as empty and stop.
            return None;
        }
        if len > out.len() {
            self.shared.head.store(at + len, Ordering::Release);
            return Some(0);
        }
        for slot in out.iter_mut().take(len) {
            *slot = self.shared.get(at);
            at += 1;
        }
        self.shared.head.store(at, Ordering::Release);
        Some(len)
    }
}
