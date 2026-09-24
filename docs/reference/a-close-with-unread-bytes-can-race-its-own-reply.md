# A close with unread bytes can race its own reply

`[2026-09-24]` found while writing `crates/metrics/tests/exporter.rs::a_request_larger_than_4_kib_is_refused_not_buffered`,
phase 4 row 1 steps 3–4 of
[the metrics exporter plan](../plans/2026-09-24-p4-metrics-exporter.md)
([ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
decision 6).

**Closing a TCP socket that still has unread bytes sitting in its receive buffer makes the
kernel send a reset (`RST`) instead of an orderly close — and the reset can reach the client
before, or instead of, a reply the exporter already wrote and flushed.** A client that sent a
request larger than the exporter's 4 KiB cap can therefore see its connection reset with no
data at all, rather than the `400 Bad Request` the exporter meant to answer with.

## The mechanism

[`crates/metrics/src/http.rs`](../../crates/metrics/src/http.rs) reads at most `REQUEST_CAP`
(4 KiB) bytes looking for the end of the request headers. A client that sends more than that
before the exporter has read it all leaves bytes unread on the socket. `Loop::refuse`
([`crates/metrics/src/thread.rs`](../../crates/metrics/src/thread.rs)) writes the `400` reply,
shuts the write half, and — until the fix below — would simply drop the `TcpStream`. Dropping a
socket with data still unread in its receive queue is exactly the condition POSIX sockets (and
the Linux kernel) answer with `RST` on close, rather than a clean `FIN`: from the client's
point of view an `RST` can overtake bytes the server already sent, because the kernel does not
guarantee the reset arrives only after everything already written has been delivered.

## The fix, in code

`Loop::refuse` drains the socket for a short, bounded time before dropping it:

```rust
// Closing with unread bytes makes the kernel send a reset, which can
// arrive before the answer is read. A short, bounded drain lets the
// answer land; the bytes themselves are thrown away.
if s.set_read_timeout(Some(Duration::from_millis(10))).is_ok() {
    for _ in 0..4 {
        match s.read(self.request.as_mut_slice()) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}
```

Four reads at 10 ms each is a bound of 40 ms on a connection the exporter has already decided
to refuse — small next to the 1 s default `read_timeout` a well-behaved request gets.

## What guards it, and how far the guard goes

`tests/exporter.rs::a_request_larger_than_4_kib_is_refused_not_buffered` sends 5 KiB of request
line and reads the reply to completion. **It does not assert that the `400` wins the race** —
it accepts either outcome:

```rust
// The refusal may arrive, or the connection may be reset under it when the
// exporter closes with bytes unread; both are a refusal. What must not
// happen is a 200.
let _ = s.read_to_end(&mut reply);
let reply = String::from_utf8_lossy(&reply);
assert!(
    reply.is_empty() || reply.starts_with("HTTP/1.1 400 Bad Request\r\n"),
    "{reply}"
);
```

So the test proves the oversized request never gets a `200` and that
`fixbolt_exporter_bad_requests_total` still counts it — it does **not** prove the drain
reliably lets the `400` win. Removing the drain entirely would very likely still pass this
test, since an empty reply is accepted too. The guard against a hang or a wrong status exists;
a guard proving the reply is delivered more often than not does not.

## Sources

- [`crates/metrics/src/thread.rs`](../../crates/metrics/src/thread.rs) — `Loop::refuse`.
- [`crates/metrics/tests/exporter.rs`](../../crates/metrics/tests/exporter.rs) —
  `a_request_larger_than_4_kib_is_refused_not_buffered`.
- `tcp(7)`, Linux man-pages: a socket closed with data in the receive queue sends `RST`.
