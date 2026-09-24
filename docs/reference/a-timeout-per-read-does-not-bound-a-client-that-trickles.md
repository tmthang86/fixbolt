# A timeout per read does not bound a client that trickles

`[2026-09-24]` senior review F1 of phase 4 row 1
([the metrics exporter plan](../plans/2026-09-24-p4-metrics-exporter.md),
[ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
decision 5).

**`set_read_timeout` bounds one `read`, not a connection.** The exporter's first version set it
once, to `read_timeout` (1 s), and then read until the headers ended. Each read that returned a
byte restarted the clock, so a client sending one byte every 700 ms was never timed out: it held
the exporter's only thread — every other scrape queued behind it, and `Exporter::stop()` waited
for it — for as long as it kept sending, up to the 4 KiB request cap (~68 min at that pace).

## What was seen

The reviewer's probe (a loris sending one byte every 700 ms for 10 s), reproduced by the manager
before the fix:

```text
loris sent 15 bytes over 10.50s
scrape during loris: 10.25s
stop() took 8.20s with a loris connected
```

and after it, same probe, `tmt-B450-I-AORUS-PRO-WIFI`, desktop grub line, loopback:

```text
scrape during loris: (857.644343ms, "HTTP/1.1 503 Service Unavailable")
stop() took 901.192592ms with a loris connected
```

## The fix

One deadline per connection, set at accept to `read_timeout` from now. Before every read and
every write the socket's timeout is set to what is left of it (`http::remaining`), and a passed
deadline drops the connection — `read_request`, `write_by`, and the refusal drain in
`crates/metrics/src/http.rs` and `thread.rs`. The answer loop takes at most 64 connections per
wake and drains the event stream between every two, so a flood of connections cannot starve the
events either.

## What guards it

`crates/metrics/tests/exporter.rs::a_client_that_trickles_bytes_is_dropped_at_the_deadline`:
a client sends a byte every 100 ms against a 300 ms `read_timeout`; it must be dropped within
1 s, a scrape queued behind it must be answered within 1 s, and `stop()` with a second such
client connected must return within 1 s. Red before the fix: "held 3.209306616s".

## What it does not change

The exporter still answers one connection at a time. N clients that each say nothing delay a
scrape queued behind them by up to N × `read_timeout` — the same probe's "scrape behind 8 silent
conns" read 8.27 s after the fix. That is ADR-0170's one-thread design; a deployment exposing
the port to anything but its own Prometheus is outside what the exporter is built for (GUIDE
§8a: loopback or a private interface).
